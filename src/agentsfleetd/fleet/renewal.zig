//! fleet — the `renew` operation: atomically extend a live lease's deadline AND
//! meter the slice of runtime + tokens consumed since the last renewal.
//!
//! Decouples lease liveness from execution duration. A runner that is actively
//! executing a Fleet calls `POST /v1/runners/me/leases/{id}/renew` inside the
//! renewal window; this pushes the kill deadline forward so a legitimate >30s
//! run is never reclaimed mid-flight, and charges the elapsed run fee + token
//! delta for that slice.
//!
//! The hard part is that reclaimability is driven by `runner_affinity.leased_until`
//! (the slot `affinity.claim` checks), a SEPARATE row from `runner_leases.
//! lease_expires_at` (the child kill deadline). Renewing one but not the other
//! still gets a healthy run reclaimed at the TTL. So `renew` extends BOTH rows
//! in ONE writable-CTE statement, to the SAME clamped value, guarded by the same
//! live fence `service_report` uses (`fencing_token >= fencing_seq`) plus
//! `status = 'active'`. The check and the two writes share one snapshot — a
//! concurrent reclaim cannot split them.
//!
//! Metering rides that same fenced statement. The `guard` arm gates EVERY write:
//! advance both cursors, debit the wallet (clamped, never negative), accumulate
//! the per-event `stage` telemetry row, and INSERT the per-renewal breakdown —
//! a lost/capped renewal writes none of them. The Δ is computed off the AFFINITY
//! cursor (the durable per-fleet anchor that survives reclaim), so a re-sent
//! renewal charges ≈0 (cumulative-diff idempotency). The four per-unit rates are
//! resolved in Zig (`tenant_billing.resolveRenewSliceRates`) and passed in, so
//! the slice math here is the SAME as `computeStageCharge` — SQL==Zig by
//! construction (free-trial / self_managed / platform are all encoded as rates).
//!
//! Runs on a caller-supplied pooled connection (drained via PgQuery).

const pg = @import("pg");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const constants = @import("common");
const protocol = @import("contract").protocol;
const id_format = @import("../types/id_format.zig");
const telemetry = @import("../state/fleet_telemetry_store.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const log = logging.scoped(.fleet_metering);
const MS_PER_SECOND: i64 = 1000;
const TOKENS_PER_MTOK: i64 = 1000000;

/// The runner's cumulative token counts + the resolved per-unit slice rates for
/// this renewal. Cumulatives are diffed against the lease's metering cursor IN
/// the CTE (this struct never carries deltas — no double-count). Rates already
/// encode posture + free-trial (all-zero during the trial; token tiers zero
/// under self_managed), so the SQL applies them uniformly.
pub const MeterInputs = struct {
    cumulative_input: i64 = 0,
    cumulative_cached: i64 = 0,
    cumulative_output: i64 = 0,
    run_nanos_per_sec: i64 = 0,
    input_nanos_per_mtok: i64 = 0,
    cached_input_nanos_per_mtok: i64 = 0,
    output_nanos_per_mtok: i64 = 0,
};

/// Resolve the four slice rates (free-trial / posture aware) and pair them with
/// the runner's cumulative token counts. Shared by `renew` (service_renew) and
/// `settle` (service_report) so both meter at the identical rates. A platform
/// model absent from the rate cache (a cache-eviction edge — a lease could not
/// have issued for an uncatalogued platform model) meters run-fee-only + logs;
/// the live renew/report path never panics.
pub fn buildMeterInputs(
    provider: []const u8,
    posture: tenant_provider.Mode,
    model: []const u8,
    now_ms: i64,
    cum_input: u32,
    cum_cached: u32,
    cum_output: u32,
) MeterInputs {
    const rates = tenant_billing.resolveRenewSliceRates(provider, posture, model, now_ms) orelse blk: {
        log.warn("meter_rate_missing_run_fee_only", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .provider = provider, .model = model });
        break :blk tenant_billing.SliceRates{ .run_nanos_per_sec = tenant_billing.RUN_NANOS_PER_SEC, .input_nanos_per_mtok = 0, .cached_input_nanos_per_mtok = 0, .output_nanos_per_mtok = 0 };
    };
    return .{
        .cumulative_input = @intCast(cum_input),
        .cumulative_cached = @intCast(cum_cached),
        .cumulative_output = @intCast(cum_output),
        .run_nanos_per_sec = rates.run_nanos_per_sec,
        .input_nanos_per_mtok = rates.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = rates.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = rates.output_nanos_per_mtok,
    };
}

/// The verdict of a renewal attempt. A tagged union so the handler can map each
/// case to its own wire code without re-deriving context (UFS/type-design rule).
pub const RenewOutcome = union(enum) {
    /// Both rows advanced to this `lease_expires_at` (epoch ms); the slice was
    /// metered + charged.
    renewed: i64,
    /// Still the live holder, but `created_at + MAX_RUNTIME_MS` is reached — the
    /// run must terminate (UZ-RUN-010). Carries the cap for logging.
    max_runtime: i64,
    /// The lease is no longer `active` or no longer ours (reclaimed/fenced) —
    /// the runner must kill its child (UZ-RUN-011).
    lost,
};

