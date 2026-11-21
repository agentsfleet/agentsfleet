SELECT d.id
  FROM setup.drafts d
 WHERE d.status = $1
   AND d.expires_at <= statement_timestamp()
   -- A draft's live run holds its thread, whose id is the draft's.
   AND NOT EXISTS (SELECT 1 FROM thread.threads s
                    WHERE s.id = d.id AND s.run_expires_at > statement_timestamp())
   AND NOT EXISTS (SELECT 1 FROM fleet.fleets f WHERE f.id = d.fleet_id AND f.status = $2)
 ORDER BY d.expires_at, d.id
 LIMIT $3
   FOR UPDATE OF d SKIP LOCKED
