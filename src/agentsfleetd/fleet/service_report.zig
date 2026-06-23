//! agentsfleetd-side runner control-plane orchestration — the `report` verb.
//!
//! Mirror of `event_loop_writepath.finalize` for the happy path: `markTerminal`
//! + the final metering settle + `checkpointFleetSession` (independent
//! autocommit statements, non-atomic) then `XACK`. The continuation/SSE-publish
//! steps of `finalize` are intentionally NOT reproduced: continuation is a no-op
//! on the happy path (`exit_ok`), and the activity publish writes no durable
//! row, so the durable row set still equals the direct path's.
//!
//! Billing is metered incrementally: the run fee + per-token delta is charged on
//! each `/renew` and the FINAL partial slice is settled ATOMICALLY with the
//! report claim (`claimReportAndSettle` → `renewal_settle.claimAndSettle`), so
//! the drained credit equals the real run and no final slice is lost to a
//! report→reclaim race. `fencing_token` is VERIFIED against the fleet's live
//! fencing sequence: a report whose lease was superseded by a reclaim (token <
//! current) is rejected UZ-RUN-005 and writes nothing — the current holder wins.
//! On success the fleet's affinity claim is released so its next event becomes
//! leasable.
//!
//! Allocator: per-request arena (`hx.alloc`); see service.zig's module note.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const common = @import("../http/handlers/common.zig");
const ec = @import("../errors/error_registry.zig");
const contract_mod = @import("contract");
const protocol = contract_mod.protocol;
const affinity = @import("affinity.zig");

const event_rows = @import("event_rows.zig");
const metering = @import("../fleet_runtime/metering.zig");
const renewal = @import("renewal.zig");
const renewal_settle = @import("renewal_settle.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const activity_publisher = @import("../fleet_runtime/activity_publisher.zig");
const metrics_runner = @import("../observability/metrics_runner.zig");
const otel_metrics = @import("../observability/otel_metrics.zig");
const runner_events = @import("runner_events.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_report);

const ExecutionResult = contract_mod.execution_result.ExecutionResult;

/// The lease-row fields the report needs to reproduce finalize. All arena-dup'd.
const Lease = struct {
    fleet_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    event_id: []const u8,
    posture: []const u8,
    provider: []const u8,
    model: []const u8,
    fencing_token: u64,
};

/// POST /v1/runners/me/reports — finalize one terminal execution the runner
/// reports. Reproduces the direct worker's finalize writes then XACKs.
pub fn report(hx: Hx, req: *httpz.Request) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(protocol.ReportRequest, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed report body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    const lease = loadLease(hx, runner_id, body.lease_id) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, "No active lease matches this lease_id for the runner");
        return;
    };

    const settled = claimReportAndSettle(hx, runner_id, lease, body) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (!settled.claimed) {
        log.debug("report_fenced", .{ .fleet_id = lease.fleet_id, .lease_id = body.lease_id, .fencing_token = lease.fencing_token, .runner_id = runner_id });
        hx.fail(ec.ERR_RUN_STALE_FENCING_TOKEN, "Lease superseded by a newer holder; report rejected");
        return;
    }
    log.debug("report_settled", .{ .fleet_id = lease.fleet_id, .event_id = lease.event_id, .charged_nanos = settled.charged_nanos });

    // Post-commit, fire-and-forget OTLP metrics for the settled run: stage
    // credit drained (final slice) + cumulative token throughput by direction +
    // run-latency. The claim+settle committed atomically above and the claim
    // won, so this records once per terminal run and never blocks the report.
    otel_metrics.recordRunSettlement(
        settled.charged_nanos,
        // input/cached/output are u32 → always fit i64. wall_ms is a runner-
        // controlled u64: saturate, never @intCast (which traps in ReleaseSafe
        // on > i64::MAX and would abort the whole daemon — Invariant 1).
        @intCast(body.input_tokens),
        @intCast(body.cached_input_tokens),
        @intCast(body.output_tokens),
        std.math.cast(i64, body.telemetry.wall_ms) orelse std.math.maxInt(i64),
        parsePosture(lease.posture).label(),
        lease.model,
        lease.workspace_id,
    );

    finalize(hx, runner_id, lease, body);
    // Per-runner telemetry (best-effort, in-memory — never gates the report).
    // The lease is now released, so drop the active-leases gauge; bucket the run
    // by outcome and stamp liveness; on failure, also bucket the granular reason.
    metrics_runner.observeRunnerExecution(runner_id, body.outcome);
    metrics_runner.decRunnerActiveLeases(runner_id);
    if (body.outcome == .fleet_error) metrics_runner.incRunnerFailure(runner_id, body.failure_reason);
    hx.ok(.ok, protocol.ReportResponse{ .ok = true });
}

