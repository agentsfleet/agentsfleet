//! SQL statement text for the fleet control-plane domain (RULE SQLMOD — query
//! text lives here, grepable in one place).
//!
//! The metering statements in `renewal.zig` and `renewal_settle.zig` stay
//! inline on purpose. They are the most correctness-critical text in the
//! repository, they are read alongside the token arithmetic they settle, and
//! moving them buys legibility they do not need.

// ── Liveness sweep ──────────────────────────────────────────────────────────

/// Runners due for a sweep: stale heartbeat, draining, or holding an active
/// lease while not active.
///
/// Ordered and bounded so a cycle's cost is the batch, not the fleet.
/// `idx_runners_updated_at_id` (schema slot 033) serves both the ordering and
/// the bound — before it, this top-N sorted the whole filtered set every cycle.
/// `$1` never-seen sentinel, `$2` now_ms, `$3` offline threshold,
/// `$4` active state, `$5` active lease status, `$6` draining state, `$7` batch.
pub const SELECT_DUE_RUNNERS =
    \\SELECT r.id::text, r.last_seen_at, r.admin_state
    \\FROM fleet.runners r
    \\WHERE (r.last_seen_at <> $1 AND ($2::bigint - r.last_seen_at) > $3)
    \\   OR r.admin_state = $6
    \\   OR (r.admin_state <> $4 AND EXISTS (
    \\        SELECT 1 FROM fleet.runner_leases l
    \\        WHERE l.runner_id = r.id AND l.status = $5
    \\      ))
    \\ORDER BY r.updated_at ASC, r.id ASC
    \\LIMIT $7
;

/// Record a runner going offline, at most once per stale episode.
///
/// The partial unique index on `(runner_id, dedup_key)` is what makes the sweep
/// idempotent: a cycle that re-observes the same stale runner inserts nothing,
/// and the returned count tells the caller whether THIS pass was the one that
/// recorded it.
pub const INSERT_OFFLINE_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\  VALUES ($1::uuid, $2::uuid, $3::text, $4::bigint,
    \\          jsonb_build_object($5::text, $6::bigint), $6::bigint, $4::bigint)
    \\  ON CONFLICT (runner_id, dedup_key)
    \\    WHERE event_type = 'runner_offline' AND dedup_key IS NOT NULL
    \\  DO NOTHING
    \\  RETURNING 1
    \\)
    \\SELECT COUNT(*)::bigint FROM inserted
;

/// Release the affinity slots a dead runner still holds.
///
/// Runs once per due runner per cycle, which is why the `last_runner_id`
/// predicate needed an index (`idx_runner_affinity_last_runner_id_leased_until`,
/// schema slot 033) — it was a full scan of `runner_affinity` per runner, and
/// `last_runner_id` is a foreign key with `ON DELETE SET NULL` besides.
pub const EXPIRE_ACTIVE_LEASE_SLOTS =
    \\WITH expired AS (
    \\  UPDATE fleet.runner_affinity a
    \\  SET leased_until = $3, updated_at = $4
    \\  WHERE a.last_runner_id = $1::uuid
    \\    AND a.leased_until > $3
    \\    AND a.fleet_id IN (
    \\      SELECT l.fleet_id FROM fleet.runner_leases l
    \\      WHERE l.runner_id = $1::uuid AND l.status = $2
    \\    )
    \\  RETURNING 1
    \\)
    \\SELECT COUNT(*)::bigint FROM expired
;

