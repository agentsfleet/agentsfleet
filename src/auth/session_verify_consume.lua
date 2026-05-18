-- Atomic verify-and-consume for the CLI device-flow session at KEYS[1].
--
-- KEYS[1]  = auth:session:{session_id}
-- ARGV[1]  = submitted_hmac_hex  (lower-case hex of HMAC-SHA256(pepper, sid||code))
-- ARGV[2]  = now_ms              (i64 as decimal string)
-- ARGV[3]  = request_fingerprint_hex (lower-case hex of sha256(addr||ua||sid))
-- ARGV[4]  = consume_window_ms   (decimal string; matches CONSUME_REPLAY_WINDOW_MS)
-- ARGV[5]  = max_verify_attempts (decimal string; matches MAX_VERIFY_ATTEMPTS)
-- ARGV[6]  = ttl_seconds         (decimal string; matches SESSION_TTL_SECONDS)
--
-- Return contract (always a Lua table):
--   {"missing"}                    -- no blob; session never existed or TTL evicted
--   {"expired"}                    -- terminal: TTL/explicit expiry
--   {"aborted", reason}            -- terminal: explicit cancel / rate_limit_exceeded / replaced
--   {"consumed"}                   -- terminal: outside replay window OR different fingerprint
--   {"not_approved"}               -- state == pending
--   {"replay", dpk, ct, nonce}     -- consume-idempotency hit (same fingerprint, in window)
--   {"invalid_code", attempts_str} -- HMAC mismatch; attempts now `attempts_str` (1..5)
--   {"success", dpk, ct, nonce}    -- first-write success; transitioned to consumed
--
-- Field names match SessionState's JSON shape; all bytes-typed fields are
-- hex-encoded in the blob so the script stays bit-library-free for Redis
-- portability.

local key                = KEYS[1]
local submitted_hex      = ARGV[1]
local now_ms             = tonumber(ARGV[2])
local fingerprint_hex    = ARGV[3]
local consume_window_ms  = tonumber(ARGV[4])
local max_attempts       = tonumber(ARGV[5])
local ttl_seconds        = tonumber(ARGV[6])

local blob = redis.call("GET", key)
if not blob then return {"missing"} end

local s = cjson.decode(blob)

-- Consume-idempotency window MUST be evaluated before terminal-state rejection
-- so a same-fingerprint retry inside 60s still gets the cached payload.
if s.status == "consumed" then
    local window_open = s.consume_payload_expires_at_ms and s.consume_payload_expires_at_ms > now_ms
    local same_fp     = s.consumed_client_fingerprint_hex == fingerprint_hex
    if window_open and same_fp then
        return {"replay", s.dashboard_public_key, s.ciphertext, s.nonce}
    end
    return {"consumed"}
end

if s.status == "expired" then return {"expired"} end
if s.status == "aborted" then return {"aborted", s.aborted_reason or "unknown"} end
if s.status == "pending" then return {"not_approved"} end
-- Below: s.status == "verification_pending"

-- Constant-time-ish byte XOR accumulator over hex strings. Arithmetic-only
-- (no bit32/bit module) so the script stays portable across Redis 5/6/7+.
-- Both inputs are HMAC-SHA256 outputs hex-encoded => fixed length 64; any
-- mismatch reduces to a non-zero accumulator without per-byte short-circuit.
local stored = s.verification_code_hmac_hex or ""
local a_len, b_len = #stored, #submitted_hex
local min_len = a_len < b_len and a_len or b_len
local diff = 0
for i = 1, min_len do
    local d = string.byte(stored, i) - string.byte(submitted_hex, i)
    diff = diff + d * d
end
diff = diff + (a_len - b_len) * (a_len - b_len)

if diff ~= 0 then
    s.verification_attempts = (s.verification_attempts or 0) + 1
    if s.verification_attempts >= max_attempts then
        s.status = "aborted"
        s.aborted_reason = "rate_limit_exceeded"
    end
    redis.call("SET", key, cjson.encode(s), "EX", ttl_seconds)
    return {"invalid_code", tostring(s.verification_attempts)}
end

s.status = "consumed"
s.consumed_at_ms = now_ms
s.consume_payload_expires_at_ms = now_ms + consume_window_ms
s.consumed_client_fingerprint_hex = fingerprint_hex
redis.call("SET", key, cjson.encode(s), "EX", ttl_seconds)
return {"success", s.dashboard_public_key, s.ciphertext, s.nonce}
