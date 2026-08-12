//! fleet — the `report` settle: atomically CLAIM the terminal report (flip the
//! lease active→reported, fenced) AND meter the FINAL partial slice in ONE
//! writable-CTE statement.
//!
//! The claim and the settle share one snapshot and one row lock. The probe reads
//! lease+slot under `FOR UPDATE OF l, a` (the `bal` arm locks the balance row),
//! the `guard` arm requires the
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
//! rates as `/renew` (shared `renewal_meter.buildMeterInputs`), so a run that finished
//! inside one renewal window (never renewed) is still charged its real runtime
//! and gets its telemetry + breakdown rows. Advances BOTH cursors so a replay
//! settles ≈0. `charged = LEAST(slice, balance)` clamps the audit rows to the
//! actual drain. Runs on a caller-supplied pooled connection (drained via
//! PgQuery).

const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");
const protocol = @import("contract").protocol;
const id_format = @import("../types/id_format.zig");
const telemetry = @import("../state/fleet_telemetry_store.zig");
const renewal_meter = @import("renewal_meter.zig");

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
// (only while `status = active`) and the affinity cursor under
// `FOR UPDATE OF l, a` — the affinity lock serialises a racing reclaim behind
// this statement. `bal` locks the tenant's billing row in its own CTE (the
// renew statement's shape: Postgres refuses `FOR UPDATE` on the nullable side
// of an outer join, and the row must stay optional), serialising same-tenant
// money ops so `bal0` is the LIVE balance after any lock wait. `calc`/`guard`
// price the slice and compute `charged = LEAST(slice, bal0)` — equal to the
// wallet's actual delta in every interleaving, so an exhaustion overlap
// records audit rows summing to the real drain, never more (exactly as the
// renew CTE does); `guard` survives only if the fence holds. `claim` flips
// active→reported AND advances the lease cursor (clamped `GREATEST(old, $n)`
// so a regressed report never rewinds it); `ext_aff` advances the slot cursor
// the same way; `wallet` and `ledger` are the guard-gated money writes;
// `tally` bumps the runner's lifetime succeeded/failed counter ($17 picks the
// column), gated `FROM claim` so a fenced retry that claims nothing also counts
// nothing. The trailing SELECT returns the charged nanos + whether the claim
// flipped a row (the report-won signal).
//
// `event_created_at` rides the probe select so the ledger arm can stamp it: it
// is the EVENT's instant, shared by every row for that event, not this settle's.
const CLAIM_SETTLE_SQL =
    \\WITH probe AS (
    \\    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
    \\           l.event_created_at,
    \\           l.posture, l.model, l.fencing_token, a.fencing_seq,
    \\           GREATEST(0, $3::bigint - a.last_metered_at)         AS d_ms,
    \\           GREATEST(0, $4::bigint - a.metered_input_tokens)    AS d_in,
    \\           GREATEST(0, $5::bigint - a.metered_cached_tokens)   AS d_cached,
    \\           GREATEST(0, $6::bigint - a.metered_output_tokens)   AS d_out
    \\    FROM fleet.runner_leases l
    \\    JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $12
    \\    FOR UPDATE OF l, a
    \\), bal AS (
    \\    SELECT tb.tenant_id, tb.balance_nanos AS bal0
    \\    FROM billing.tenant_wallet tb
    \\    JOIN probe p ON p.tenant_id = tb.tenant_id
    \\    FOR UPDATE OF tb
    \\), calc AS (
    \\    SELECT p.*, b.bal0,
    \\           (d_ms * $7::bigint) / $14::bigint    AS run_fee,
    \\           (d_in * $8::bigint) / $15::bigint
    \\             + (d_cached * $9::bigint) / $15::bigint
    \\             + (d_out * $10::bigint) / $15::bigint AS token_cost
    \\    FROM probe p
    \\    LEFT JOIN bal b ON b.tenant_id = p.tenant_id
    \\), guard AS (
    \\    SELECT *, run_fee + token_cost AS slice,
    \\           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged
    \\    FROM calc
    \\    WHERE fencing_token >= fencing_seq
    \\), claim AS (
    \\    UPDATE fleet.runner_leases l
    \\    SET status = $13,
    \\        metered_input_tokens = GREATEST(l.metered_input_tokens, $4),
    \\        metered_cached_tokens = GREATEST(l.metered_cached_tokens, $5),
    \\        metered_output_tokens = GREATEST(l.metered_output_tokens, $6),
    \\        last_metered_at = $3, updated_at = $3
    \\    FROM guard g WHERE l.id = g.id
    \\    RETURNING g.id
    \\), ext_aff AS (
    \\    UPDATE fleet.runner_affinity a
    \\    SET metered_input_tokens = GREATEST(a.metered_input_tokens, $4),
    \\        metered_cached_tokens = GREATEST(a.metered_cached_tokens, $5),
    \\        metered_output_tokens = GREATEST(a.metered_output_tokens, $6),
    \\        last_metered_at = $3, updated_at = $3
    \\    FROM guard g WHERE a.fleet_id = g.fleet_id
    \\    RETURNING a.fleet_id
    \\), wallet AS (
    \\    UPDATE billing.tenant_wallet tb
    \\    SET balance_nanos = GREATEST(0, tb.balance_nanos - g.slice),
    \\        balance_exhausted_at = CASE
    \\            WHEN tb.balance_nanos - g.slice <= 0 THEN COALESCE(tb.balance_exhausted_at, $3)
    \\            ELSE NULL END,
    \\        updated_at = $3
    \\    FROM guard g WHERE tb.tenant_id = g.tenant_id
    \\    RETURNING tb.tenant_id
    \\), ledger AS (
    \\    INSERT INTO billing.usage_ledger
    \\      (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture,
    \\       model, credit_deducted_nanos, token_count_input, token_count_cached_input,
    \\       token_count_output, wall_ms, event_created_at, created_at, last_charged_at)
    \\    SELECT $16::uuid, g.tenant_id, g.workspace_id, g.fleet_id, g.event_id, $11,
    \\           g.posture, g.model, g.charged, g.d_in, g.d_cached, g.d_out, g.d_ms,
    \\           g.event_created_at, $3, $3
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
    \\), tally AS (
    \\    INSERT INTO fleet.runner_lifetime_counters
    \\      (runner_id, succeeded, failed, created_at, updated_at)
    \\    SELECT $2::uuid,
    \\           CASE WHEN $17::boolean THEN 1 ELSE 0 END,
    \\           CASE WHEN $17::boolean THEN 0 ELSE 1 END,
    \\           $3, $3
    \\    FROM claim
    \\    ON CONFLICT (runner_id) DO UPDATE
    \\       SET succeeded = fleet.runner_lifetime_counters.succeeded + EXCLUDED.succeeded,
    \\           failed    = fleet.runner_lifetime_counters.failed + EXCLUDED.failed,
    \\           updated_at = EXCLUDED.updated_at
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
    meter: renewal_meter.MeterInputs,
    succeeded: bool,
) !SettleOutcome {
    const ledger_uid_value = try id_format.generateUuidV7();
    const ledger_uid: []const u8 = &ledger_uid_value;

    // Elevate to `metering_runtime` for the one fenced statement (schema/120).
    // The statement is unchanged; the result drains before the callback
    // returns and the commit runs (see `renewal.renew`).
    const Ctx = struct {
        lease_id: []const u8,
        runner_id: []const u8,
        now_ms: i64,
        meter: renewal_meter.MeterInputs,
        ledger_uid: []const u8,
        succeeded: bool,
    };
    const outcome = try pool_elevation.withRole(conn, .metering, Ctx{
        .lease_id = lease_id,
        .runner_id = runner_id,
        .now_ms = now_ms,
        .meter = meter,
        .ledger_uid = ledger_uid,
        .succeeded = succeeded,
    }, struct {
        fn run(c: Ctx, v: pool_elevation.Elevated(.metering)) !?SettleOutcome {
            var q = PgQuery.from(try v.conn.query(CLAIM_SETTLE_SQL, .{
                c.lease_id,
                c.runner_id,
                c.now_ms,
                c.meter.cumulative_input,
                c.meter.cumulative_cached,
                c.meter.cumulative_output,
                c.meter.run_nanos_per_sec,
                c.meter.input_nanos_per_mtok,
                c.meter.cached_input_nanos_per_mtok,
                c.meter.output_nanos_per_mtok,
                telemetry.ChargeType.stage.label(),
                protocol.RUNNER_LEASE_STATUS_ACTIVE,
                protocol.RUNNER_LEASE_STATUS_REPORTED,
                MS_PER_SECOND,
                TOKENS_PER_MTOK,
                c.ledger_uid,
                c.succeeded,
            }));
            defer q.deinit();
            const row = try q.next() orelse return null;
            return .{
                .charged_nanos = (try row.get(?i64, 0)) orelse 0,
                .claimed = (try row.get(i64, 1)) == 1,
            };
        }
    }.run);
    return outcome orelse .{ .claimed = false, .charged_nanos = 0 };
}
