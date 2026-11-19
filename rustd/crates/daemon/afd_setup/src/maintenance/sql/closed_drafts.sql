SELECT d.id
  FROM setup.drafts d
 WHERE d.status = $1
   AND d.expires_at <= statement_timestamp()
   AND NOT EXISTS (SELECT 1 FROM setup.attempts a
                    WHERE a.draft_id = d.id AND a.run_expires_at > statement_timestamp())
   AND NOT EXISTS (SELECT 1 FROM fleet.fleets f WHERE f.id = d.fleet_id AND f.status = $2)
 ORDER BY d.expires_at, d.id
 LIMIT $3
   FOR UPDATE OF d SKIP LOCKED
