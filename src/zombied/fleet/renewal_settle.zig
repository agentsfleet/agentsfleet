//! fleet — the `report` settle: atomically CLAIM the terminal report (flip the
//! lease active→reported, fenced) AND meter the FINAL partial slice in ONE
//! writable-CTE statement.
//!
//! The claim and the settle share one snapshot and one row lock. The probe reads
//! lease+slot+balance under `FOR UPDATE OF l, a`, the `guard` arm requires the
//! presenter still hold the live fence (`fencing_token >= fencing_seq`), the
//! `claim` arm flips the lease to `reported` (only from `active`), and the same
//! three guard-gated money writes the renewal does charge the final slice. Fusing
//! the two removes the report→settle race: a concurrent reclaim that would bump
//! `fencing_seq` blocks on the affinity row lock until this commits — by then the
//! lease is `reported` and the slice is charged, so no final slice is ever lost
//! on the MAX_RUNTIME cap path (the fence ownership that authorizes reporting
//! authorizes settlement).
//!
//! Charges `now - last_metered` of run fee + the final token delta via the same
//! rates as `/renew` (shared `renewal.buildMeterInputs`), so a run that finished
//! inside one renewal window (never renewed) is still charged its real runtime
//! and gets its telemetry + breakdown rows. Advances BOTH cursors so a replay
//! settles ≈0. `charged = LEAST(slice, balance)` clamps the audit rows to the
//! actual drain. Runs on a caller-supplied pooled connection (drained via
//! PgQuery).

const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;
const telemetry = @import("../state/zombie_telemetry_store.zig");
const renewal = @import("renewal.zig");

const MS_PER_SECOND: i64 = 1000;
const TOKENS_PER_MTOK: i64 = 1000000;

/// The verdict of a claim+settle. `claimed` is the fenced active→reported flip
/// (this holder won the report); `charged_nanos` is the final slice debited (0
/// when fenced out or nothing was owed). A `claimed == false` result means the
/// lease was superseded or already reported — the caller rejects UZ-RUN-005.
pub const SettleOutcome = struct {
    claimed: bool,
    charged_nanos: i64,
};

