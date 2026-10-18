SELECT lease_version, last_lease_type FROM runner.runners WHERE id = $1::uuid