/// Finish draining a runner once its last lease is gone, and record the
/// transition in the same statement.
///
/// The `NOT EXISTS` guard is the safety property: a draining runner still
/// holding an active lease is not drained, so the state flip can never orphan
/// running work. Both the flip and its event land atomically or neither does.
pub const MARK_DRAINED_IF_IDLE =
    \\WITH updated AS (
    \\  UPDATE fleet.runners r
    \\  SET admin_state = $2, updated_at = $3
    \\  WHERE r.id = $1::uuid AND r.admin_state = $4
    \\    AND NOT EXISTS (
    \\      SELECT 1 FROM fleet.runner_leases l
    \\      WHERE l.runner_id = r.id AND l.status = $5
    \\    )
    \\  RETURNING r.id
    \\), inserted AS (
    \\  INSERT INTO fleet.runner_events
    \\    (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\  SELECT $6::uuid, id, $7::text, $3::bigint,
    \\         jsonb_build_object($8::text, $4::text, $9::text, $2::text), NULL, $3::bigint
    \\  FROM updated
    \\  RETURNING 1
    \\)
    \\SELECT COUNT(*)::bigint FROM inserted
;

// ── Budget drain ────────────────────────────────────────────────────────────
// One ledger row holds a whole run's accumulated spend, and a run may last
// MAX_RUNTIME_MS (12h) against a rolling 24h window — so which window that spend
// falls in is not a question one timestamp can answer. The retired
// `metering_periods` table answered it with a row per slice; the ledger answers
// it with the run's span, `[created_at, last_charged_at]`, and APPORTIONS the
// total by how much of that span the window covers.
//
// Stamping the whole total on one instant instead would make the daily check
// all-or-nothing: a 12h run whose first charge predates the floor would count
// ZERO against a cap it had genuinely spent against, which under-enforces
// exactly where the amounts are largest.
//
// Apportioning assumes spend is spread evenly across a run — true for the
// time-based run fee, approximate for token cost. It is bounded by one run's
// total either way, where the all-or-nothing error was unbounded in both
// directions.

// Each CASE is one row's share of the window opening at that floor: none if the
// run stopped charging before it, all of it if the run began after it, else the
// covered fraction. `numeric` because nanos times a millisecond span overflows
// BIGINT. The two arms are spelled out rather than factored into a helper —
// this is a money path, and the SQL a reader sees here is the SQL that runs.

/// Drain totals at two window starts. `$3` and `$4` are the window instants.
/// No backdating parameter: `last_charged_at` states when a run stopped
/// charging, so the row filter is exact where the retired one was a heuristic.
pub const SELECT_BUDGET_DRAIN =
    \\SELECT
    \\  COALESCE(SUM(CASE
    \\    WHEN l.last_charged_at <= $3::bigint THEN 0
    \\    WHEN l.created_at >= $3::bigint THEN l.credit_deducted_nanos
    \\    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $3::bigint)
    \\         / NULLIF(l.last_charged_at - l.created_at, 0)
    \\  END), 0)::bigint,
    \\  COALESCE(SUM(CASE
    \\    WHEN l.last_charged_at <= $4::bigint THEN 0
    \\    WHEN l.created_at >= $4::bigint THEN l.credit_deducted_nanos
    \\    ELSE l.credit_deducted_nanos::numeric * (l.last_charged_at - $4::bigint)
    \\         / NULLIF(l.last_charged_at - l.created_at, 0)
    \\  END), 0)::bigint
    \\FROM billing.usage_ledger l
    \\WHERE l.workspace_id = $1::uuid AND l.fleet_id = $2::uuid
    \\  AND l.charge_type IN ($5, $6)
    \\  AND l.last_charged_at >= LEAST($3::bigint, $4::bigint)
;

/// The same drain, plus the fleet's declared budget, so the policy and the
/// spend it is checked against are read at one instant and cannot skew.
pub const SELECT_BUDGET_POLICY_AND_DRAIN =
    \\WITH drains AS (
    \\  SELECT l.credit_deducted_nanos AS amt,
    \\         l.created_at            AS first_at,
    \\         l.last_charged_at       AS last_at
    \\  FROM billing.usage_ledger l
    \\  WHERE l.workspace_id = $2::uuid AND l.fleet_id = $3::uuid
    \\    AND l.charge_type IN ($6, $7)
    \\    AND l.last_charged_at >= LEAST($4::bigint, $5::bigint)
    \\)
    \\SELECT
    \\  (z.config_json->'x-agentsfleet'->'budget')::text,
    \\  COALESCE((SELECT SUM(CASE
    \\    WHEN last_at <= $4::bigint THEN 0
    \\    WHEN first_at >= $4::bigint THEN amt
    \\    ELSE amt::numeric * (last_at - $4::bigint) / NULLIF(last_at - first_at, 0)
    \\  END) FROM drains), 0)::bigint,
    \\  COALESCE((SELECT SUM(CASE
    \\    WHEN last_at <= $5::bigint THEN 0
    \\    WHEN first_at >= $5::bigint THEN amt
    \\    ELSE amt::numeric * (last_at - $5::bigint) / NULLIF(last_at - first_at, 0)
    \\  END) FROM drains), 0)::bigint
    \\FROM core.fleets z
    \\WHERE z.id = $1::uuid