// One writable-CTE statement that claims AND settles. `probe` reads the lease
// (only while `status = active`), the affinity cursor, and the balance under
// `FOR UPDATE OF l, a` — the affinity lock is what serialises a racing reclaim
// behind this statement. `calc`/`guard` price the slice and compute `charged =
// LEAST(slice, balance)` exactly as the renew CTE does; `guard` survives only if
// the fence holds. `claim` flips active→reported AND advances the lease cursor;
// `ext_aff` advances the slot cursor; `wallet`/`ledger`/`breakdown` are the
// guard-gated money writes (drain `GREATEST(0, balance - slice)` = charged,
// record `charged`). The trailing SELECT returns the charged nanos + whether the
// claim flipped a row (the report-won signal).
const CLAIM_SETTLE_SQL =
    \\WITH probe AS (
    \\    SELECT l.id, l.zombie_id, l.workspace_id, l.tenant_id, l.event_id,
    \\           l.posture, l.model, l.fencing_token, a.fencing_seq, a.meter_slice_seq,
    \\           GREATEST(0, $3::bigint - a.last_metered_at_ms)      AS d_ms,
    \\           GREATEST(0, $4::bigint - a.metered_input_tokens)    AS d_in,
    \\           GREATEST(0, $5::bigint - a.metered_cached_tokens)   AS d_cached,
    \\           GREATEST(0, $6::bigint - a.metered_output_tokens)   AS d_out,
    \\           tb.balance_nanos AS bal0
    \\    FROM fleet.runner_leases l
    \\    JOIN fleet.runner_affinity a ON a.zombie_id = l.zombie_id
    \\    LEFT JOIN billing.tenant_billing tb ON tb.tenant_id = l.tenant_id
    \\    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $12
    \\    FOR UPDATE OF l, a
    \\), calc AS (
    \\    SELECT *,
    \\           (d_ms * $7::bigint) / $14::bigint    AS run_fee,
    \\           (d_in * $8::bigint) / $15::bigint
    \\             + (d_cached * $9::bigint) / $15::bigint
    \\             + (d_out * $10::bigint) / $15::bigint AS token_cost
    \\    FROM probe
    \\), guard AS (
    \\    SELECT *, run_fee + token_cost AS slice,
    \\           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged,
    \\           meter_slice_seq + 1 AS next_seq
    \\    FROM calc
    \\    WHERE fencing_token >= fencing_seq
    \\), claim AS (
    \\    UPDATE fleet.runner_leases l
    \\    SET status = $13, metered_input_tokens = $4, metered_cached_tokens = $5,
    \\        metered_output_tokens = $6, last_metered_at_ms = $3, updated_at = $3
    \\    FROM guard g WHERE l.id = g.id
    \\    RETURNING g.id
    \\), ext_aff AS (
    \\    UPDATE fleet.runner_affinity a
    \\    SET metered_input_tokens = $4, metered_cached_tokens = $5,
    \\        metered_output_tokens = $6, last_metered_at_ms = $3, updated_at = $3,
    \\        meter_slice_seq = g.next_seq
    \\    FROM guard g WHERE a.zombie_id = g.zombie_id
    \\    RETURNING a.zombie_id
    \\), wallet AS (
    \\    UPDATE billing.tenant_billing tb
    \\    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
    \\        balance_exhausted_at = CASE
    \\            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $3)
    \\            ELSE NULL END,
    \\        updated_at = $3
    \\    FROM guard g WHERE tb.tenant_id = g.tenant_id
    \\    RETURNING tb.tenant_id
    \\), ledger AS (
    \\    INSERT INTO core.zombie_execution_telemetry
    \\      (id, tenant_id, workspace_id, zombie_id, event_id, charge_type, posture,
    \\       model, credit_deducted_nanos, token_count_input, token_count_output,
    \\       wall_ms, recorded_at)
    \\    SELECT 'mtr_' || g.event_id, g.tenant_id, g.workspace_id::text,
    \\           g.zombie_id::text, g.event_id, $11, g.posture, g.model,
    \\           g.charged, g.d_in, g.d_out, g.d_ms, $3
    \\    FROM guard g
    \\    ON CONFLICT (event_id, charge_type) DO UPDATE SET
    \\        credit_deducted_nanos = core.zombie_execution_telemetry.credit_deducted_nanos
    \\            + EXCLUDED.credit_deducted_nanos,
    \\        token_count_input  = COALESCE(core.zombie_execution_telemetry.token_count_input, 0)
    \\            + EXCLUDED.token_count_input,
    \\        token_count_output = COALESCE(core.zombie_execution_telemetry.token_count_output, 0)
    \\            + EXCLUDED.token_count_output,
    \\        wall_ms = COALESCE(core.zombie_execution_telemetry.wall_ms, 0) + EXCLUDED.wall_ms
    \\    RETURNING event_id
    \\), breakdown AS (
    \\    INSERT INTO fleet.metering_periods
    \\      (event_id, slice_seq, d_input_tokens, d_cached_tokens, d_output_tokens,
    \\       run_ms, run_fee_nanos, token_cost_nanos, charged_nanos, created_at)
    \\    SELECT g.event_id, g.next_seq,
    \\           g.d_in, g.d_cached, g.d_out, g.d_ms, g.run_fee, g.token_cost, g.charged, $3
    \\    FROM guard g
    \\    RETURNING event_id
    \\)
    \\SELECT (SELECT charged FROM guard)          AS charged,
    \\       (SELECT count(*) FROM claim)::bigint AS claimed
;

/// Claim the terminal report (fenced active→reported) AND settle the final
/// partial slice in one atomic statement. Returns whether the claim won + the
/// nanos charged. Errors propagate so the caller answers 500 (the report is
/// retryable; on retry an uncommitted attempt re-claims a still-`active` lease).
/// Runs on a caller-supplied pooled connection.
pub fn claimAndSettle(
    conn: *pg.Conn,
    lease_id: []const u8,
    runner_id: []const u8,
    now_ms: i64,
    meter: renewal.MeterInputs,
) !SettleOutcome {
    var q = PgQuery.from(try conn.query(CLAIM_SETTLE_SQL, .{
        lease_id,
        runner_id,
        now_ms,
        meter.cumulative_input,
        meter.cumulative_cached,
        meter.cumulative_output,
        meter.run_nanos_per_sec,
        meter.input_nanos_per_mtok,
        meter.cached_input_nanos_per_mtok,
        meter.output_nanos_per_mtok,
        telemetry.ChargeType.stage.label(),
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        MS_PER_SECOND,
        TOKENS_PER_MTOK,
    }));
    defer q.deinit();
    const row = try q.next() orelse return .{ .claimed = false, .charged_nanos = 0 };
    return .{
        .charged_nanos = (try row.get(?i64, 0)) orelse 0,
        .claimed = (try row.get(i64, 1)) == 1,
    };
}
