-- SnugOM get_or_create script
-- Atomically returns existing entity if it exists, or creates it if not.
-- No race conditions - existence check and mutation happen in single Redis call.
--
-- Arguments:
--  KEYS[1] - placeholder (unused; commands rely on explicit keys)
--  ARGV[1] - JSON payload describing GetOrCreate command
--  ARGV[2] - script version for optimistic upgrades (unused for now)

local cjson = cjson

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(parts, part)
    end
    return parts
end

local function encode_result(result)
    return cjson.encode(result)
end

-- Idempotency handling
local IDEMPOTENCY_TTL_SECONDS = 900

local function check_idempotency(key, idempotency_key)
    if not idempotency_key then
        return nil
    end
    local parts = split_key(key)
    if #parts < 3 then
        return nil
    end
    local store_key = table.concat({ parts[1], parts[2], "idempotency", idempotency_key }, ":")
    local existing = redis.call("GET", store_key)
    if existing then
        return existing
    end
    return nil
end

local function store_idempotency(key, idempotency_key, response, ttl)
    if not idempotency_key then
        return
    end
    local parts = split_key(key)
    if #parts < 3 then
        return
    end
    local store_key = table.concat({ parts[1], parts[2], "idempotency", idempotency_key }, ":")
    if ttl and ttl > 0 then
        redis.call("SET", store_key, response, "EX", ttl)
    else
        redis.call("SET", store_key, response, "EX", IDEMPOTENCY_TTL_SECONDS)
    end
end

-- Check unique constraint (simplified version from upsert)
local function check_unique_constraint(constraint, entity_id, prefix, service, collection)
    local fields = constraint["fields"]
    local values = constraint["values"]
    local case_insensitive = constraint["case_insensitive"] == true

    -- Build lookup value
    local lookup_parts = {}
    local has_null = false
    for i = 1, #fields do
        local v = values[i]
        if v == nil or v == cjson.null then
            has_null = true
            break
        end
        if case_insensitive and type(v) == "string" then
            v = string.lower(v)
        end
        table.insert(lookup_parts, tostring(v))
    end

    if has_null then
        return nil, nil, nil
    end

    local lookup_value = table.concat(lookup_parts, "|")
    local unique_key = table.concat({ prefix, service, "unique", collection, table.concat(fields, "_") }, ":")

    -- Check if this value already exists
    local existing_id = redis.call("HGET", unique_key, lookup_value)
    if existing_id and existing_id ~= entity_id then
        return {
            error = "unique_constraint_violation",
            fields = fields,
            values = values,
            existing_entity_id = existing_id,
        }, nil, nil
    end

    return nil, unique_key, lookup_value
end

-- Apply datetime mirrors (copy from upsert)
local function apply_datetime_mirrors(key, datetime_mirrors)
    for i = 1, #datetime_mirrors do
        local mirror = datetime_mirrors[i]
        if mirror["mirror_field"] ~= nil then
            if mirror["value"] == cjson.null or mirror["value"] == nil then
                redis.call("JSON.DEL", key, "$." .. mirror["mirror_field"])
            else
                redis.call("JSON.SET", key, "$." .. mirror["mirror_field"], cjson.encode(mirror["value"]))
            end
        end
    end
end

-- Apply relations (simplified from upsert)
local function apply_relations(relations, prefix, service)
    for i = 1, #relations do
        local rel = relations[i]
        local op = rel["op"]
        local rel_key = rel["key"]
        local entity_id = rel["entity_id"]
        local target_id = rel["target_id"]

        if op == "connect" then
            redis.call("SADD", rel_key, target_id)
        end
    end
end

-- Main get_or_create logic
local function main()
    local payload = cjson.decode(ARGV[1])
    local get_or_create = payload["get_or_create"]
    if get_or_create == nil then
        return encode_result({ error = "invalid_payload", message = "expected GetOrCreate" })
    end

    local entity_key = get_or_create["entity_key"]
    local entity_id = get_or_create["entity_id"]
    local idempotency_key = get_or_create["idempotency_key"]
    local idempotency_ttl = get_or_create["idempotency_ttl"]

    -- Check idempotency first
    local cached = check_idempotency(entity_key, idempotency_key)
    if cached then
        return cached
    end

    -- Parse key structure
    local key_parts = split_key(entity_key)
    local prefix = key_parts[1]
    local service = key_parts[2]
    local collection = key_parts[3]

    -- Check if the entity exists
    local exists = redis.call("EXISTS", entity_key) == 1

    local response
    if exists then
        -- =====================
        -- FOUND PATH - Just return the existing entity
        -- =====================
        local entity_json = redis.call("JSON.GET", entity_key, "$")
        if not entity_json or entity_json == false then
            return encode_result({ error = "internal_error", message = "entity exists but could not be read" })
        end

        response = encode_result({
            ok = true,
            branch = "found",
            entity_id = entity_id,
            entity = cjson.decode(entity_json),
        })
    else
        -- =====================
        -- CREATE PATH
        -- =====================
        local create_payload_json = get_or_create["create_payload_json"]
        local unique_constraints = get_or_create["unique_constraints"] or {}
        local relations = get_or_create["relations"] or {}
        local datetime_mirrors = get_or_create["datetime_mirrors"] or {}

        if create_payload_json == nil then
            return encode_result({ error = "invalid_payload", message = "create_payload_json is required" })
        end

        -- Check unique constraints
        local unique_updates = {}
        for i = 1, #unique_constraints do
            local constraint = unique_constraints[i]
            local violation, unique_key, lookup_value = check_unique_constraint(
                constraint, entity_id, prefix, service, collection
            )
            if violation then
                return encode_result(violation)
            end
            if unique_key and lookup_value then
                table.insert(unique_updates, {
                    unique_key = unique_key,
                    lookup_value = lookup_value,
                    entity_id = entity_id,
                })
            end
        end

        -- Create the entity
        redis.call("JSON.SET", entity_key, "$", create_payload_json)

        -- Set version
        redis.call("JSON.SET", entity_key, "$.metadata.version", 1)
        redis.call("PERSIST", entity_key)

        -- Register unique constraint values
        for i = 1, #unique_updates do
            local update = unique_updates[i]
            redis.call("HSET", update.unique_key, update.lookup_value, update.entity_id)
        end

        -- Apply datetime mirrors
        apply_datetime_mirrors(entity_key, datetime_mirrors)

        -- Apply relations
        apply_relations(relations, prefix, service)

        -- Re-read the created entity to return it
        local created_json = redis.call("JSON.GET", entity_key, "$")
        local created_entity = nil
        if created_json and created_json ~= false then
            created_entity = cjson.decode(created_json)
        end

        response = encode_result({
            ok = true,
            branch = "created",
            version = 1,
            entity_id = entity_id,
            entity = created_entity,
        })
    end

    -- Store idempotency
    store_idempotency(entity_key, idempotency_key, response, idempotency_ttl)

    return response
end

return main()
