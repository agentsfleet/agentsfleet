INSERT INTO fleet.admissions
  (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
   actor, event_type, request_json, event_created_at,
   reply_provider, reply_address, created_at, updated_at)
SELECT $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, now(),
       COALESCE($12::text, inherited.reply_provider),
       COALESCE($13::text, inherited.reply_address),
       now(), now()
FROM (SELECT) AS this_admission
LEFT JOIN fleet.admissions AS inherited
  ON inherited.fleet_id = $2::uuid
 AND inherited.id = $14::uuid
 AND inherited.reply_provider IS NOT NULL
WHERE $11::bigint < $10
   OR EXISTS (SELECT 1 FROM fleet.admissions
              WHERE producer = $4 AND producer_key = $5)
ON CONFLICT (producer, producer_key) DO UPDATE SET updated_at = EXCLUDED.updated_at
RETURNING (xmax = 0) AS inserted, id::text AS id, payload_digest AS digest,
          fleet_id::text AS fleet, created_at
