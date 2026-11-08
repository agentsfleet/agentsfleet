-- Owner-checked abort in ONE evaluation.
--
-- The ownership check and the transition cannot be split: between a client-side
-- "is this mine" read and its write, the session can be consumed by the command
-- line that owns it, and the abort would then erase a login that had already
-- succeeded.
--
-- KEYS[1] the session key
-- ARGV[1] the identity claiming the session
-- ARGV[2] the reason stored on the aborted session
-- ARGV[3] time-to-live, in seconds
--
-- Returns {"ok"}, {"already_aborted"}, {"missing"}, {"not_owner"}, {"consumed"}.
local blob = redis.call("GET", KEYS[1])
if not blob then return {"missing"} end
local s = cjson.decode(blob)
if s.clerk_user_id ~= ARGV[1] then return {"not_owner"} end
if s.status == "consumed" then return {"consumed"} end
if s.status == "aborted" then return {"already_aborted"} end
s.status = "aborted"
s.aborted_reason = ARGV[2]
redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[3]))
return {"ok"}
