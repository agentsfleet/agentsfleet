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
//! advance both cursors, debit the wallet (clamped, never negative), and
//! accumulate the per-event `stage` row in `billing.usage_ledger`, stamping
//! `last_charged_at` so the budget gate can apportion the run's accumulated
//! total across the span it actually charged over (M154 §4 deleted the
//! per-renewal breakdown table that used to carry that detail row by row) —
//! a lost/capped renewal writes none of them. The Δ is computed off the AFFINITY
//! cursor (the durable per-fleet anchor that survives reclaim), so a re-sent
//! renewal charges ≈0 (cumulative-diff idempotency). The four per-unit rates are
//! resolved in Zig (`tenant_billing_rates.resolveRenewSliceRates`) and passed in, so
//! the slice math here is the SAME as `computeStageCharge` — SQL==Zig by
//! construction (free-trial / self_managed / platform are all encoded as rates).
//!
//! Runs on a caller-supplied pooled connection (drained via PgQuery).

const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");
const constants = @import("common");
const protocol = @import("contract").protocol;
const id_format = @import("../types/id_format.zig");
const telemetry = @import("../state/fleet_telemetry_store.zig");
const renewal_meter = @import("renewal_meter.zig");

const MS_PER_SECOND: i64 = 1000;
const TOKENS_PER_MTOK: i64 = 1000000;

/// A renewal that committed: both rows advanced and the slice was metered.
pub const Renewed = struct {
    /// The new `lease_expires_at` (epoch ms) both rows now carry.
    lease_expires_at: i64,
    /// Nanocredits this slice actually drained — `LEAST(slice, balance)`, so it
    /// equals the wallet's real delta even at exhaustion. Returned because the
    /// service layer emits the credit metric for it *after* this statement
    /// commits; without it, renewal drain would be invisible to operators while
    /// receive and settle were not.
    charged_nanos: i64,
};

