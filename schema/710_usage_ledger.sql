-- The usage ledger: what each event cost, and the row the wallet reconciles
-- against. At most three rows per event — one per charge type — and never more.
--
-- Renamed from `core.fleet_execution_telemetry` and moved out of `core`. The old
-- name called it telemetry, which made it look optional: it is the billing
-- record, and it belongs in `billing` beside the wallet it must agree with.
--
-- NOT append-only, and this is load-bearing. Each row ACCUMULATES in place —
-- the renewal and settle paths both write
-- `ON CONFLICT (event_id, charge_type) DO UPDATE SET … = … + EXCLUDED.…`, so a
-- run that renews forty times still holds one stage row carrying the summed
-- cost. An append-only ledger was proposed and withdrawn: three rows per event
-- would have become thousands. The retired per-slice table was NOT consumerless
-- as §4 first claimed — the budget drain read it to attribute a long run's spend
-- across a window — and `last_charged_at` below keeps that, on this one row.
--
-- The three identity columns are UUIDs with real foreign keys. They were
-- `tenant_id UUID` with no reference and `workspace_id`/`fleet_id` as bare TEXT,
-- which is why erasing an account needed a hand-maintained delete order and why
-- the activity-counter trigger regex-checked its own `fleet_id` before casting.
-- Typed and referenced, the database answers both.
--
-- The three references delete DIFFERENTLY, and that difference is the point.
-- `tenant_id` is NOT NULL and cascades, so erasing an account still leaves zero
-- rows (Dimension 3.3). `workspace_id` and `fleet_id` are nullable and SET NULL,
-- because deleting a fleet is routine and a charge already incurred is not
-- theirs to erase by doing it. Under a cascade the rows would vanish while the
-- wallet stayed drained — holes in the charges endpoint, and Invariant 5 ("the
-- wallet drain equals the ledger sum") falsified by an ordinary User Interface
-- action. Readers must therefore treat both as optional: a NULL fleet is a
-- charge whose fleet was deleted, not a corrupt row.
--
-- `event_created_at` is the originating event's creation instant, and it is NOT
-- the row's. It is carried so a later partitioning decision has a stable key
-- available without a rewrite; no partitioning machinery is built here.
-- It must key on the EVENT's creation time rather than the
-- write time, because a renewal firing hours after the receive row would
-- otherwise land in a different partition, miss the conflict target, and
-- silently duplicate ledger rows instead of accumulating into the existing one.
-- Every row for one event carries the same value; the accumulate arm never writes it.
--
-- `charge_type` and `posture` vocabularies are app-enforced named constants,
-- never SQL CHECKs (RULE STS).

CREATE TABLE IF NOT EXISTS billing.usage_ledger (
    id                    UUID   PRIMARY KEY,
    CONSTRAINT ck_usage_ledger_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id             UUID   NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    workspace_id          UUID   REFERENCES core.workspaces(id) ON DELETE SET NULL,
    fleet_id              UUID   REFERENCES core.fleets(id) ON DELETE SET NULL,
    event_id              TEXT   NOT NULL,
    charge_type           TEXT   NOT NULL,
    posture               TEXT   NOT NULL,
    model                 TEXT   NOT NULL,
    -- Structural DEFAULT: zero is the identity of an accumulator, not a
    -- vocabulary value. The receive row is inserted before any cost is known.
    credit_deducted_nanos BIGINT NOT NULL DEFAULT 0,
    token_count_input     BIGINT,
    -- Cached input is priced differently from fresh input, so without it the
    -- charge cannot be recomputed from this row and a disputed bill cannot be
    -- answered from the ledger. Carried for auditability, not a query reader —
    -- the one column here that earns its place that way.
    token_count_cached_input BIGINT,
    token_count_output    BIGINT,
    wall_ms               BIGINT,
    event_created_at      BIGINT NOT NULL,
    created_at            BIGINT NOT NULL,
    -- The run's LAST charge instant; `created_at` is its first. The drain
    -- apportions the accumulated total across the two (fleet/sql.zig).
    last_charged_at       BIGINT NOT NULL,
    -- The accumulate arbiter: what makes a re-sent renewal update the existing
    -- row rather than add one, and what caps the table at three rows per event.
    -- It holds a different value from `id` rather than duplicating it.
    CONSTRAINT uq_usage_ledger_event_id_charge_type UNIQUE (event_id, charge_type)
);

-- `recorded_at` is gone, renamed to `created_at`: it meant the instant the row
-- was written, a fourth name for one concept. The genuinely
-- distinct instants here are `event_created_at` and `last_charged_at`, which
-- keep domain names because neither is this row's birth.

-- Indexes live in schema/720_usage_ledger_indexes.sql.

-- billing_runtime owns the table. api_runtime is a member (schema/110) and must
-- assume this role to reach it. No DELETE: a ledger row leaves only with the
-- tenant that paid, through the cascade above. Nothing else in the system can
-- erase a charge — not a fleet delete, not a workspace delete, not a handler.
GRANT SELECT, INSERT, UPDATE ON billing.usage_ledger TO billing_runtime;

-- api_runtime reads the ledger WITHOUT elevating, deliberately: the privilege
-- split fences money that MOVES, and a charge history does not move. Four readers need it —
-- the charges list, the events-list cost join (`state/fleet_events_store.zig`),
-- the per-fleet outcome reads and the fleet delete path — none of which writes.
-- Omitting it answers all four with insufficient_privilege. SELECT only: every
-- write to this table runs under metering_runtime.
GRANT SELECT ON billing.usage_ledger TO api_runtime;

-- Read-only operator principals see no money rows, stated explicitly so
-- re-widening them is a visible edit to this line.
REVOKE ALL ON billing.usage_ledger FROM ops_readonly_human, ops_readonly_fleet;