;

// ── Runner events ───────────────────────────────────────────────────────────

/// One page of a runner's event history, with a total that survives an offset
/// past the end. Every filter is optional at the SQL level (`$n IS NULL OR …`)
/// so one statement serves the filtered and unfiltered reads alike.
/// Filtered total for the runner-events read; the same predicate set as the
/// page statements so `total` always describes the filtered history.
pub const SELECT_RUNNER_EVENT_COUNT =
    \\SELECT COUNT(*)::bigint FROM fleet.runner_events
    \\WHERE runner_id = $1::uuid
    \\  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
    \\  AND ($3::bigint IS NULL OR occurred_at >= $3::bigint)
    \\  AND ($4::bigint IS NULL OR occurred_at <= $4::bigint)
;

/// One keyset page of runner events, newest first over `(occurred_at, id)` —
/// rides `runner_events_runner_idx (runner_id, occurred_at DESC, id DESC)`.
const RUNNER_EVENT_KEYSET_COLS =
    \\SELECT id::text, runner_id::text, event_type, occurred_at, metadata::text
    \\FROM fleet.runner_events
    \\WHERE runner_id = $1::uuid
    \\  AND ($2::text[] IS NULL OR event_type = ANY($2::text[]))
    \\  AND ($3::bigint IS NULL OR occurred_at >= $3::bigint)
    \\  AND ($4::bigint IS NULL OR occurred_at <= $4::bigint)
    \\
;

/// `$5` limit.
pub const SELECT_RUNNER_EVENT_KEYSET_FIRST = RUNNER_EVENT_KEYSET_COLS ++
    \\ORDER BY occurred_at DESC, id DESC
    \\LIMIT $5::bigint
;

/// `$5` boundary occurred_at, `$6` boundary event row id, `$7` limit.
pub const SELECT_RUNNER_EVENT_KEYSET_AFTER = RUNNER_EVENT_KEYSET_COLS ++
    \\  AND (occurred_at, id) < ($5::bigint, $6::uuid)
    \\ORDER BY occurred_at DESC, id DESC
    \\LIMIT $7::bigint
;

/// Record an operator-plane runner event. `dedup_key` is NULL here — only the
/// offline sweep dedupes, and a NULL key is excluded from its partial index.
pub const INSERT_RUNNER_EVENT =
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::text, $4::bigint,
    \\        jsonb_build_object($5::text, $6::text, $7::text, $8::text, $9::text, $10::text),
    \\        NULL, $4::bigint)
;

// ── Event rows ──────────────────────────────────────────────────────────────

/// Record an inbound event. `ON CONFLICT DO NOTHING` on `(fleet_id, event_id)`
/// is the idempotence boundary for redelivery: the same event arriving twice
/// writes one row, so a retrying producer cannot double-run a fleet.
pub const INSERT_FLEET_EVENT =
    \\INSERT INTO core.fleet_events
    \\  (uid, fleet_id, event_id, workspace_id, actor, event_type,
    \\   status, request_json, resumes_event_id, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, $5, $6, $10, $7::jsonb, $8, $9, $9)
    \\ON CONFLICT (fleet_id, event_id) DO NOTHING
;

/// Move an event to a terminal failure. The trailing `status = $6` is a guard,
/// not a filter: only an event still in the expected state transitions, so a
/// late writer cannot overwrite an already-settled outcome.
pub const UPDATE_FLEET_EVENT_FAILURE =
    \\UPDATE core.fleet_events
    \\SET status = $3, failure_label = $4, updated_at = $5
    \\WHERE fleet_id = $1::uuid AND event_id = $2 AND status = $6
;

pub const SELECT_FLEET_EVENT_STATUS =
    \\SELECT status FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2
;

/// Settle an event with its result. Same state guard as the failure path.
pub const UPDATE_FLEET_EVENT_RESULT =
    \\UPDATE core.fleet_events
    \\SET status = $3, response_text = $4, tokens = $5, wall_ms = $6, updated_at = $7, failure_label = $8, failure_detail = $10
    \\WHERE fleet_id = $1::uuid AND event_id = $2 AND status = $9
;

