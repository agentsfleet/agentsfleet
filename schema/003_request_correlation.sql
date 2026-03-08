-- Correlation metadata for end-to-end API request to worker lifecycle tracing.

ALTER TABLE runs
ADD COLUMN IF NOT EXISTS request_id TEXT;

CREATE INDEX IF NOT EXISTS idx_runs_request_id ON runs(request_id);
