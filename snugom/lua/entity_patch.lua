local cjson = cjson

local function decode_payload(arg)
    local payload, err = cjson.decode(arg)
    if not payload then
        return nil, { error = 'invalid_payload', message = err or 'unable to decode json payload' }
    end
    return payload, nil
end

local function normalize_version(value)
    if value == cjson.null then
        return nil
    end
    if type(value) == 'table' then
        return normalize_version(value[1])
    end
    if type(value) == 'string' then
        local num = tonumber(value)
        if num then
            return num
        end
        return value
    end
    return value
end

local function load_current_version(key)
    local raw = redis.call('JSON.GET', key, '$.metadata.version')
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

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, '([^:]+)') do
        table.insert(parts, part)
    end
    return parts
end

local function ensure_idempotency(key, idempotency_key, response)
    if not idempotency_key then
        return nil
    end

    local parts = split_key(key)
    local global_store_key = nil
    if #parts >= 3 then
        global_store_key = table.concat({ parts[1], parts[2], parts[3], 'idempotency', idempotency_key }, ':')
    end
    local entity_store_key = key .. ':idempotency:' .. idempotency_key

    if response == nil then
        local existing = redis.call('GET', entity_store_key)
        if existing then
            return existing
        end
        if global_store_key ~= nil then
            return redis.call('GET', global_store_key)
        end
        return nil
    end

    if global_store_key ~= nil then
        local existing_global = redis.call('GET', global_store_key)
        if existing_global ~= nil then
            return existing_global
        end
    end

    local existing_entity = redis.call('GET', entity_store_key)
    if existing_entity ~= nil then
        return existing_entity
    end

    redis.call('SET', entity_store_key, response, 'EX', 900)
    if global_store_key ~= nil then
        redis.call('SET', global_store_key, response, 'EX', 900)
    end
    return nil
end

local function apply_operation(key, op)
    local path = op['path']
    local op_type = op['type']
    local value_json = op['value_json']

    if op_type == 'assign' then
        if value_json == nil then
            return { error = 'invalid_payload', message = 'value_json is required for assign' }
        end
        redis.call('JSON.SET', key, path, value_json)
    elseif op_type == 'merge' then
        if value_json == nil then
            return { error = 'invalid_payload', message = 'value_json is required for merge' }
        end
        redis.call('JSON.MERGE', key, path, value_json)
    elseif op_type == 'delete' then
        redis.call('JSON.DEL', key, path)
    elseif op_type == 'increment' then
        local number_value = op['value']
        if type(number_value) == 'table' then
            number_value = number_value[1]
        end
        if type(number_value) == 'string' then
            number_value = tonumber(number_value)
        end
        redis.call('JSON.NUMINCRBY', key, path, number_value)
    else
        error('unknown patch operation type: ' .. tostring(op_type))
    end
end

local function apply_mirror(key, mirror)
    if not mirror then
        return
    end
    local mirror_field = mirror['mirror_field']
    local value_json = mirror['mirror_value_json']
    if value_json == nil or value_json == 'null' then
        redis.call('JSON.DEL', key, '$.' .. mirror_field)
    else
        redis.call('JSON.SET', key, '$.' .. mirror_field, value_json)
    end
end

