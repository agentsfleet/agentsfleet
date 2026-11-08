UPDATE runner.runners
SET selftest_checks = $2::jsonb, selftest_sandbox_tier = $3::text,
    selftest_network_policy = $4::text, selftest_completed_at = now(),
    selftest_requested_at = NULL, updated_at = now()
WHERE id = $1::uuid
