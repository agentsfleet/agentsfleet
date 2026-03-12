-- W3C Trace Context correlation for distributed tracing.
-- trace_id is 32 hex chars (16 bytes); stored as text for query/log interop.

ALTER TABLE runs
ADD COLUMN IF NOT EXISTS trace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_runs_trace_id ON runs(trace_id);
