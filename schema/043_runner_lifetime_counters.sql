-- One lifetime tally row per runner, maintained by the lease write paths.
--
-- Why: the runner detail read aggregated the runner's whole lease history —
-- plus a per-lease join to core.fleet_events for outcome classification — on
-- every page load. That read grows with the runner's history (slot 041's own
-- note names this follow-up); a mature runner holds tens of thousands of
-- leases and nothing pruned them. Maintaining the tallies at write time makes
-- the detail read an indexed one-to-one join, constant in history.
--
-- No triggers (unlike slot 030): classifying a terminal transition needs the
-- lease/event status vocabulary, which lives in application constants — RULE
-- STS keeps value vocabularies out of schema objects so they cannot drift
-- from code. Instead, each transition's owning SQL statement gains a counter
-- arm conditioned on the guarded write actually affecting rows (single owner
-- per transition, transactional by construction, retry-safe).
-- Exactly ONE unique index, deliberately: `uid` IS the runner id (writers
-- supply both columns with the same value). A second unique key (the earlier
-- `runner_id UNIQUE` + generated-uid shape) breaks concurrent first-touch
-- upserts — `ON CONFLICT` can arbitrate only one constraint, so two sessions
-- inserting a brand-new runner's row race to a duplicate-key error on the
-- other index instead of taking the update arm.
CREATE TABLE IF NOT EXISTS fleet.runner_lifetime_counters (
    uid        UUID   PRIMARY KEY,
    CONSTRAINT ck_runner_lifetime_counters_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    runner_id  UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    CONSTRAINT ck_runner_lifetime_counters_uid_is_runner CHECK (uid = runner_id),
    acquired   BIGINT NOT NULL DEFAULT 0,
    succeeded  BIGINT NOT NULL DEFAULT 0,
    failed     BIGINT NOT NULL DEFAULT 0,
    expired    BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

GRANT SELECT, INSERT, UPDATE ON fleet.runner_lifetime_counters TO api_runtime;

-- Backfill tallies for runners that existed before this migration. The status
-- literals below mirror values already frozen in existing rows (written by the
-- application's named constants); the runtime increments never read them, so
-- they cannot drift forward — they describe history, not vocabulary.
--
-- The conflict arm takes GREATEST, not the recount, and that is load-bearing
-- twice over.
--
-- 1. It makes this statement the ONE safe repair for the rolling-deploy gap.
--    `release_command` applies this migration while the old machines are still
--    serving, and only the replaced ones carry the tally arms — so a lease
--    acquired by an old replica after this snapshot is counted by nobody, and
--    the tallies sit permanently low by however many leases the rollout
--    overlapped. Re-running the statement any time before the retention sweep
--    prunes (30 days) recomputes exactly those, and takes them.
-- 2. It stops that repair from ever becoming destructive. Once retention has
--    deleted a runner's aged history, a recount is SMALLER than the truth --
--    lifetime tallies count transitions, not surviving rows. An absolute
--    assignment would silently zero a mature runner's totals; GREATEST cannot
--    lower anything, so the statement is safe to re-run at any age, and the
--    monotonicity the counters promise holds in the one place that could break
--    it.
--
-- A resident reconciler is deliberately NOT shipped for the same reason: after
-- the first prune a recount is no longer a source of truth, so nothing that
-- recounts on a schedule can be left running.
INSERT INTO fleet.runner_lifetime_counters
    (uid, runner_id, acquired, succeeded, failed, expired, created_at, updated_at)
SELECT r.id, r.id,
       COUNT(l.id),
       COUNT(l.id) FILTER (WHERE l.status = 'reported' AND e.status = 'processed'),
       COUNT(l.id) FILTER (WHERE l.status = 'reported' AND e.status = 'fleet_error'),
       COUNT(l.id) FILTER (WHERE l.status = 'expired'),
       r.created_at, r.updated_at
  FROM fleet.runners r
  LEFT JOIN fleet.runner_leases l ON l.runner_id = r.id
  LEFT JOIN core.fleet_events e
         ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
 GROUP BY r.id, r.created_at, r.updated_at
ON CONFLICT (uid) DO UPDATE
   SET acquired  = GREATEST(fleet.runner_lifetime_counters.acquired,  EXCLUDED.acquired),
       succeeded = GREATEST(fleet.runner_lifetime_counters.succeeded, EXCLUDED.succeeded),
       failed    = GREATEST(fleet.runner_lifetime_counters.failed,    EXCLUDED.failed),
       expired   = GREATEST(fleet.runner_lifetime_counters.expired,   EXCLUDED.expired),
       updated_at = GREATEST(fleet.runner_lifetime_counters.updated_at, EXCLUDED.updated_at);
