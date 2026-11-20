-- Per-runner tallies, kept off the runner row so run outcomes do not lock the row a heartbeat
-- writes.
CREATE TABLE runner.lifetime_counters (
    runner_id        UUID PRIMARY KEY REFERENCES runner.runners(id) ON DELETE CASCADE,
    acquired_count   BIGINT NOT NULL,
    succeeded_count  BIGINT NOT NULL,
    failed_count     BIGINT NOT NULL,
    expired_count    BIGINT NOT NULL,
    created_at       TIMESTAMPTZ(3) NOT NULL,
    updated_at       TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE ON runner.lifetime_counters TO api_runtime;
