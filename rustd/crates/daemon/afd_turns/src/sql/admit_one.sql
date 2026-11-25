-- Admits one message in one statement. Rust turns the answer row into
-- 202 / 404 / 409 / 503 (stage/verdict/).
--
-- $1 new row id · $2 fleet · $3 workspace · $4 producer (who sent it) ·
-- $5 dedupe key (a steer's is `<fleet_id>:<operation_id>`) · $6 payload digest ·
-- $7 actor · $8 event type · $9 request JSON · $10 deployment ceiling ·
-- $11 current deployment estimate · $12/$13 reply connector and address ·
-- $14 earlier turn in this workspace to copy a reply from ·
-- $15 runnable status, or NULL for any · $16 the status a new turn waits in.

-- 1. Lock the fleet row. One message per fleet is written at a time, so a
--    later sender draws a later sequence. MATERIALIZED: runs once, first. The
--    fleet's thread was written with the fleet, so the turn always has one.
WITH locked_fleet AS MATERIALIZED (
    SELECT f.id, f.status
    FROM fleet.fleets f
    WHERE f.id = $2 AND f.workspace_id = $3
    FOR NO KEY UPDATE OF f
),
-- 2. Was this key sent before? Read only for a fleet the caller owns.
already_sent AS (
    SELECT a.id AS id, a.payload_digest AS digest, a.thread_id AS fleet, a.created_at
    FROM thread.turns a, locked_fleet
    WHERE a.producer = $4 AND a.producer_key = $5
),
-- 3 and 4. Write the message only when every check passes. A twin that
--    committed while this one waited on the lock is returned, not duplicated.
new_message AS (
    INSERT INTO thread.turns
      (id, thread_id, workspace_id, producer, producer_key, payload_digest,
       actor, kind, status, request_json,
       reply_provider, reply_address, created_at, updated_at)
    SELECT $1, locked_fleet.id, $3, $4, $5, $6, $7, $8, $16, $9::jsonb,
           COALESCE($12::text, earlier.reply_provider),
           COALESCE($13::text, earlier.reply_address),
           now(), now()
    FROM locked_fleet
    LEFT JOIN thread.turns earlier
      ON earlier.id = $14 AND earlier.workspace_id = $3
     AND earlier.reply_provider IS NOT NULL
    WHERE NOT EXISTS (SELECT 1 FROM already_sent)
      AND ($15::text IS NULL OR locked_fleet.status = $15::text)
      AND $11::bigint < $10::bigint
    ON CONFLICT (producer, producer_key) DO UPDATE SET updated_at = EXCLUDED.updated_at
    RETURNING (xmax = 0) AS inserted, id, payload_digest AS digest,
              thread_id AS fleet, created_at
)
-- 5. Exactly one answer row, whatever happened above.
SELECT EXISTS (SELECT 1 FROM locked_fleet) AS fleet_found,
       (SELECT status FROM locked_fleet) AS status,
       COALESCE(n.inserted, false) AS inserted,
       COALESCE(n.id, s.id) AS id,
       COALESCE(n.digest, s.digest) AS digest,
       COALESCE(n.fleet, s.fleet) AS fleet,
       COALESCE(n.created_at, s.created_at) AS created_at
FROM (SELECT) AS one
LEFT JOIN new_message n ON true
LEFT JOIN already_sent s ON true