local function main()
    local payload, error_payload = decode_payload(ARGV[1])
    if error_payload then
        return encode_result(error_payload)
    end

    local patch = payload['patch_entity']
    if not patch then
        return encode_result({ error = 'invalid_payload', message = 'expected PatchEntity payload' })
    end

    local key = patch['key']
    local expected_version = patch['expected_version']
    local operations = patch['operations'] or {}
    local idempotency_key = patch['idempotency_key']
    local relations = patch['relations'] or {}
    local entity_id = patch['entity_id']
    local unique_constraints = patch['unique_constraints'] or {}

    local exists = redis.call('EXISTS', key)
    if exists == 0 then
        return encode_result({
            error = 'entity_not_found',
            entity_id = entity_id
        })
    end

    if #operations == 0 and #relations == 0 then
        return encode_result({ ok = true, version = nil, entity_id = nil })
    end

    local replay = ensure_idempotency(key, idempotency_key, nil)
    if replay then
        return replay
    end

    local current_version, err = load_current_version(key)
    if err then
        return encode_result({ error = 'version_read_failed', message = err })
    end

    if expected_version ~= nil then
        if current_version == nil or current_version ~= expected_version then
            if idempotency_key ~= nil then
                local replay_conflict = ensure_idempotency(key, idempotency_key, nil)
                if replay_conflict then
                    return replay_conflict
                end
                local response = encode_result({
                    ok = true,
                    version = current_version,
                    entity_id = entity_id,
                })
                ensure_idempotency(key, idempotency_key, response)
                return response
            end
            return encode_result({
                error = 'version_conflict',
                expected = expected_version,
                actual = current_version,
            })
        end
    end

    -- Handle unique constraint enforcement for patch operations
    local key_parts = split_key(key)
    local prefix = key_parts[1]
    local service = key_parts[2]
    local collection = key_parts[3]
    local unique_updates = {}

    if #unique_constraints > 0 then
        -- Read current entity to get existing values for unique fields
        local entity_json = redis.call('JSON.GET', key, '$')
        local entity_data = nil
        if entity_json ~= nil and entity_json ~= false then
            local decoded = cjson.decode(entity_json)
            if type(decoded) == 'table' and decoded[1] ~= nil then
                entity_data = decoded[1]
            else
                entity_data = decoded
            end
        end

        for i = 1, #unique_constraints do
            local constraint = unique_constraints[i]
            local fields = constraint['fields']
            local case_insensitive = constraint['case_insensitive'] == true
            local new_values = constraint['values']  -- From the patch, null means read from entity

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
                    if case_insensitive and type(v) == 'string' then
                        v = string.lower(v)
                    end
                    table.insert(old_lookup_parts, tostring(v))
                end
            else
                old_has_null = true
            end

            -- Build new lookup value (from patch or entity for unchanged fields)
            local new_lookup_parts = {}
            local new_has_null = false
            local final_values = {}
            for j = 1, #fields do
                local v = new_values[j]
                -- If value is null in patch, use current entity value
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
                if case_insensitive and type(v) == 'string' then
                    v = string.lower(v)
                end
                table.insert(new_lookup_parts, tostring(v))
            end

            if not new_has_null then
                local new_lookup_value = table.concat(new_lookup_parts, ':')
                local old_lookup_value = nil
                if not old_has_null then
                    old_lookup_value = table.concat(old_lookup_parts, ':')
                end

                -- Only check if the value is actually changing
                if old_lookup_value ~= new_lookup_value then
                    -- Build unique index key
                    local unique_key
                    if #fields == 1 then
                        unique_key = table.concat({ prefix, service, collection, 'unique', fields[1] }, ':')
                    else
                        local field_suffix = table.concat(fields, '_')
                        unique_key = table.concat({ prefix, service, collection, 'unique_compound', field_suffix }, ':')
                    end

                    -- Check if new value conflicts with OTHER entity
                    local existing_id = redis.call('HGET', unique_key, new_lookup_value)
                    if existing_id ~= nil and existing_id ~= false and existing_id ~= entity_id then
                        return encode_result({
                            error = 'unique_constraint_violation',
                            fields = fields,
                            values = final_values,
                            existing_entity_id = existing_id,
                        })
                    end

                    -- Track for later update
                    table.insert(unique_updates, {
                        unique_key = unique_key,
                        old_lookup_value = old_lookup_value,
                        new_lookup_value = new_lookup_value,
                        entity_id = entity_id,
                    })
                end
            end
        end
    end

    for _, op in ipairs(operations) do
        local op_result = apply_operation(key, op)
        if op_result ~= nil and op_result['error'] ~= nil then
            return encode_result(op_result)
        end
        local mirror = op['mirror']
        if mirror then
            apply_mirror(key, mirror)
        end
    end

    for i = 1, #relations do
        local relation = relations[i]
        local relation_key = relation['relation_key']
        local add = relation['add'] or {}
        local remove = relation['remove'] or {}
        local maintain_reverse = relation['maintain_reverse'] == true

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
            reverse_alias = alias .. '_reverse'
        end

        if #add > 0 then
            redis.call('SADD', relation_key, unpack(add))
            if maintain_reverse then
                for j = 1, #add do
                    local member_id = add[j]
                    local reverse_key = table.concat({ rel_prefix, rel_service, 'rel', reverse_alias, member_id }, ':')
                    redis.call('SADD', reverse_key, left_id)
                end
            end
        end

        if #remove > 0 then
            redis.call('SREM', relation_key, unpack(remove))
            if maintain_reverse then
                for j = 1, #remove do
                    local member_id = remove[j]
                    local reverse_key = table.concat({ rel_prefix, rel_service, 'rel', reverse_alias, member_id }, ':')
                    redis.call('SREM', reverse_key, left_id)
                    if redis.call('SCARD', reverse_key) == 0 then
                        redis.call('DEL', reverse_key)
                    end
                end
            end
        end
    end

    local next_version
    if current_version == nil then
        next_version = 1
    else
        if type(current_version) ~= 'number' then
            next_version = 1
        else
            next_version = current_version + 1
        end
    end

    redis.call('JSON.SET', key, '$.metadata.version', next_version)
    redis.call('PERSIST', key)

    -- Update unique indexes after successful patch
    for i = 1, #unique_updates do
        local update = unique_updates[i]
        -- Remove old value if it existed
        if update.old_lookup_value ~= nil then
            redis.call('HDEL', update.unique_key, update.old_lookup_value)
        end
        -- Add new value
        redis.call('HSET', update.unique_key, update.new_lookup_value, update.entity_id)
    end

    local response = encode_result({
        ok = true,
        version = next_version,
        entity_id = patch['entity_id'] or nil,
    })

    ensure_idempotency(key, idempotency_key, response)

    return response
end

return main()
