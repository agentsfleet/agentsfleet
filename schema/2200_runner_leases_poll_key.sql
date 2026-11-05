-- The worker poll that issued a lease, so a poll repeated after its reply was
-- lost frees that lease at once instead of stalling the fleet for the lease's
-- lifetime.
--
-- `poll_key` is the `Idempotency-Key` one runner worker sent with one poll
-- attempt, NULL for a runner that sends none. It is a lookup key, not a retry
-- key in the receipt sense (rule SQL-RETRYKEY): a repeated key never creates a
-- row, it only finds the lease its own poll issued, so it needs no receipt to
-- outlive that lease. Writer: the lease issue (`insert_lease_with_event.sql`).
-- Reader: the claim's replay arm (`claim_admission_head.sql`), through the
-- unique index, which also proves one poll issues at most one lease.
ALTER TABLE runner.leases ADD COLUMN poll_key UUID;

CREATE UNIQUE INDEX uq_leases_runner_id_poll_key
    ON runner.leases (runner_id, poll_key) WHERE poll_key IS NOT NULL;
