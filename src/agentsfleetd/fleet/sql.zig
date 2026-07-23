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
// Both reads union the two ways credit leaves an account — metered execution
// (joined through `metering_periods` for the settled figure) and direct
// deductions — then window the union at two instants in one pass, so the
// per-period and per-window totals cannot disagree with each other.

/// Drain totals at two window starts. `$3` and `$4` are the window instants;
/// `$7` backdates the metered leg so a period settling late still lands in it.
pub const SELECT_BUDGET_DRAIN =
    \\WITH drains AS (
    \\  SELECT mp.charged_nanos AS amt, mp.created_at AS ts
    \\  FROM core.fleet_execution_telemetry t
    \\  JOIN fleet.metering_periods mp ON mp.event_id = t.event_id
    \\  WHERE t.workspace_id = $1 AND t.fleet_id = $2 AND t.charge_type = $5
    \\    AND t.recorded_at >= LEAST($3::bigint, $4::bigint) - $7::bigint
    \\  UNION ALL
    \\  SELECT r.credit_deducted_nanos AS amt, r.recorded_at AS ts
    \\  FROM core.fleet_execution_telemetry r
    \\  WHERE r.workspace_id = $1 AND r.fleet_id = $2 AND r.charge_type = $6
    \\    AND r.recorded_at >= LEAST($3::bigint, $4::bigint)
    \\)
    \\SELECT
    \\  COALESCE(SUM(amt) FILTER (WHERE ts >= $3::bigint), 0)::bigint,
    \\  COALESCE(SUM(amt) FILTER (WHERE ts >= $4::bigint), 0)::bigint
    \\FROM drains
;

/// The same drain, plus the fleet's declared budget, so the policy and the
/// spend it is checked against are read at one instant and cannot skew.
pub const SELECT_BUDGET_POLICY_AND_DRAIN =
    \\WITH drains AS (
    \\  SELECT mp.charged_nanos AS amt, mp.created_at AS ts
    \\  FROM core.fleet_execution_telemetry t
    \\  JOIN fleet.metering_periods mp ON mp.event_id = t.event_id
    \\  WHERE t.workspace_id = $2 AND t.fleet_id = $3 AND t.charge_type = $6
    \\    AND t.recorded_at >= LEAST($4::bigint, $5::bigint) - $8::bigint
    \\  UNION ALL
    \\  SELECT r.credit_deducted_nanos AS amt, r.recorded_at AS ts
    \\  FROM core.fleet_execution_telemetry r
    \\  WHERE r.workspace_id = $2 AND r.fleet_id = $3 AND r.charge_type = $7
    \\    AND r.recorded_at >= LEAST($4::bigint, $5::bigint)
    \\)
    \\SELECT
    \\  (z.config_json->'x-agentsfleet'->'budget')::text,
    \\  COALESCE((SELECT SUM(amt) FROM drains WHERE ts >= $4::bigint), 0)::bigint,
    \\  COALESCE((SELECT SUM(amt) FROM drains WHERE ts >= $5::bigint), 0)::bigint
    \\FROM core.fleets z
    \\WHERE z.id = $1::uuid
;

// ── Runner events ───────────────────────────────────────────────────────────

/// One page of a runner's event history, with a total that survives an offset
/// past the end. Every filter is optional at the SQL level (`$n IS NULL OR …`)
/// so one statement serves the filtered and unfiltered reads alike.
pub const SELECT_RUNNER_EVENT_PAGE =
    \\WITH filtered AS (
    \\  SELECT id::text, runner_id::text, event_type, occurred_at, metadata::text
    \\  FROM fleet.runner_events
    \\  WHERE runner_id = $1::uuid
    \\    AND ($2::text IS NULL OR event_type = $2::text)
    \\    AND ($3::bigint IS NULL OR occurred_at >= $3::bigint)
    \\    AND ($4::bigint IS NULL OR occurred_at <= $4::bigint)
    \\),
    \\page AS (
    \\  SELECT id, runner_id, event_type, occurred_at, metadata,
    \\    COUNT(*) OVER()::bigint AS total,
    \\    false AS count_only,
    \\    ROW_NUMBER() OVER (ORDER BY occurred_at DESC, id DESC)::bigint AS page_ord
    \\  FROM filtered
    \\  ORDER BY occurred_at DESC, id DESC
    \\  LIMIT $5::bigint OFFSET $6::bigint
    \\),
    \\total_row AS (
    \\  SELECT NULL::text AS id, NULL::text AS runner_id, NULL::text AS event_type,
    \\    0::bigint AS occurred_at, NULL::text AS metadata,
    \\    (SELECT COUNT(*)::bigint FROM filtered) AS total,
    \\    true AS count_only,
    \\    NULL::bigint AS page_ord
    \\  WHERE NOT EXISTS (SELECT 1 FROM page)
    \\)
    \\SELECT * FROM page
    \\UNION ALL
    \\SELECT * FROM total_row
    \\ORDER BY count_only ASC, page_ord ASC NULLS LAST
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
/// The `WHERE fleet.runner_affinity.leased_until < $5` on the conflict arm is
/// what makes this a lock rather than an upsert: a live slot is not stolen, and
/// the returned `fencing_seq` is the token every later write is checked
/// against, so a superseded holder cannot act on stale authority.
pub const CLAIM_AFFINITY_SLOT =
    \\INSERT INTO fleet.runner_affinity
    \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, $4, 0, 0, 0, $5, $5, $5)
    \\ON CONFLICT (fleet_id) DO UPDATE
    \\  SET last_runner_id = EXCLUDED.last_runner_id,
    \\      fencing_seq    = fleet.runner_affinity.fencing_seq + 1,
    \\      leased_until   = EXCLUDED.leased_until,
    \\      updated_at     = EXCLUDED.updated_at
    \\  WHERE fleet.runner_affinity.leased_until < $5
    \\RETURNING fencing_seq
;

/// Reset the slot's metering counters at the start of a fresh billing slice.
pub const RESET_AFFINITY_METERS =
    \\UPDATE fleet.runner_affinity
    \\SET metered_input_tokens = 0, metered_cached_tokens = 0,
    \\    metered_output_tokens = 0, last_metered_at_ms = $2, updated_at = $2,
    \\    meter_slice_seq = 0
    \\WHERE fleet_id = $1::uuid
;

/// Release the slot — fencing-guarded, so only the current holder can free it.
pub const RELEASE_AFFINITY_SLOT =
    \\UPDATE fleet.runner_affinity SET leased_until = $2, updated_at = $2
    \\WHERE fleet_id = $1::uuid AND fencing_seq = $3
;

// ── Lease row ───────────────────────────────────────────────────────────────

/// Open a lease and record the event that opened it, atomically. Writing the
/// lease and its audit trail in one statement means an observer can never see a
/// lease with no corresponding event, or the reverse.
pub const INSERT_LEASE_WITH_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runner_leases
    \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id,
    \\   actor, event_type, request_json, event_created_at,
    \\   posture, provider, model,
    \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
    \\   fencing_token, lease_expires_at, status,
    \\   created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
    \\        $7, $8, $9, $10, $11, $12, $13,
    \\        0, 0, 0, $17,
    \\        $14, $15, $16, $17, $17)
    \\  RETURNING id, runner_id, fleet_id, event_id
    \\)
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
    \\SELECT $18::uuid, runner_id, $19::text, $17::bigint,
    \\       jsonb_build_object($20::text, id::text, $21::text, fleet_id::text, $22::text, event_id, $23::text, $24::text),
    \\       NULL, $17::bigint
    \\FROM inserted
;
