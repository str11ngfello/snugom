-- SnugOM entity mutation script (create/update)
-- Arguments:
--  KEYS[1] - placeholder (unused; commands rely on explicit keys)
--  ARGV[1] - JSON payload describing MutationCommand::UpsertEntity
--  ARGV[2] - script version for optimistic upgrades (unused for now)

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(parts, part)
    end
    return parts
end

local payload = cjson.decode(ARGV[1])
local mutation = payload["upsert_entity"]
if mutation == nil then
    return cjson.encode({ error = "invalid_payload", message = "expected UpsertEntity" })
end

local key = mutation["key"]
local expected_version = mutation["expected_version"]
local idempotency_key = mutation["idempotency_key"]
local datetime_mirrors = mutation["datetime_mirrors"] or {}
local relations = mutation["relations"] or {}

-- Key structure: {prefix}:{service}:{collection}:{entity_id}
local key_parts = {}
for part in string.gmatch(key, "([^:]+)") do
    table.insert(key_parts, part)
end

local prefix = key_parts[1]
local service = key_parts[2]
-- key_parts[3] is collection, key_parts[4] is entity_id

local idempotency_store_key = nil
local IDEMPOTENCY_TTL_SECONDS = 900
local idempotency_ttl = nil

if idempotency_key ~= nil then
    idempotency_store_key = table.concat({ prefix, service, "idempotency", idempotency_key }, ":")
    local existing = redis.call("GET", idempotency_store_key)
    if existing then
        return existing
    end
    idempotency_ttl = mutation["idempotency_ttl"]
    if idempotency_ttl ~= nil then
        if type(idempotency_ttl) ~= "number" or idempotency_ttl < 0 then
            idempotency_ttl = IDEMPOTENCY_TTL_SECONDS
        elseif idempotency_ttl == 0 then
            idempotency_ttl = nil
        end
    else
        idempotency_ttl = IDEMPOTENCY_TTL_SECONDS
    end
end

local stored_version_raw = redis.call("JSON.GET", key, "$.metadata.version")
local stored_version = nil
if stored_version_raw ~= nil then
    if stored_version_raw == cjson.null or type(stored_version_raw) == "boolean" then
        stored_version_raw = nil
    end
end
if stored_version_raw ~= nil then
    local decoded = cjson.decode(stored_version_raw)
    if type(decoded) == "table" then
        decoded = decoded[1]
    end
    if type(decoded) == "string" then
        decoded = tonumber(decoded)
    end
    stored_version = decoded
end

if expected_version ~= nil then
    if stored_version == nil or stored_version ~= expected_version then
        return cjson.encode({
            error = "version_conflict",
            expected = expected_version,
            actual = stored_version or cjson.null,
        })
    end
end

local base_version = stored_version or 0
local next_version = base_version + 1

local payload_json = mutation["payload_json"]
local entity_id = mutation["entity_id"]

if payload_json == nil then
    return cjson.encode({ error = "invalid_payload", message = "payload_json is required" })
end
if entity_id == nil or entity_id == cjson.null then
    entity_id = key_parts[#key_parts]
end

-- Unique constraint enforcement
-- Structure: unique_constraints is an array of {fields: ["name"], case_insensitive: bool, values: ["value"]}
local unique_constraints = mutation["unique_constraints"] or {}
local collection = key_parts[3]

-- Track unique keys we need to update (old values to remove, new values to add)
local unique_updates = {}

for i = 1, #unique_constraints do
    local constraint = unique_constraints[i]
    local fields = constraint["fields"]
    local case_insensitive = constraint["case_insensitive"] == true
    local values = constraint["values"]  -- array of values matching fields order

    -- Build lookup key from all field values
    local lookup_parts = {}
    local has_null = false
    for j = 1, #values do
        local v = values[j]
        if v == nil or v == cjson.null then
            -- Skip constraint check if any field is null
            has_null = true
            break
        end
        if case_insensitive and type(v) == "string" then
            v = string.lower(v)
        end
        table.insert(lookup_parts, tostring(v))
    end

    if not has_null then
        local lookup_value = table.concat(lookup_parts, ":")

        -- Build unique index key
        local unique_key
        if #fields == 1 then
            -- Single field: {prefix}:{service}:{collection}:unique:{field}
            unique_key = table.concat({ prefix, service, collection, "unique", fields[1] }, ":")
        else
            -- Compound: {prefix}:{service}:{collection}:unique_compound:{field1}_{field2}
            local field_suffix = table.concat(fields, "_")
            unique_key = table.concat({ prefix, service, collection, "unique_compound", field_suffix }, ":")
        end

        -- Check if value already taken by DIFFERENT entity
        local existing_id = redis.call("HGET", unique_key, lookup_value)
        if existing_id ~= nil and existing_id ~= false and existing_id ~= entity_id then
            return cjson.encode({
                error = "unique_constraint_violation",
                fields = fields,
                values = constraint["values"],
                existing_entity_id = existing_id,
            })
        end

        -- Track this for update after JSON.SET succeeds
        table.insert(unique_updates, {
            unique_key = unique_key,
            lookup_value = lookup_value,
            entity_id = entity_id,
        })
    end
end

redis.call("JSON.SET", key, "$", payload_json)

-- Now reserve all unique values atomically
for i = 1, #unique_updates do
    local update = unique_updates[i]
    redis.call("HSET", update.unique_key, update.lookup_value, update.entity_id)
end
redis.call("JSON.SET", key, "$.metadata.version", next_version)

redis.call("PERSIST", key)

for i = 1, #datetime_mirrors do
    local mirror = datetime_mirrors[i]
    if mirror["mirror_field"] ~= nil then
        if mirror["value"] == cjson.null or mirror["value"] == nil then
            redis.call("JSON.DEL", key, "$." .. mirror["mirror_field"])
        else
            redis.call(
                "JSON.SET",
                key,
                "$." .. mirror["mirror_field"],
                cjson.encode(mirror["value"])
            )
        end
    end
end

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
        -- Relation key structure: {prefix}:{service}:rel:{alias}:{left_id}
        relation_parts = split_key(relation_key)
        rel_prefix = relation_parts[1]
        rel_service = relation_parts[2]
        -- relation_parts[3] is "rel"
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

local response = {
    ok = true,
    version = next_version,
    entity_id = entity_id,
    datetime_mirrors = datetime_mirrors,
}

local encoded = cjson.encode(response)

if idempotency_store_key ~= nil then
    if idempotency_ttl ~= nil then
        redis.call("SET", idempotency_store_key, encoded, "EX", idempotency_ttl)
    else
        redis.call("SET", idempotency_store_key, encoded)
    end
end

return encoded