/// The verdict of a renewal attempt. A tagged union so the handler can map each
/// case to its own wire code without re-deriving context (UFS/type-design rule).
pub const RenewOutcome = union(enum) {
    renewed: Renewed,
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
// `wallet` and `ledger` are the two guard-gated money writes. The trailing
// SELECT disambiguates renewed / max_runtime / lost in one round-trip.
//
// `event_created_at` rides the probe select purely so the ledger arm can stamp
// it: every row for one event must carry the same value, and it is the EVENT's
// instant, not this renewal's. The `SELECT p.*` / `SELECT *` chain through
// `calc` and `guard` is what carries it down.
const RENEW_METER_SQL =
    \\WITH probe AS (
    \\    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
    \\           l.created_at, l.event_created_at,
    \\           l.fencing_token, l.posture, l.model, a.fencing_seq,
    \\           LEAST($3::bigint, l.created_at + $4::bigint) AS capped,
    \\           GREATEST(0, $6::bigint - a.last_metered_at)         AS d_ms,
    \\           GREATEST(0, $7::bigint - a.metered_input_tokens)    AS d_in,
    \\           GREATEST(0, $8::bigint - a.metered_cached_tokens)   AS d_cached,
    \\           GREATEST(0, $9::bigint - a.metered_output_tokens)   AS d_out
    \\    FROM fleet.runner_leases l
    \\    JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $5
    \\    FOR UPDATE OF l, a
    \\), bal AS (
    \\    SELECT tb.tenant_id, tb.balance_nanos AS bal0
    \\    FROM billing.tenant_wallet tb
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
    \\           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged
    \\    FROM calc
    \\    WHERE fencing_token >= fencing_seq AND capped > $6::bigint
    \\), ext_lease AS (
    \\    UPDATE fleet.runner_leases l
    \\    SET lease_expires_at = g.capped, updated_at = $6,
    \\        metered_input_tokens = GREATEST(l.metered_input_tokens, $7),
    \\        metered_cached_tokens = GREATEST(l.metered_cached_tokens, $8),
    \\        metered_output_tokens = GREATEST(l.metered_output_tokens, $9),
    \\        last_metered_at = $6
    \\    FROM guard g WHERE l.id = g.id
    \\    RETURNING g.capped, g.charged
    \\), ext_aff AS (
    \\    UPDATE fleet.runner_affinity a
    \\    SET leased_until = g.capped, updated_at = $6,
    \\        metered_input_tokens = GREATEST(a.metered_input_tokens, $7),
    \\        metered_cached_tokens = GREATEST(a.metered_cached_tokens, $8),
    \\        metered_output_tokens = GREATEST(a.metered_output_tokens, $9),
    \\        last_metered_at = $6
    \\    FROM guard g WHERE a.fleet_id = g.fleet_id
    \\    RETURNING a.fleet_id
    \\), wallet AS (
    \\    UPDATE billing.tenant_wallet tb
    \\    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
    \\        balance_exhausted_at = CASE
    \\            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $6)
    \\            ELSE NULL END,
    \\        updated_at = $6
    \\    FROM guard g WHERE tb.tenant_id = g.tenant_id
    \\    RETURNING tb.tenant_id
    \\), ledger AS (
    \\    INSERT INTO billing.usage_ledger
    \\      (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
    \\       model, credit_deducted_nanos, token_count_input, token_count_cached_input,
    \\       token_count_output, wall_ms, event_created_at, created_at, last_charged_at)
    \\    SELECT $17::uuid, g.tenant_id, g.workspace_id, g.fleet_id, g.event_id, $14,
    \\           g.posture, g.model, g.charged, g.d_in, g.d_cached, g.d_out, g.d_ms,
    \\           g.event_created_at, $6, $6
    \\    FROM guard g
    \\    ON CONFLICT (event_id, charge_type) DO UPDATE SET
    \\        credit_deducted_nanos = billing.usage_ledger.credit_deducted_nanos
    \\            + EXCLUDED.credit_deducted_nanos,
    \\        token_count_input  = COALESCE(billing.usage_ledger.token_count_input, 0)
    \\            + EXCLUDED.token_count_input,
    \\        token_count_cached_input = COALESCE(billing.usage_ledger.token_count_cached_input, 0)
    \\            + EXCLUDED.token_count_cached_input,
    \\        token_count_output = COALESCE(billing.usage_ledger.token_count_output, 0)
    \\            + EXCLUDED.token_count_output,
    \\        wall_ms = COALESCE(billing.usage_ledger.wall_ms, 0) + EXCLUDED.wall_ms,
    \\        last_charged_at = GREATEST(billing.usage_ledger.last_charged_at,
    \\                                   EXCLUDED.last_charged_at)
    \\    RETURNING event_id
    \\)
    \\SELECT
    \\    (SELECT count(*) FROM probe)::bigint        AS probe_found,
    \\    (SELECT capped FROM ext_lease)              AS new_until,
    \\    (SELECT created_at + $4::bigint FROM probe) AS hard_cap,
    \\    (SELECT count(*) FROM ext_aff)::bigint      AS aff_updated,
    \\    (SELECT charged FROM ext_lease)             AS charged_nanos
;