/// Atomically CLAIM the report (flip the lease active→reported, fenced) AND
/// settle the final partial slice in ONE statement (`renewal_settle`), so the
/// fence ownership that authorizes reporting authorizes settlement — a concurrent
/// reclaim cannot bump the sequence between the claim and the settle, and the
/// cap-path final slice is never lost. A reclaim that already bumped the sequence
/// (or a lease no longer `active`) yields `claimed = false` (fenced, UZ-RUN-005),
/// charging nothing. The slice rates resolve at one `now_ms` shared with the
/// settle math. Errors propagate so the caller answers 500 (the report is
/// retryable; an uncommitted attempt leaves the lease `active` to re-claim).
fn claimReportAndSettle(hx: Hx, runner_id: []const u8, lease: Lease, body: protocol.ReportRequest) !renewal_settle.SettleOutcome {
    const now_ms = clock.nowMillis();
    const meter = renewal.buildMeterInputs(
        lease.provider,
        parsePosture(lease.posture),
        lease.model,
        now_ms,
        body.input_tokens,
        body.cached_input_tokens,
        body.output_tokens,
    );
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    return renewal_settle.claimAndSettle(conn, body.lease_id, runner_id, now_ms, meter);
}

/// Load the lease scoped to the presenting runner. A foreign or stale
/// `lease_id` yields null → the caller answers 400; the runner-id scope is the
/// ownership check (a runner can only report its own lease).
fn loadLease(hx: Hx, runner_id: []const u8, lease_id: []const u8) ?Lease {
    return loadLeaseInner(hx, runner_id, lease_id) catch |err| {
        log.warn("report_lease_load_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .lease_id = lease_id, .err = @errorName(err) });
        return null;
    };
}

fn loadLeaseInner(hx: Hx, runner_id: []const u8, lease_id: []const u8) !?Lease {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT fleet_id::text, workspace_id::text, tenant_id::text,
        \\       event_id, posture, provider, model, fencing_token
        \\FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid
    , .{ lease_id, runner_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // Dup every column before q.deinit() invalidates the row-backed slices.
    return .{
        .fleet_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0)),
        .workspace_id = try hx.alloc.dupe(u8, try row.get([]const u8, 1)),
        .tenant_id = try hx.alloc.dupe(u8, try row.get([]const u8, 2)),
        .event_id = try hx.alloc.dupe(u8, try row.get([]const u8, 3)),
        .posture = try hx.alloc.dupe(u8, try row.get([]const u8, 4)),
        .provider = try hx.alloc.dupe(u8, try row.get([]const u8, 5)),
        .model = try hx.alloc.dupe(u8, try row.get([]const u8, 6)),
        .fencing_token = @intCast(try row.get(i64, 7)),
    };
}