// One writable-CTE statement. `probe` reads lease+slot and the cursor deltas
// (clamped ≥0) under `FOR UPDATE OF l, a` — that lock SERIALISES renewals of
// the same lease (re-read advanced cursor → ≈0 charge, no double-charge).
// `bal` locks the tenant's billing row in its own CTE: Postgres refuses
// `FOR UPDATE` on the nullable side of an outer join, and the row must stay
// optional (a tenant without a billing row still renews), so the lock cannot
// ride probe's LEFT JOIN. The lock serialises same-tenant money ops and makes
// `bal0` the LIVE balance after any lock wait, not a stale pre-lock read.
// `calc` prices the slice; `guard` survives only if the fence holds and the
// cap is not yet reached, and computes `charged = LEAST(slice, bal0)` — equal
// to the wallet's actual delta (`bal0 − GREATEST(0, bal0 − slice)`) in every
// interleaving, so two concurrent same-tenant renewals at exhaustion record
// audit rows summing to the real drain, never more. `ext_*` advance both
// rows' deadline AND cursor, clamping the token cursors with
// `GREATEST(old, $n)` so a regressed cumulative report never rewinds them.
// `wallet`/`ledger`/`breakdown` are the three guard-gated money writes. The
// trailing SELECT disambiguates renewed / max_runtime / lost in one
// round-trip.
const RENEW_METER_SQL =
    \\WITH probe AS (
    \\    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
    \\           l.created_at, l.fencing_token, l.posture, l.model, a.fencing_seq,
    \\           a.meter_slice_seq,
    \\           LEAST($3::bigint, l.created_at + $4::bigint) AS capped,
    \\           GREATEST(0, $6::bigint - a.last_metered_at_ms)      AS d_ms,
    \\           GREATEST(0, $7::bigint - a.metered_input_tokens)    AS d_in,
    \\           GREATEST(0, $8::bigint - a.metered_cached_tokens)   AS d_cached,
    \\           GREATEST(0, $9::bigint - a.metered_output_tokens)   AS d_out
    \\    FROM fleet.runner_leases l
    \\    JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $5
    \\    FOR UPDATE OF l, a
    \\), bal AS (
    \\    SELECT tb.tenant_id, tb.balance_nanos AS bal0
    \\    FROM billing.tenant_billing tb
    \\    JOIN probe p ON p.tenant_id = tb.tenant_id
    \\    FOR UPDATE OF tb
    \\), calc AS (
    \\    SELECT p.*, b.bal0,
    \\           (d_ms * $10::bigint) / $15::bigint    AS run_fee,
    \\           (d_in * $11::bigint) / $16::bigint
    \\             + (d_cached * $12::bigint) / $16::bigint
    \\             + (d_out * $13::bigint) / $16::bigint  AS token_cost
    \\    FROM probe p
    \\    LEFT JOIN bal b ON b.tenant_id = p.tenant_id
    \\), guard AS (
    \\    SELECT *, run_fee + token_cost AS slice,
    \\           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged,
    \\           meter_slice_seq + 1 AS next_seq
    \\    FROM calc
    \\    WHERE fencing_token >= fencing_seq AND capped > $6::bigint
    \\), ext_lease AS (
    \\    UPDATE fleet.runner_leases l
    \\    SET lease_expires_at = g.capped, updated_at = $6,
    \\        metered_input_tokens = GREATEST(l.metered_input_tokens, $7),
    \\        metered_cached_tokens = GREATEST(l.metered_cached_tokens, $8),
    \\        metered_output_tokens = GREATEST(l.metered_output_tokens, $9),
    \\        last_metered_at_ms = $6
    \\    FROM guard g WHERE l.id = g.id
    \\    RETURNING g.capped
    \\), ext_aff AS (
    \\    UPDATE fleet.runner_affinity a
    \\    SET leased_until = g.capped, updated_at = $6,
    \\        metered_input_tokens = GREATEST(a.metered_input_tokens, $7),
    \\        metered_cached_tokens = GREATEST(a.metered_cached_tokens, $8),
    \\        metered_output_tokens = GREATEST(a.metered_output_tokens, $9),
    \\        last_metered_at_ms = $6,
    \\        meter_slice_seq = g.next_seq
    \\    FROM guard g WHERE a.fleet_id = g.fleet_id
    \\    RETURNING a.fleet_id
    \\), wallet AS (
    \\    UPDATE billing.tenant_billing tb
    \\    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
    \\        balance_exhausted_at = CASE
    \\            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $6)
    \\            ELSE NULL END,
    \\        updated_at = $6
    \\    FROM guard g WHERE tb.tenant_id = g.tenant_id
    \\    RETURNING tb.tenant_id
    \\), ledger AS (
    \\    INSERT INTO core.fleet_execution_telemetry
    \\      (uid, id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
    \\       model, credit_deducted_nanos, token_count_input, token_count_output,
    \\       wall_ms, recorded_at)
    \\    SELECT $17::uuid, 'mtr_' || g.event_id, g.tenant_id, g.workspace_id::text,
    \\           g.fleet_id::text, g.event_id, $14, g.posture, g.model,
    \\           g.charged, g.d_in, g.d_out, g.d_ms, $6
    \\    FROM guard g
    \\    ON CONFLICT (event_id, charge_type) DO UPDATE SET
    \\        credit_deducted_nanos = core.fleet_execution_telemetry.credit_deducted_nanos
    \\            + EXCLUDED.credit_deducted_nanos,
    \\        token_count_input  = COALESCE(core.fleet_execution_telemetry.token_count_input, 0)
    \\            + EXCLUDED.token_count_input,
    \\        token_count_output = COALESCE(core.fleet_execution_telemetry.token_count_output, 0)
    \\            + EXCLUDED.token_count_output,
    \\        wall_ms = COALESCE(core.fleet_execution_telemetry.wall_ms, 0) + EXCLUDED.wall_ms
    \\    RETURNING event_id
    \\), breakdown AS (
    \\    INSERT INTO fleet.metering_periods
    \\      (uid, event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
    \\       run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
    \\    SELECT $18::uuid, g.event_id, g.next_seq,
    \\           g.d_in, g.d_cached, g.d_out, g.d_ms, g.run_fee, g.token_cost, g.charged, $6
    \\    FROM guard g
    \\    RETURNING event_id
    \\)
    \\SELECT
    \\    (SELECT count(*) FROM probe)::bigint        AS probe_found,
    \\    (SELECT capped FROM ext_lease)              AS new_until,
    \\    (SELECT created_at + $4::bigint FROM probe) AS hard_cap,
    \\    (SELECT count(*) FROM ext_aff)::bigint      AS aff_updated
;

/// Atomically extend the lease + slot deadline to `min(now + LEASE_TTL_MS,
/// created_at + MAX_RUNTIME_MS)` AND meter the slice since the last renewal,
/// guarded by `status = 'active'` AND the presenting runner still being the live
/// fencing holder. All writes ride one fenced statement: both rows advance and
/// the wallet/ledger/breakdown are charged, or none do.
pub fn renew(
    conn: *pg.Conn,
    lease_id: []const u8,
    runner_id: []const u8,
    now_ms: i64,
    meter: MeterInputs,
) !RenewOutcome {
    const want_until = now_ms + constants.LEASE_TTL_MS;
    const ledger_uid_value = try id_format.generateUuidV7();
    const breakdown_uid_value = try id_format.generateUuidV7();
    const ledger_uid: []const u8 = &ledger_uid_value;
    const breakdown_uid: []const u8 = &breakdown_uid_value;
    var q = PgQuery.from(try conn.query(RENEW_METER_SQL, .{
        lease_id,
        runner_id,
        want_until,
        constants.MAX_RUNTIME_MS,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        now_ms,
        meter.cumulative_input,
        meter.cumulative_cached,
        meter.cumulative_output,
        meter.run_nanos_per_sec,
        meter.input_nanos_per_mtok,
        meter.cached_input_nanos_per_mtok,
        meter.output_nanos_per_mtok,
        telemetry.ChargeType.stage.label(),
        MS_PER_SECOND,
        TOKENS_PER_MTOK,
        ledger_uid,
        breakdown_uid,
    }));
    defer q.deinit();
    const row = try q.next() orelse return .lost;
    return mapOutcome(
        try row.get(i64, 0),
        try row.get(?i64, 1),
        try row.get(?i64, 2),
        try row.get(i64, 3),
        now_ms,
    );
}

/// Translate the trailing SELECT's four columns into the verdict. Both rows must
/// advance together: if `ext_lease` wrote but `ext_aff` did not (a concurrent
/// reclaim touched the affinity row between the snapshot and the UPDATE's
/// EvalPlanQual recheck), the slot can be reclaimed before the deadline we'd
/// report — so a half-applied renewal is `.lost`, killing the child cleanly.
fn mapOutcome(probe_found: i64, new_until: ?i64, hard_cap: ?i64, aff_updated: i64, now_ms: i64) RenewOutcome {
    if (new_until) |until| {
        if (aff_updated == 1) return .{ .renewed = until };
        return .lost;
    }
    if (probe_found == 0) return .lost;
    // Still ours+active, so the guard failed on the cap (capped <= now) or a
    // stale fence. The cap is the deterministic, reported case; a stale fence
    // means a reclaim already won → also lost.
    if (hard_cap) |cap| if (cap <= now_ms) return .{ .max_runtime = cap };
    return .lost;
}
