-- One trigger. The row's key share on its fleet keeps a delete from passing it; a second row for
-- one (fleet, kind, source) inserts nothing, and the caller refuses it.
--
-- $1 id · $2 fleet · $3 workspace · $4 kind · $5 source · $6 events · $7 repositories ·
-- $8 verifier scheme · $9 verifier header · $10 secret name · $11 channel
INSERT INTO fleet.triggers
    (id, fleet_id, workspace_id, kind, source, events, repositories, verifier_scheme,
     verifier_header, secret_name, channel, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6::text[], $7::text[], $8, $9, $10, $11, now(), now())
ON CONFLICT (fleet_id, kind, source) DO NOTHING
RETURNING id, kind, source, events, repositories, verifier_scheme, verifier_header, secret_name,
          channel, created_at
