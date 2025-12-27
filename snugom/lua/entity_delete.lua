-- SnugOM entity delete script
-- Arguments:
--  ARGV[1] - JSON payload describing MutationCommand::DeleteEntity

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(parts, part)
    end
    return parts
end

local function compute_child_relations(child_specs, prefix, service, entity_id)
    local result = {}
    for i = 1, #child_specs do
        local spec = child_specs[i]
        local child_service = spec["target_service"]
        if child_service == nil then
            child_service = service
        end
        local child_alias = spec["alias"]
        local child_relation_key = table.concat({ prefix, child_service, "rel", child_alias, entity_id }, ":")
        local nested = compute_child_relations(spec["child_relations"] or {}, prefix, child_service, entity_id)
        table.insert(result, {
            alias = child_alias,
            relation_key = child_relation_key,
            target_collection = spec["target_collection"],
            target_service = spec["target_service"],
            cascade = spec["cascade"],
            maintain_reverse = spec["maintain_reverse"] == true,
            child_relations = nested,
        })
    end
    return result
end

local function delete_with_relations(key, expected_version, relations, unique_constraints)
    unique_constraints = unique_constraints or {}

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
        local actual = stored_version
        if actual == nil or actual ~= expected_version then
            if actual == nil then
                actual = cjson.null
            end
            return {
                error = "version_conflict",
                expected = expected_version,
                actual = actual,
            }
        end
    end

    -- Key structure: {prefix}:{service}:{collection}:{entity_id}
    local key_parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(key_parts, part)
    end

    local prefix = key_parts[1]
    local service = key_parts[2]
    local collection = key_parts[3]

    -- Clean up unique constraint indexes before deleting the entity
    if #unique_constraints > 0 then
        -- Read the entity to get current field values
        local entity_json = redis.call("JSON.GET", key, "$")
        if entity_json ~= nil and entity_json ~= false then
            local entity_data = cjson.decode(entity_json)
            if type(entity_data) == "table" and entity_data[1] ~= nil then
                entity_data = entity_data[1]
            end

            for i = 1, #unique_constraints do
                local constraint = unique_constraints[i]
                local fields = constraint["fields"]
                local case_insensitive = constraint["case_insensitive"] == true

                -- Build lookup key from field values
                local lookup_parts = {}
                local has_null = false
                for j = 1, #fields do
                    local v = entity_data[fields[j]]
                    if v == nil or v == cjson.null then
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
                        unique_key = table.concat({ prefix, service, collection, "unique", fields[1] }, ":")
                    else
                        local field_suffix = table.concat(fields, "_")
                        unique_key = table.concat({ prefix, service, collection, "unique_compound", field_suffix }, ":")
                    end

                    -- Remove the entry from the unique index
                    redis.call("HDEL", unique_key, lookup_value)
                end
            end
        end
    end

    redis.call("DEL", key)

    for i = 1, #relations do
        local relation = relations[i]
        local cascade = relation["cascade"]
        local relation_key = relation["relation_key"]
        local maintain_reverse = relation["maintain_reverse"] == true
        local relation_parts
        local alias
        local left_id
        local reverse_alias

        if maintain_reverse then
            -- Relation key structure: {prefix}:{service}:rel:{alias}:{left_id}
            relation_parts = split_key(relation_key)
            alias = relation_parts[4]
            left_id = relation_parts[5]
            reverse_alias = alias .. "_reverse"
        end

        if cascade == "delete_dependents" then
            local members = redis.call("SMEMBERS", relation_key)
            local target_collection = relation["target_collection"]
            local target_service = relation["target_service"]
            if target_service == nil then
                target_service = service
            end
            local child_specs = relation["child_relations"] or {}
            if target_collection ~= nil then
                for j = 1, #members do
                    local member_id = members[j]
                    local child_key = table.concat({ prefix, target_service, target_collection, member_id }, ":")
                    local child_relations_payload = compute_child_relations(child_specs, prefix, target_service, member_id)
                    -- Unique constraints for child entities are passed through the relation info
                    local child_unique_constraints = relation["unique_constraints"] or {}
                    local result = delete_with_relations(child_key, nil, child_relations_payload, child_unique_constraints)
                    if result["error"] ~= nil then
                        return result
                    end
                end
            end
            if maintain_reverse then
                for j = 1, #members do
                    local member_id = members[j]
                    local reverse_key = table.concat({ prefix, service, "rel", reverse_alias, member_id }, ":")
                    redis.call("SREM", reverse_key, left_id)
                    if redis.call("SCARD", reverse_key) == 0 then
                        redis.call("DEL", reverse_key)
                    end
                end
            end
            redis.call("DEL", relation_key)
        elseif cascade == "detach_dependents" then
            if maintain_reverse then
                local members = redis.call("SMEMBERS", relation_key)
                for j = 1, #members do
                    local member_id = members[j]
                    local reverse_key = table.concat({ prefix, service, "rel", reverse_alias, member_id }, ":")
                    redis.call("SREM", reverse_key, left_id)
                    if redis.call("SCARD", reverse_key) == 0 then
                        redis.call("DEL", reverse_key)
                    end
                end
            end
            redis.call("DEL", relation_key)
        end

        if maintain_reverse then
            local reverse_self_key = table.concat({ prefix, service, "rel", reverse_alias, left_id }, ":")
            local parents = redis.call("SMEMBERS", reverse_self_key)
            for j = 1, #parents do
                local parent_id = parents[j]
                local parent_forward_key = table.concat({ prefix, service, "rel", alias, parent_id }, ":")
                redis.call("SREM", parent_forward_key, left_id)
                if redis.call("SCARD", parent_forward_key) == 0 then
                    redis.call("DEL", parent_forward_key)
                end
            end
            redis.call("DEL", reverse_self_key)
        end
    end

    return { ok = true }
end

local payload = cjson.decode(ARGV[1])
local deletion = payload["delete_entity"]
if deletion == nil then
    return cjson.encode({ error = "invalid_payload", message = "expected DeleteEntity" })
end

local result = delete_with_relations(
    deletion["key"],
    deletion["expected_version"],
    deletion["relations"] or {},
    deletion["unique_constraints"] or {}
)

return cjson.encode(result)
