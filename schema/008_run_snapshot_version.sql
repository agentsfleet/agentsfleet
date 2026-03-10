-- UseZombie M5_008 run snapshot linkage

ALTER TABLE runs
ADD COLUMN IF NOT EXISTS run_snapshot_version TEXT;

CREATE INDEX IF NOT EXISTS idx_runs_snapshot_version
    ON runs(run_snapshot_version, created_at DESC);
