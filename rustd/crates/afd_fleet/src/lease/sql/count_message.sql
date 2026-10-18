UPDATE runner.leases l
SET posted_message_count = l.posted_message_count + 1, updated_at = now()
WHERE l.id = $1::uuid AND l.posted_message_count < $2
  AND l.fencing_token = $3 AND l.status = $4 AND l.lease_expires_at > clock_timestamp()
  AND l.fencing_token >= COALESCE(
        (SELECT a.fencing_token FROM runner.fleet_slots a WHERE a.fleet_id = l.fleet_id),
        l.fencing_token)
RETURNING l.posted_message_count
