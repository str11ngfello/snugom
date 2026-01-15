-- SnugOM upsert script
-- Atomically creates entity if not exists, or updates if exists.
-- No race conditions - existence check and mutation happen in single Redis call.
--
-- Arguments:
--  KEYS[1] - placeholder (unused; commands rely on explicit keys)
--  ARGV[1] - JSON payload describing Upsert command
--  ARGV[2] - script version for optimistic upgrades (unused for now)

local cjson = cjson

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(parts, part)
    end
    return parts
end

local function normalize_version(value)
    if value == cjson.null then
        return nil
    end
    if type(value) == "table" then
        return normalize_version(value[1])
    end
    if type(value) == "string" then
        local num = tonumber(value)
        if num then
            return num
        end
        return value
    end
    return value
end

local function load_current_version(key)
    local raw = redis.call("JSON.GET", key, "$.metadata.version")
    if not raw then
        return nil
    end
    local decoded, err = cjson.decode(raw)
    if decoded == nil and err ~= nil then
        return nil, err
    end
    return normalize_version(decoded), nil
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

-- Apply a single patch operation
local function apply_operation(key, op)
    local path = op["path"]
    local op_type = op["type"]
    local value_json = op["value_json"]

    if op_type == "assign" then
        if value_json == nil then
            return { error = "invalid_payload", message = "value_json is required for assign" }
        end
        redis.call("JSON.SET", key, path, value_json)
    elseif op_type == "merge" then
        if value_json == nil then
            return { error = "invalid_payload", message = "value_json is required for merge" }
        end
        redis.call("JSON.MERGE", key, path, value_json)
    elseif op_type == "delete" then
        redis.call("JSON.DEL", key, path)
    elseif op_type == "increment" then
        local number_value = op["value"]
        if type(number_value) == "table" then
            number_value = number_value[1]
        end
        if type(number_value) == "string" then
            number_value = tonumber(number_value)
        end
        redis.call("JSON.NUMINCRBY", key, path, number_value)
    else
        return { error = "unknown_operation", message = "unknown patch operation type: " .. tostring(op_type) }
    end
    return nil
end

-- Apply datetime mirror field
local function apply_mirror(key, mirror)
    if not mirror then
        return
    end
    local mirror_field = mirror["mirror_field"]
    local value_json = mirror["mirror_value_json"]
    if value_json == nil or value_json == "null" then
        redis.call("JSON.DEL", key, "$." .. mirror_field)
    else
        redis.call("JSON.SET", key, "$." .. mirror_field, value_json)
    end
end

-- Apply relation mutations
local function apply_relations(relations, prefix, service)
    for i = 1, #relations do
        local relation = relations[i]
        local relation_key = relation["relation_key"]
        local add = relation["add"] or {}
        local remove = relation["remove"] or {}
        local maintain_reverse = relation["maintain_reverse"] == true

        local relation_parts
        local rel_prefix
        local rel_service
        local alias
        local left_id
        local reverse_alias

        if maintain_reverse then
            relation_parts = split_key(relation_key)
            rel_prefix = relation_parts[1]
            rel_service = relation_parts[2]
            alias = relation_parts[4]
            left_id = relation_parts[5]
            reverse_alias = alias .. "_reverse"
        end

        if #add > 0 then
            redis.call("SADD", relation_key, unpack(add))
            if maintain_reverse then
                for j = 1, #add do
                    local member_id = add[j]
                    local reverse_key = table.concat({ rel_prefix, rel_service, "rel", reverse_alias, member_id }, ":")
                    redis.call("SADD", reverse_key, left_id)
                end
            end
        end

        if #remove > 0 then
            redis.call("SREM", relation_key, unpack(remove))
            if maintain_reverse then
                for j = 1, #remove do
                    local member_id = remove[j]
                    local reverse_key = table.concat({ rel_prefix, rel_service, "rel", reverse_alias, member_id }, ":")
                    redis.call("SREM", reverse_key, left_id)
                    if redis.call("SCARD", reverse_key) == 0 then
                        redis.call("DEL", reverse_key)
                    end
                end
            end
        end
    end
end

-- Apply datetime mirrors for create path
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

