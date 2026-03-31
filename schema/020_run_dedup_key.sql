-- M16_002 §2.1: Run dedup key column and unique index.
-- Nullable for legacy rows; only set for inline-spec runs that carry a computed dedup key.
-- The unique index uses a partial predicate so legacy NULL rows do not conflict.
ALTER TABLE core.runs ADD COLUMN IF NOT EXISTS dedup_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_runs_dedup_key ON core.runs(dedup_key) WHERE dedup_key IS NOT NULL;
