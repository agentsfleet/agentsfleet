-- Append-only runner history for the operator plane.
--
-- `fleet.runners.admin_state` is operator intent and liveness is derived at read
-- time; this table is the durable history of runner lifecycle and work
-- transitions. Event type values are app-enforced tags (RULE STS). `metadata` is
-- JSONB so event-specific detail can ride beside the typed event without bloating
-- the current-state row.
--
-- The retired shape carried `occurred_at` AND `created_at`. They were never
-- different: all four writers bound the same parameter to both columns, so every
-- row stored one instant twice — sixteen bytes per row on a table that gains two
-- rows per lease, plus a second name for one concept. An
-- append-only event row occurs when it is created, so `created_at` is the whole
-- truth and the indexes below order by it.
--
-- `dedup_key` belongs to the liveness sweeper: a runner_offline event carries the
-- stale last_seen_at snapshot, so N replicas racing the same stale runner admit
-- exactly one offline row.

CREATE TABLE IF NOT EXISTS fleet.runner_events (
    id          UUID   PRIMARY KEY,
    CONSTRAINT ck_runner_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id   UUID   NOT NULL REFERENCES fleet.runners(id) ON DELETE CASCADE,
    event_type  TEXT   NOT NULL,
    metadata    JSONB  NOT NULL,
    dedup_key   BIGINT,
    created_at  BIGINT NOT NULL
);

-- No `updated_at`: the table is append-only, and api_runtime holds no UPDATE
-- grant, so a row-change time could never be written.

-- Reader: the unfiltered per-runner history feed, newest-first
-- (GET /v1/fleets/runners/{id}/events).
CREATE INDEX IF NOT EXISTS idx_runner_events_runner_id_created_at_id
    ON fleet.runner_events (runner_id, created_at DESC, id DESC);

-- Reader: the same feed filtered to the rare runner-lifecycle tags. The table is
-- dominated by the two per-lease tags — one acquired and one released per lease —
-- so on the index above a filtered page walks and discards the bulk to fill 25
-- rows, and a filtered COUNT walks everything. Both grow with lease history.
--
-- A partial index excluding the two high-volume tags was considered and
-- rejected: the reads bind the tag list as a parameter array
-- (event_type = ANY($n)), and the planner cannot prove a bound parameter
-- satisfies a partial-index predicate, so generic plans would fall off it. The
-- full composite stays usable for any parameterised tag set, and the retention
-- sweep bounds its size.
CREATE INDEX IF NOT EXISTS idx_runner_events_runner_id_type_created_at_id
    ON fleet.runner_events (runner_id, event_type, created_at DESC, id DESC);

-- Reader: the retention sweep (fleet/retention_sweeper.zig), which filters by tag
-- and age across ALL runners — so neither runner-leading index above can serve
-- it. Measured on the steady-state cycle at 100,000 rows across 201 runners:
-- 4.76 ms on the runner-leading index → 0.36 ms here. The runner-leading index
-- can only bound the age WITHIN one runner's segment, so the sweep walked every
-- runner's segment in turn; on a single-runner fixture that looks free and the
-- planner even prefers it, so measure at real cardinality or this index reads as
-- redundant.
CREATE INDEX IF NOT EXISTS idx_runner_events_type_created_at
    ON fleet.runner_events (event_type, created_at);

-- Exactly one runner_offline row per stale-last_seen_at episode, across every
-- agentsfleetd replica. The literal in the predicate names its application
-- constant: it is the tag protocol.RunnerEventType.runner_offline, and a partial
-- index requires a SQL predicate to express that. The offline
-- insert's ON CONFLICT clause repeats this predicate verbatim, so the two must
-- stay identical.
CREATE UNIQUE INDEX IF NOT EXISTS uq_runner_events_runner_id_dedup_key_offline
    ON fleet.runner_events (runner_id, dedup_key)
    WHERE event_type = 'runner_offline' AND dedup_key IS NOT NULL;

-- api_runtime appends lifecycle events and serves the operator history endpoint.
-- No UPDATE grant: append-only by privilege. DELETE is the retention sweeper's,
-- which removes aged rows — the one retention policy that exists, kept exactly as
-- M149 shipped it.
GRANT SELECT, INSERT, DELETE ON fleet.runner_events TO api_runtime;
