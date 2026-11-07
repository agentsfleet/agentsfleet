WITH bound AS (
  INSERT INTO connector.channels AS c
    (id, provider, external_account_id, external_channel_id, fleet_id, binding_type,
     created_at, updated_at)
  SELECT $1::uuid, $2, $3, $4, f.id, $6, now(), now()
  FROM fleet.fleets f WHERE f.id = $5::uuid AND f.workspace_id = $7::uuid
  ON CONFLICT (provider, external_account_id, external_channel_id) DO UPDATE
    SET fleet_id = EXCLUDED.fleet_id, binding_type = EXCLUDED.binding_type, updated_at = now()
    WHERE NOT EXISTS (
      SELECT 1 FROM fleet.fleets f WHERE f.id = c.fleet_id AND f.workspace_id = $7::uuid
    )
  RETURNING fleet_id
)
SELECT fleet_id::text FROM bound
UNION ALL
SELECT c.fleet_id::text
FROM connector.channels c
JOIN fleet.fleets f ON f.id = c.fleet_id AND f.workspace_id = $7::uuid
WHERE c.provider = $2 AND c.external_account_id = $3 AND c.external_channel_id = $4
LIMIT 1
