UPDATE runner.slots
SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
    hold_expires_at = CASE WHEN run_expires_at > clock_timestamp()
                           AND $3::timestamptz > clock_timestamp()
                           THEN LEAST($3::timestamptz, clock_timestamp() + $4::interval) END
WHERE id = $1 AND fencing_token = $2
RETURNING hold_expires_at