-- Check unique constraint and return violation if found
local function check_unique_constraint(constraint, entity_id, prefix, service, collection)
    local fields = constraint["fields"]
    local case_insensitive = constraint["case_insensitive"] == true
    local values = constraint["values"]

    local lookup_parts = {}
    local has_null = false
    for j = 1, #values do
        local v = values[j]
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

    local lookup_value = table.concat(lookup_parts, ":")
    local unique_key
    if #fields == 1 then
        unique_key = table.concat({ prefix, service, collection, "unique", fields[1] }, ":")
    else
        local field_suffix = table.concat(fields, "_")
        unique_key = table.concat({ prefix, service, collection, "unique_compound", field_suffix }, ":")
    end

    local existing_id = redis.call("HGET", unique_key, lookup_value)
    if existing_id ~= nil and existing_id ~= false and existing_id ~= entity_id then
        return {
            error = "unique_constraint_violation",
            fields = fields,
            values = constraint["values"],
            existing_entity_id = existing_id,
        }, nil, nil
    end

    return nil, unique_key, lookup_value
end

-- Main upsert logic
local function main()
    local payload = cjson.decode(ARGV[1])
    local upsert = payload["upsert"]
    if upsert == nil then
        return encode_result({ error = "invalid_payload", message = "expected Upsert" })
    end

    -- Update path uses these (for existence check)
    local update_key = upsert["update_key"]
    local update_entity_id = upsert["update_entity_id"]

    -- Create path uses these (different entity may be created)
    local create_key = upsert["create_key"]
    local create_entity_id = upsert["create_entity_id"]

    local idempotency_key = upsert["idempotency_key"]
    local idempotency_ttl = upsert["idempotency_ttl"]

    -- Check idempotency first (use update key for consistency)
    local cached = check_idempotency(update_key, idempotency_key)
    if cached then
        return cached
    end

    -- Parse key structure from update_key
    local key_parts = split_key(update_key)
    local prefix = key_parts[1]
    local service = key_parts[2]
    local collection = key_parts[3]

    -- Check if the entity to UPDATE exists
    local exists = redis.call("EXISTS", update_key) == 1

    local response
    if exists then
        -- =====================
        -- UPDATE PATH
        -- =====================
        local update_operations = upsert["update_operations"] or {}
        local update_relations = upsert["update_relations"] or {}
        local update_unique_constraints = upsert["update_unique_constraints"] or {}

        -- Handle unique constraint enforcement for updates
        local unique_updates = {}
        if #update_unique_constraints > 0 then
            local entity_json = redis.call("JSON.GET", update_key, "$")
            local entity_data = nil
            if entity_json ~= nil and entity_json ~= false then
                local decoded = cjson.decode(entity_json)
                if type(decoded) == "table" and decoded[1] ~= nil then
                    entity_data = decoded[1]
                else
                    entity_data = decoded
                end
            end

            for i = 1, #update_unique_constraints do
                local constraint = update_unique_constraints[i]
                local fields = constraint["fields"]
                local case_insensitive = constraint["case_insensitive"] == true
                local new_values = constraint["values"]

                -- Build old lookup value (from current entity)
                local old_lookup_parts = {}
                local old_has_null = false
                if entity_data ~= nil then
                    for j = 1, #fields do
                        local v = entity_data[fields[j]]
                        if v == nil or v == cjson.null then
                            old_has_null = true
                            break
                        end
                        if case_insensitive and type(v) == "string" then
                            v = string.lower(v)
                        end
                        table.insert(old_lookup_parts, tostring(v))
                    end
                else
                    old_has_null = true
                end

                -- Build new lookup value
                local new_lookup_parts = {}
                local new_has_null = false
                local final_values = {}
                for j = 1, #fields do
                    local v = new_values[j]
                    if v == nil or v == cjson.null then
                        if entity_data ~= nil then
                            v = entity_data[fields[j]]
                        end
                    end
                    if v == nil or v == cjson.null then
                        new_has_null = true
                        break
                    end
                    table.insert(final_values, v)
                    if case_insensitive and type(v) == "string" then
                        v = string.lower(v)
                    end
                    table.insert(new_lookup_parts, tostring(v))
                end

                if not new_has_null then
                    local new_lookup_value = table.concat(new_lookup_parts, ":")
                    local old_lookup_value = nil
                    if not old_has_null then
                        old_lookup_value = table.concat(old_lookup_parts, ":")
                    end

                    if old_lookup_value ~= new_lookup_value then
                        local unique_key
                        if #fields == 1 then
                            unique_key = table.concat({ prefix, service, collection, "unique", fields[1] }, ":")
                        else
                            local field_suffix = table.concat(fields, "_")
                            unique_key = table.concat({ prefix, service, collection, "unique_compound", field_suffix }, ":")
                        end

                        local existing_id = redis.call("HGET", unique_key, new_lookup_value)
                        if existing_id ~= nil and existing_id ~= false and existing_id ~= update_entity_id then
                            return encode_result({
                                error = "unique_constraint_violation",
                                fields = fields,
                                values = final_values,
                                existing_entity_id = existing_id,
                            })
                        end

                        table.insert(unique_updates, {
                            unique_key = unique_key,
                            old_lookup_value = old_lookup_value,
                            new_lookup_value = new_lookup_value,
                            entity_id = update_entity_id,
                        })
                    end
                end
            end
        end

        -- Apply operations
        for _, op in ipairs(update_operations) do
            local op_result = apply_operation(update_key, op)
            if op_result ~= nil and op_result["error"] ~= nil then
                return encode_result(op_result)
            end
            local mirror = op["mirror"]
            if mirror then
                apply_mirror(update_key, mirror)
            end
        end

        -- Apply relations
        apply_relations(update_relations, prefix, service)

        -- Increment version
        local current_version = load_current_version(update_key)
        local next_version
        if current_version == nil or type(current_version) ~= "number" then
            next_version = 1
        else
            next_version = current_version + 1
        end
        redis.call("JSON.SET", update_key, "$.metadata.version", next_version)
        redis.call("PERSIST", update_key)

        -- Update unique indexes
        for i = 1, #unique_updates do
            local update = unique_updates[i]
            if update.old_lookup_value ~= nil then
                redis.call("HDEL", update.unique_key, update.old_lookup_value)
            end
            redis.call("HSET", update.unique_key, update.new_lookup_value, update.entity_id)
        end

        response = encode_result({
            ok = true,
            branch = "updated",
            version = next_version,
            entity_id = update_entity_id,
        })
    else
        -- =====================
        -- CREATE PATH
        -- =====================
        local create_payload_json = upsert["create_payload_json"]
        local create_unique_constraints = upsert["create_unique_constraints"] or {}
        local create_relations = upsert["create_relations"] or {}
        local datetime_mirrors = upsert["datetime_mirrors"] or {}

        if create_payload_json == nil then
            return encode_result({ error = "invalid_payload", message = "create_payload_json is required" })
        end

        -- Parse create key structure (may differ from update key)
        local create_key_parts = split_key(create_key)
        local create_prefix = create_key_parts[1]
        local create_service = create_key_parts[2]
        local create_collection = create_key_parts[3]

        -- Check unique constraints
        local unique_updates = {}
        for i = 1, #create_unique_constraints do
            local constraint = create_unique_constraints[i]
            local violation, unique_key, lookup_value = check_unique_constraint(
                constraint, create_entity_id, create_prefix, create_service, create_collection
            )
            if violation then
                return encode_result(violation)
            end
            if unique_key and lookup_value then
                table.insert(unique_updates, {
                    unique_key = unique_key,
                    lookup_value = lookup_value,
                    entity_id = create_entity_id,
                })
            end
        end

        -- Create the entity
        redis.call("JSON.SET", create_key, "$", create_payload_json)

        -- Set version
        redis.call("JSON.SET", create_key, "$.metadata.version", 1)
        redis.call("PERSIST", create_key)

        -- Register unique constraint values
        for i = 1, #unique_updates do
            local update = unique_updates[i]
            redis.call("HSET", update.unique_key, update.lookup_value, update.entity_id)
        end

        -- Apply datetime mirrors
        apply_datetime_mirrors(create_key, datetime_mirrors)

        -- Apply relations
        apply_relations(create_relations, create_prefix, create_service)

        response = encode_result({
            ok = true,
            branch = "created",
            version = 1,
            entity_id = create_entity_id,
        })
    end

    -- Store idempotency (use update key for consistency)
    store_idempotency(update_key, idempotency_key, response, idempotency_ttl)

    return response
end

return main()
