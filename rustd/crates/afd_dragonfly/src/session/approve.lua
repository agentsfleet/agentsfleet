-- Atomic approve: pending -> verification_pending in ONE evaluation.
--
-- Two dashboards racing one Approve click is the whole reason this is a
-- script. Read-branch-write from the client has a window between the read and
-- the write in which both callers see `pending`, and both then store their own
-- ciphertext -- the losing one silently overwriting the credential the person
-- actually saw a code for.
--
-- KEYS[1] the session key
-- ARGV[1] dashboard_public_key   ARGV[2] ciphertext
-- ARGV[3] nonce                  ARGV[4] verification_code_hmac_hex
-- ARGV[5] clerk_user_id          ARGV[6] now, in milliseconds
-- ARGV[7] time-to-live, in seconds
--
-- Returns {"ok"}, {"missing"}, or {"conflict", <status>}.
local blob = redis.call("GET", KEYS[1])
if not blob then return {"missing"} end
local s = cjson.decode(blob)
if s.status ~= "pending" then return {"conflict", s.status} end
s.status = "verification_pending"
s.dashboard_public_key = ARGV[1]
s.ciphertext = ARGV[2]
s.nonce = ARGV[3]
s.verification_code_hmac_hex = ARGV[4]
s.clerk_user_id = ARGV[5]
s.approved_at_ms = tonumber(ARGV[6])
-- Re-stamped rather than left at its create-time value: the SET below resets
-- the key's time-to-live, and a stale expiry would let the background sweep
-- prune a session that was approved one second ago.
s.expires_at_ms = tonumber(ARGV[6]) + tonumber(ARGV[7]) * 1000
redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[7]))
return {"ok"}
