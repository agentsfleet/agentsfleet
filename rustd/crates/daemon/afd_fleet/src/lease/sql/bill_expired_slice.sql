WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.created_at, l.lease_expires_at,
           l.last_metered_at, l.posture, l.provider, l.model, l.event_id,
           (EXTRACT(EPOCH FROM l.lease_expires_at - l.last_metered_at) * $3::bigint)::bigint AS d_ms,
           -- The runtime line's number, as the meter numbers it: the slice's start.
           (EXTRACT(EPOCH FROM l.last_metered_at - l.created_at) * $3::bigint)::bigint AS runtime_seq
    FROM runner.leases l
    WHERE l.id = $1 AND l.status <> $2
      AND l.lease_expires_at < statement_timestamp()
      AND l.last_metered_at < l.lease_expires_at
    FOR UPDATE OF l
), moved AS (
    UPDATE runner.leases l
    SET last_metered_at = p.lease_expires_at, updated_at = now()
    FROM probe p WHERE l.id = p.id
    RETURNING l.id
)
SELECT p.lease_expires_at AS checked_at, p.created_at AS started_at,
       p.d_ms,
       p.tenant_id AS tenant_id, p.workspace_id AS workspace_id,
       p.fleet_id AS fleet_id, p.event_id AS event_id,
       (SELECT f.name FROM fleet.fleets f WHERE f.id = p.fleet_id) AS fleet_name,
       p.runtime_seq,
       p.posture, p.provider, p.model
FROM probe p
JOIN moved m ON m.id = p.id
