SELECT status FROM runner.leases WHERE id = $1::uuid AND runner_id = $2::uuid
