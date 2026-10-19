WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.created_at, l.lease_expires_at,
           l.last_metered_at, l.posture, l.provider, l.model,
           COALESCE(e.event_id, l.fleet_event_id::text) AS event_id
    FROM runner.leases l
    LEFT JOIN fleet.events e ON e.id = l.fleet_event_id
    WHERE l.id = $1::uuid AND l.status <> $2
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
       (EXTRACT(EPOCH FROM p.lease_expires_at - p.last_metered_at) * $3::bigint)::bigint AS d_ms,
       p.tenant_id::text AS tenant_id, p.workspace_id::text AS workspace_id,
       p.fleet_id::text AS fleet_id, p.event_id,
       (SELECT f.name FROM fleet.fleets f WHERE f.id = p.fleet_id) AS fleet_name,
       (SELECT count(*) FROM billing.usage_lines u
         WHERE u.work_type = $4 AND u.work_id = p.id AND u.seq >= $5)::bigint AS daemon_lines,
       p.posture, p.provider, p.model
FROM probe p
JOIN moved m ON m.id = p.id
