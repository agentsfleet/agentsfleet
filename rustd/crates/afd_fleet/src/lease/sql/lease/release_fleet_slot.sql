UPDATE runner.fleet_slots
SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
    hold_expires_at = CASE WHEN $3::timestamptz > clock_timestamp()
                           THEN LEAST($3::timestamptz, clock_timestamp() + $4::interval) END
WHERE fleet_id = $1::uuid AND fencing_token = $2
RETURNING hold_expires_at