/// Atomically extend the lease + slot deadline to `min(now + LEASE_TTL_MS,
/// created_at + MAX_RUNTIME_MS)` AND meter the slice since the last renewal,
/// guarded by `status = 'active'` AND the presenting runner still being the live
/// fencing holder. All writes ride one fenced statement: both rows advance and
/// the wallet and ledger are charged, or none do.
/// The fenced renew statement's bound values, and the one elevated call that
/// issues it.
const RenewMeterArgs = struct {
    lease_id: []const u8,
    runner_id: []const u8,
    want_until: i64,
    now_ms: i64,
    meter: renewal_meter.MeterInputs,
    ledger_uid: []const u8,

    /// The result drains (defer) before this returns and the commit runs:
    /// COMMIT with a result in flight is a protocol error.
    fn run(c: RenewMeterArgs, v: pool_elevation.Elevated(.metering)) !?OutcomeRow {
        var q = PgQuery.from(try v.conn.query(RENEW_METER_SQL, .{
            c.lease_id,
            c.runner_id,
            c.want_until,
            constants.MAX_RUNTIME_MS,
            protocol.RUNNER_LEASE_STATUS_ACTIVE,
            c.now_ms,
            c.meter.cumulative_input,
            c.meter.cumulative_cached,
            c.meter.cumulative_output,
            c.meter.run_nanos_per_sec,
            c.meter.input_nanos_per_mtok,
            c.meter.cached_input_nanos_per_mtok,
            c.meter.output_nanos_per_mtok,
            telemetry.ChargeType.stage.label(),
            MS_PER_SECOND,
            TOKENS_PER_MTOK,
            c.ledger_uid,
        }));
        defer q.deinit();
        const row = try q.next() orelse return null;
        return .{
            .probe_found = try row.get(i64, 0),
            .new_until = try row.get(?i64, 1),
            .hard_cap = try row.get(?i64, 2),
            .aff_updated = try row.get(i64, 3),
            .charged_nanos = try row.get(?i64, 4),
        };
    }
};

pub fn renew(
    conn: *pg.Conn,
    lease_id: []const u8,
    runner_id: []const u8,
    now_ms: i64,
    meter: renewal_meter.MeterInputs,
) !RenewOutcome {
    const want_until = now_ms + constants.LEASE_TTL_MS;
    const ledger_uid_value = try id_format.generateUuidV7();
    const ledger_uid: []const u8 = &ledger_uid_value;

    // The statement's tables belong to `metering_runtime` (schema/120), not to
    // the connection's `api_runtime`. The statement itself is not modified —
    // the elevation transaction brackets the same single fenced statement, so
    // its atomicity argument (charge and cursor advance commit together) is
    // untouched.
    const outcome = try pool_elevation.withRole(conn, .metering, RenewMeterArgs{
        .lease_id = lease_id,
        .runner_id = runner_id,
        .want_until = want_until,
        .now_ms = now_ms,
        .meter = meter,
        .ledger_uid = ledger_uid,
    }, RenewMeterArgs.run);
    return mapOutcome(outcome orelse return .lost, now_ms);
}

/// The trailing SELECT's five columns, named so `mapOutcome` cannot transpose
/// two same-typed positional arguments.
const OutcomeRow = struct {
    probe_found: i64,
    new_until: ?i64,
    hard_cap: ?i64,
    aff_updated: i64,
    charged_nanos: ?i64,
};

/// Translate the trailing SELECT's four columns into the verdict. Both rows must
/// advance together: if `ext_lease` wrote but `ext_aff` did not (a concurrent
/// reclaim touched the affinity row between the snapshot and the UPDATE's
/// EvalPlanQual recheck), the slot can be reclaimed before the deadline we'd
/// report — so a half-applied renewal is `.lost`, killing the child cleanly.
fn mapOutcome(row: OutcomeRow, now_ms: i64) RenewOutcome {
    if (row.new_until) |until| {
        if (row.aff_updated != 1) return .lost;
        // A guard row always prices a charge, so a null here would mean the
        // guard survived without `calc` — treat it as zero drain rather than
        // reporting a debit the wallet never took.
        return .{ .renewed = .{ .lease_expires_at = until, .charged_nanos = row.charged_nanos orelse 0 } };
    }
    if (row.probe_found == 0) return .lost;
    // Still ours+active, so the guard failed on the cap (capped <= now) or a
    // stale fence. The cap is the deterministic, reported case; a stale fence
    // means a reclaim already won → also lost.
    if (row.hard_cap) |cap| if (cap <= now_ms) return .{ .max_runtime = cap };
    return .lost;
}
