-- SnugOM entity find script (read with includes)
-- Atomically fetches an entity and its related entities via relation SETs.
-- Supports recursive nested includes for multi-level relation loading.
--
-- Arguments:
--  KEYS[1] - placeholder (unused; commands rely on explicit keys)
--  ARGV[1] - JSON payload describing Find query
--  ARGV[2] - script version for optimistic upgrades (unused for now)

local cjson = cjson

local function encode_result(result)
    return cjson.encode(result)
end

-- Recursively fetch included relations for a given parent entity.
-- Returns a raw JSON string fragment representing the includes map.
-- Each include spec has:
--   alias:              string key in the result
--   relation_key:       exact SET key (top-level, parent ID known)
--   relation_key_prefix: SET key prefix (nested, child ID appended here)
--   child_prefix:       key prefix for child entities (+ child_id = full key)
--   includes:           nested include specs (recursive)
local function fetch_includes_json(includes, parent_id)
    -- Build the includes map as a raw JSON string to avoid cjson encode/decode
    -- cycle which corrupts empty arrays [] into empty objects {}.
    -- Same pattern as entity_get_or_create.lua lines 168-170, 228-230.
    local alias_parts = {}
    for i = 1, #includes do
        local spec = includes[i]
        -- Build relation key: exact for top-level, computed for nested
        local rel_key = spec["relation_key"]
        if rel_key == nil or rel_key == cjson.null then
            rel_key = spec["relation_key_prefix"] .. parent_id
        end
        local child_ids = redis.call("SMEMBERS", rel_key)
        local child_parts = {}
        for j = 1, #child_ids do
            local child_key = spec["child_prefix"] .. child_ids[j]
            local child_json = redis.call("JSON.GET", child_key, "$")
            if child_json and child_json ~= false then
                -- Recurse for nested includes
                local nested_specs = spec["includes"]
                if nested_specs and #nested_specs > 0 then
                    local nested_json = fetch_includes_json(nested_specs, child_ids[j])
                    -- IMPORTANT: embed child_json raw (from JSON.GET $) to preserve arrays
                    table.insert(child_parts, '{"json":' .. child_json .. ',"includes":' .. nested_json .. '}')
                else
                    table.insert(child_parts, '{"json":' .. child_json .. '}')
                end
            end
            -- Skip missing children (deleted entities still in relation SET)
        end
        table.insert(alias_parts, '"' .. spec["alias"] .. '":[' .. table.concat(child_parts, ",") .. ']')
    end
    return '{' .. table.concat(alias_parts, ",") .. '}'
end

-- Main find logic
local function main()
    local payload = cjson.decode(ARGV[1])
    local find = payload["find"]
    if find == nil then
        return encode_result({ error = "invalid_payload", message = "expected find" })
    end

    local entity_key = find["entity_key"]
    if entity_key == nil then
        return encode_result({ error = "invalid_payload", message = "entity_key is required" })
    end

    -- Fetch the parent entity
    local entity_json = redis.call("JSON.GET", entity_key, "$")
    if not entity_json or entity_json == false then
        return encode_result({ error = "entity_not_found" })
    end

    -- Build response using raw string concatenation to avoid cjson decode/encode
    -- cycle which corrupts empty arrays [] into empty objects {}.
    -- IMPORTANT: Pass entity_json raw (from JSON.GET $) — same pattern as
    -- entity_get_or_create.lua lines 168-170, 228-230.
    local includes_spec = find["includes"] or {}
    if #includes_spec > 0 then
        local parent_id = string.match(entity_key, "([^:]+)$")
        local includes_json = fetch_includes_json(includes_spec, parent_id)
        return '{"ok":true,"entity":' .. entity_json .. ',"includes":' .. includes_json .. '}'
    else
        return '{"ok":true,"entity":' .. entity_json .. ',"includes":{}}'
    end
end

return main()