/// The terminal write + telemetry + checkpoint + XACK, then mark the lease
/// reported. Each step is best-effort and logged on failure (non-atomic by
/// design, matching the deleted direct path's finalize). The narrowed
/// `report_rows` writers take the few fields they read, so no partial-struct
/// shims are needed.
fn finalize(hx: Hx, runner_id: []const u8, lease: Lease, body: protocol.ReportRequest) void {
    const pool = hx.ctx.pool;
    const alloc = hx.alloc;
    const wall_ms = body.telemetry.wall_ms;

    const result = ExecutionResult{
        .content = body.response_text,
        .token_count = body.tokens,
        .wall_seconds = wall_ms / std.time.ms_per_s,
        .exit_ok = body.outcome == .processed,
        // Trust-boundary invariant: failure_label is null iff the run is
        // processed — never persist a reason a misbehaving runner pairs with
        // a clean outcome.
        .failure = if (body.outcome == .processed) null else body.failure_reason,
    };

    event_rows.markTerminal(pool, lease.fleet_id, lease.event_id, result, wall_ms);
    // Close the SSE activity bracket the deleted worker published on completion —
    // the dashboard + `agentsfleet steer` consume `event_complete` to end the live
    // tail. Best-effort (the publisher swallows failures).
    const status_text: []const u8 = if (result.exit_ok) event_rows.STATUS_PROCESSED else event_rows.STATUS_FLEET_ERROR;
    var scratch = activity_publisher.Scratch.init(alloc);
    defer scratch.deinit();
    activity_publisher.publishEventComplete(hx.ctx.queue, &scratch, lease.fleet_id, lease.event_id, status_text);
    // Emit the delivery span. The final slice was already settled atomically with
    // the report claim (`claimReportAndSettle`, before finalize), so by here the
    // billing is closed and only the OTel span remains. Best-effort.
    metering.emitDeliverySpan(lease.tenant_id, .{
        .workspace_id = lease.workspace_id,
        .fleet_id = lease.fleet_id,
        .event_id = lease.event_id,
        .posture = parsePosture(lease.posture),
        .provider = lease.provider,
        .model = lease.model,
    }, 0, body.tokens, wall_ms, clock.nowMillis() - (std.math.cast(i64, wall_ms) orelse std.math.maxInt(i64)));
    event_rows.checkpointFleetSession(alloc, pool, lease.fleet_id, buildContextJson(alloc, body.checkpoint)) catch |err| {
        log.warn("report_checkpoint_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = lease.fleet_id, .err = @errorName(err) });
    };
    redis_fleet.xackFleet(hx.ctx.queue, lease.fleet_id, lease.event_id) catch |err| {
        log.warn("report_xack_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = lease.fleet_id, .event_id = lease.event_id, .err = @errorName(err) });
    };
    releaseAffinity(hx, lease.fleet_id, lease.fencing_token);
    runner_events.appendLeaseReleasedBestEffort(hx.ctx.pool, hx.alloc, runner_id, body.lease_id, lease.fleet_id, lease.event_id);
    log.debug("report_finalized", .{ .fleet_id = lease.fleet_id, .event_id = lease.event_id, .lease_id = body.lease_id });
}

/// Reproduce the `context_json` the direct path wrote: `{last_event_id,
/// last_response}` with the response truncated identically, so the checkpoint
/// row equals the direct path's.
fn buildContextJson(alloc: std.mem.Allocator, checkpoint: protocol.ReportCheckpoint) []const u8 {
    const ContextUpdate = struct { last_event_id: []const u8, last_response: []const u8 };
    return std.json.Stringify.valueAlloc(alloc, ContextUpdate{
        .last_event_id = checkpoint.last_event_id,
        .last_response = event_rows.truncateForJson(checkpoint.last_response),
    }, .{}) catch "{}";
}

/// Map the stored posture label back to `Mode` for the telemetry span. Keyed on
/// the enum's own `label()` (RULE UFS — no literal); unknown → platform.
fn parsePosture(label: []const u8) tenant_provider.Mode {
    if (std.mem.eql(u8, label, tenant_provider.Mode.self_managed.label())) return .self_managed;
    return .platform;
}

/// Release the fleet's affinity claim so its next event becomes leasable. The
/// active→reported flip + final settle already happened atomically in
/// `claimReportAndSettle`; this only frees the slot, token-guarded so a
/// superseded holder can't free the current one. Best-effort — a DB blip must
/// not fail an already-finalized report.
fn releaseAffinity(hx: Hx, fleet_id: []const u8, token: u64) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    affinity.release(conn, fleet_id, token) catch |err| {
        log.warn("report_claim_release_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = fleet_id, .err = @errorName(err) });
    };
}
