WITH lease AS MATERIALIZED (
    SELECT l.id, l.fleet_id, l.posted_message_count, l.fencing_token, l.status,
           l.lease_expires_at
    FROM runner.leases l
    WHERE l.id = $1::uuid
    FOR NO KEY UPDATE OF l
), live AS MATERIALIZED (
    SELECT lease.id
    FROM lease
    WHERE lease.posted_message_count < $2
      AND lease.fencing_token = $3 AND lease.status = $4
      AND lease.lease_expires_at > clock_timestamp()
      AND lease.fencing_token >= COALESCE(
            (SELECT a.fencing_token FROM runner.fleet_slots a WHERE a.fleet_id = lease.fleet_id),
            lease.fencing_token)
)
UPDATE runner.leases l
SET posted_message_count = l.posted_message_count + 1, updated_at = now()
FROM live
WHERE l.id = live.id
RETURNING l.posted_message_count