/// Checkpoint a fleet's session. One row per fleet, replaced in place.
pub const UPSERT_FLEET_SESSION =
    \\INSERT INTO core.fleet_sessions (id, fleet_id, context_json, checkpoint_at, created_at, updated_at)
    \\VALUES ($1, $2, $3, $4, $4, $4)
    \\ON CONFLICT (fleet_id) DO UPDATE
    \\  SET context_json = EXCLUDED.context_json,
    \\      checkpoint_at = EXCLUDED.checkpoint_at,
    \\      updated_at = EXCLUDED.updated_at
;

// ── Affinity slot ───────────────────────────────────────────────────────────

/// Claim a fleet's single runner slot, bumping the fencing sequence.
///
/// The `WHERE fleet.runner_affinity.leased_until < $4` on the conflict arm is
/// what makes this a lock rather than an upsert: a live slot is not stolen, and
/// the returned `fencing_seq` is the token every later write is checked
/// against, so a superseded holder cannot act on stale authority.
///
/// `fleet_id` is the whole primary key (schema/630), so the conflict target IS
/// the table's only unique index — two runners racing a brand-new fleet's slot
/// take the update arm rather than colliding on an index this statement does
/// not name.
pub const CLAIM_AFFINITY_SLOT =
    \\INSERT INTO fleet.runner_affinity
    \\  (fleet_id, last_runner_id, fencing_seq, leased_until,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, $4, $4)
    \\ON CONFLICT (fleet_id) DO UPDATE
    \\  SET last_runner_id = EXCLUDED.last_runner_id,
    \\      fencing_seq    = fleet.runner_affinity.fencing_seq + 1,
    \\      leased_until   = EXCLUDED.leased_until,
    \\      updated_at     = EXCLUDED.updated_at
    \\  WHERE fleet.runner_affinity.leased_until < $4
    \\RETURNING fencing_seq
;

/// Reset the slot's metering counters at the start of a fresh billing slice.
pub const RESET_AFFINITY_METERS =
    \\UPDATE fleet.runner_affinity
    \\SET metered_input_tokens = 0, metered_cached_tokens = 0,
    \\    metered_output_tokens = 0, last_metered_at = $2, updated_at = $2
    \\WHERE fleet_id = $1::uuid
;

/// Release the slot — fencing-guarded, so only the current holder can free it.
pub const RELEASE_AFFINITY_SLOT =
    \\UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2
    \\WHERE fleet_id = $1::uuid AND fencing_seq = $3
;

// ── Lease row ───────────────────────────────────────────────────────────────

/// Split to a sibling for the file-length budget; re-exported so query text
/// stays reachable through this module (RULE SQLMOD).
pub const INSERT_LEASE_WITH_EVENT = @import("sql_lease_row.zig").INSERT_LEASE_WITH_EVENT;
// ── Lease assignment ────────────────────────────────────────────────────────

/// Eligible active fleets for one lease poll, sticky-first and bounded.
///
/// Readiness NARROWS the input; it never decides eligibility. The label gate and
/// the sticky ordering are unchanged from the unbounded form — `required_tags <@
/// labels` still filters (empty tags ⊆ any labels ⇒ any runner) and the runner's
/// own affinity still sorts to the front. What is new is the `$3` membership
/// restriction to the fleets the readiness index reported, plus the `$4` ceiling
/// that makes per-poll cost independent of how many fleets exist.
///
/// The runner's labels (stored JSONB) bind as a constant TEXT[] via the
/// uncorrelated subquery, so `<@` stays a `column <@ constant` shape the
/// `required_tags` GIN index can serve — not a column-to-column join, which no
/// index serves.
///
/// `$1` active status, `$2` runner id, `$3` ready fleet ids, `$4` ceiling.
pub const SELECT_READY_CANDIDATES =
    \\SELECT z.id::text
    \\FROM core.fleets z
    \\LEFT JOIN fleet.runner_affinity a ON a.fleet_id = z.id
    \\WHERE z.status = $1
    \\  AND z.id = ANY(($3::text[])::uuid[])
    \\  AND z.required_tags <@ (
    \\        SELECT COALESCE(array_agg(e), '{}'::text[])
    \\        FROM jsonb_array_elements_text(
    \\               (SELECT CASE WHEN jsonb_typeof(labels) = 'array'
    \\                            THEN labels ELSE '[]'::jsonb END
    \\                FROM fleet.runners WHERE id = $2::uuid)
    \\             ) AS e
    \\      )
    \\ORDER BY (a.last_runner_id = $2::uuid) DESC NULLS LAST, z.created_at ASC
    \\LIMIT $4
;
