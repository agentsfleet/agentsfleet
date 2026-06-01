//! zombied-side runner control-plane orchestration — the `report` verb.
//!
//! Faithful mirror of `event_loop_writepath.finalize` for the happy path:
//! `markTerminal` + `recordStageActuals` + `checkpointZombieSession` (three
//! independent autocommit statements, non-atomic) then `XACK` — the exact set
//! of writes, in the same order, with the same non-atomicity as the direct
//! worker's finalize. The continuation/SSE-publish steps of `finalize` are
//! intentionally NOT reproduced: continuation is a no-op on the happy path
//! (`exit_ok`), and the activity publish writes no durable row, so the row set
//! still equals the direct path's.
//!
//! The debit was already taken at lease (estimate, never re-charged here).
//! `fencing_token` is VERIFIED against the zombie's live fencing sequence: a
//! report whose lease was superseded by a reclaim (token < current) is rejected
//! UZ-RUN-005 and writes nothing — the current holder wins. On success the
//! zombie's affinity claim is released so its next event becomes leasable.
//!
//! Allocator: per-request arena (`hx.alloc`); see service.zig's module note.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const common = @import("../http/handlers/common.zig");
const ec = @import("../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const affinity = @import("affinity.zig");

const event_rows = @import("event_rows.zig");
const metering = @import("../zombie/metering.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const activity_publisher = @import("../zombie/activity_publisher.zig");
const metrics_runner = @import("../observability/metrics_runner.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_report);

const ExecutionResult = @import("contract").execution_result.ExecutionResult;

/// The lease-row fields the report needs to reproduce finalize. All arena-dup'd.
const Lease = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    event_id: []const u8,
    posture: []const u8,
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

    const won = claimReport(hx, body.lease_id, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (!won) {
        log.info("report_fenced", .{ .zombie_id = lease.zombie_id, .lease_id = body.lease_id, .fencing_token = lease.fencing_token, .runner_id = runner_id });
        hx.fail(ec.ERR_RUN_STALE_FENCING_TOKEN, "Lease superseded by a newer holder; report rejected");
        return;
    }

    finalize(hx, lease, body);
    // Per-runner failure telemetry: a failed run is bucketed by its granular
    // reason (absent → unknown). Best-effort, in-memory — never gates the report.
    if (body.outcome == .agent_error) metrics_runner.incRunnerFailure(runner_id, body.failure_reason);
    hx.ok(.ok, protocol.ReportResponse{ .ok = true });
}

/// Atomically claim the report: flip the lease active→reported in ONE statement
/// guarded by the live fence (`fencing_token >= fencing_seq`). A reclaim that
/// bumped the sequence makes the guard fail → 0 rows → `false` (fenced,
/// UZ-RUN-005); the row also won't flip if it is no longer `active` (already
/// reported/expired). The check and the state change cannot be split by a
/// concurrent reclaim — that is the fence. Errors propagate so the caller
/// answers 500 (the report is retryable).
fn claimReport(hx: Hx, lease_id: []const u8, runner_id: []const u8) !bool {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    var q = PgQuery.from(try conn.query(
        \\UPDATE fleet.runner_leases AS l
        \\SET status = $3, updated_at = $4
        \\FROM fleet.runner_affinity AS a
        \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $5
        \\  AND a.zombie_id = l.zombie_id AND l.fencing_token >= a.fencing_seq
        \\RETURNING l.id
    , .{ lease_id, runner_id, protocol.RUNNER_LEASE_STATUS_REPORTED, now_ms, protocol.RUNNER_LEASE_STATUS_ACTIVE }));
    defer q.deinit();
    return (try q.next()) != null;
}

/// Load the lease scoped to the presenting runner. A foreign or stale
/// `lease_id` yields null → the caller answers 400; the runner-id scope is the
/// ownership check (a runner can only report its own lease).
fn loadLease(hx: Hx, runner_id: []const u8, lease_id: []const u8) ?Lease {
    return loadLeaseInner(hx, runner_id, lease_id) catch |err| {
        log.warn("report_lease_load_failed", .{ .lease_id = lease_id, .err = @errorName(err) });
        return null;
    };
}

fn loadLeaseInner(hx: Hx, runner_id: []const u8, lease_id: []const u8) !?Lease {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT zombie_id::text, workspace_id::text, tenant_id::text,
        \\       event_id, posture, model, fencing_token
        \\FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid
    , .{ lease_id, runner_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    // Dup every column before q.deinit() invalidates the row-backed slices.
    return Lease{
        .zombie_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0)),
        .workspace_id = try hx.alloc.dupe(u8, try row.get([]const u8, 1)),
        .tenant_id = try hx.alloc.dupe(u8, try row.get([]const u8, 2)),
        .event_id = try hx.alloc.dupe(u8, try row.get([]const u8, 3)),
        .posture = try hx.alloc.dupe(u8, try row.get([]const u8, 4)),
        .model = try hx.alloc.dupe(u8, try row.get([]const u8, 5)),
        .fencing_token = @intCast(try row.get(i64, 6)),
    };
}

/// The terminal write + telemetry + checkpoint + XACK, then mark the lease
/// reported. Each step is best-effort and logged on failure (non-atomic by
/// design, matching the deleted direct path's finalize). The narrowed
/// `report_rows` writers take the few fields they read, so no partial-struct
/// shims are needed.
fn finalize(hx: Hx, lease: Lease, body: protocol.ReportRequest) void {
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

    event_rows.markTerminal(pool, lease.zombie_id, lease.event_id, result, wall_ms);
    // Close the SSE activity bracket the deleted worker published on completion —
    // the dashboard + `zombiectl steer` consume `event_complete` to end the live
    // tail. Best-effort (the publisher swallows failures).
    const status_text: []const u8 = if (result.exit_ok) event_rows.STATUS_PROCESSED else event_rows.STATUS_AGENT_ERROR;
    var scratch = activity_publisher.Scratch.init(alloc);
    defer scratch.deinit();
    activity_publisher.publishEventComplete(hx.ctx.queue, &scratch, lease.zombie_id, lease.event_id, status_text);
    metering.recordStageActuals(pool, alloc, lease.tenant_id, .{
        .workspace_id = lease.workspace_id,
        .zombie_id = lease.zombie_id,
        .event_id = lease.event_id,
        .posture = parsePosture(lease.posture),
        .model = lease.model,
    }, 0, body.tokens, wall_ms, std.time.milliTimestamp() - @as(i64, @intCast(wall_ms)));
    event_rows.checkpointZombieSession(alloc, pool, lease.zombie_id, buildContextJson(alloc, body.checkpoint)) catch |err| {
        log.warn("report_checkpoint_failed", .{ .zombie_id = lease.zombie_id, .err = @errorName(err) });
    };
    redis_zombie.xackZombie(hx.ctx.queue, lease.zombie_id, lease.event_id) catch |err| {
        log.warn("report_xack_failed", .{ .zombie_id = lease.zombie_id, .event_id = lease.event_id, .err = @errorName(err) });
    };
    releaseAffinity(hx, lease.zombie_id, lease.fencing_token);
    log.info("report_finalized", .{ .zombie_id = lease.zombie_id, .event_id = lease.event_id, .lease_id = body.lease_id });
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

/// Release the zombie's affinity claim so its next event becomes leasable. The
/// active→reported flip already happened atomically in `claimReport`; this only
/// frees the slot, token-guarded so a superseded holder can't free the current
/// one. Best-effort — a DB blip here must not fail an already-finalized report.
fn releaseAffinity(hx: Hx, zombie_id: []const u8, token: u64) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    affinity.release(conn, zombie_id, token) catch |err| {
        log.warn("report_claim_release_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
    };
}
