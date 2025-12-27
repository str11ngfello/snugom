-- SnugOM relation mutation script
-- Arguments:
--  ARGV[1] - JSON payload describing MutationCommand::MutateRelations

local function split_key(key)
    local parts = {}
    for part in string.gmatch(key, "([^:]+)") do
        table.insert(parts, part)
    end
    return parts
end

local payload = cjson.decode(ARGV[1])
local mutation = payload["mutate_relations"]
if mutation == nil then
    return cjson.encode({ error = "invalid_payload", message = "expected MutateRelations" })
end

local relation_key = mutation["relation_key"]
local add = mutation["add"] or {}
local remove = mutation["remove"] or {}
local maintain_reverse = mutation["maintain_reverse"] == true

local relation_parts
local prefix
local service
local alias
local left_id
local reverse_alias

if maintain_reverse then
    -- Relation key structure: {prefix}:{service}:rel:{alias}:{left_id}
    relation_parts = split_key(relation_key)
    prefix = relation_parts[1]
    service = relation_parts[2]
    -- relation_parts[3] is "rel"
    alias = relation_parts[4]
    left_id = relation_parts[5]
    reverse_alias = alias .. "_reverse"
end

if #add > 0 then
    redis.call("SADD", relation_key, unpack(add))
    if maintain_reverse then
        for i = 1, #add do
            local member_id = add[i]
            local reverse_key = table.concat({ prefix, service, "rel", reverse_alias, member_id }, ":")
            redis.call("SADD", reverse_key, left_id)
        end
    end
end

if #remove > 0 then
    redis.call("SREM", relation_key, unpack(remove))
    if maintain_reverse then
        for i = 1, #remove do
            local member_id = remove[i]
            local reverse_key = table.concat({ prefix, service, "rel", reverse_alias, member_id }, ":")
            redis.call("SREM", reverse_key, left_id)
            if redis.call("SCARD", reverse_key) == 0 then
                redis.call("DEL", reverse_key)
            end
        end
    end
end

return cjson.encode({ ok = true })
