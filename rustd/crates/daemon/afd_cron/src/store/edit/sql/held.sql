SELECT sync_token IS NOT NULL
           AND coalesce(sync_lease_expires_at > clock_timestamp(), false)
  FROM fleet.schedules
 WHERE id = $1::uuid
