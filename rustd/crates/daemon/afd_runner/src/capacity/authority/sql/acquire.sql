WITH locked AS MATERIALIZED (
    SELECT id, token_hash = $2 AS credential_matches, status = $4 AS active
    FROM runner.runners
    WHERE id = $1
    FOR UPDATE
), acquired AS (
    UPDATE runner.runners r
    SET connection_id = $3, updated_at = now()
    FROM locked l
    WHERE r.id = l.id AND l.credential_matches AND l.active
      AND r.token_hash = $2 AND r.status = $4
      AND r.connection_id IS NOT DISTINCT FROM $5
    RETURNING r.id
)
SELECT credential_matches, active, EXISTS (SELECT 1 FROM acquired) AS owned
FROM locked
