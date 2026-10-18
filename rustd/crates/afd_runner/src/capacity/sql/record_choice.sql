UPDATE runner.runners
SET lease_version = lease_version + 1, last_lease_type = $3::text, updated_at = now()
WHERE id = $1::uuid AND lease_version = $2::bigint
