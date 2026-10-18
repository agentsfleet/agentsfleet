UPDATE runner.fleet_slots
SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
    last_runner_id = CASE WHEN hold_expires_at > clock_timestamp() THEN last_runner_id END
WHERE fleet_id = $1::uuid AND fencing_token = $2
