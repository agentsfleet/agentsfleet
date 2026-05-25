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
//! `fencing_token` is recorded on the lease but NOT verified — fencing
//! verification and true `INSERT … ON CONFLICT` idempotency are a follow-up;
//! the single-zombie skeleton has no reclaim path to fence against.
//!
//! Allocator: per-request arena (`hx.alloc`); see service.zig's module note.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const protocol = @import("protocol.zig");

const event_loop_types = @import("../zombie/event_loop_types.zig");
const event_loop_helpers = @import("../zombie/event_loop_helpers.zig");
const rows = @import("../zombie/event_loop_writepath_rows.zig");
const metering = @import("../zombie/metering.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const executor_client = @import("../executor/client.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_report);

const ZombieSession = event_loop_types.ZombieSession;
const StageResult = executor_client.ExecutorClient.StageResult;

/// The lease-row fields the report needs to reproduce finalize. All arena-dup'd.
const Lease = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    event_id: []const u8,
    posture: []const u8,
    model: []const u8,
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
        hx.fail(ec.ERR_INVALID_REQUEST, "Unknown or foreign lease");
        return;
    };

    finalize(hx, lease, body);
    hx.ok(.ok, protocol.ReportResponse{ .ok = true });
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
        \\       event_id, posture, model
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
    };
}

/// The three finalize writes + XACK, then mark the lease reported. Each step is
/// best-effort and logged on failure — mirroring `finalize`, which never
/// unwinds a partial write (non-atomic by design).
fn finalize(hx: Hx, lease: Lease, body: protocol.ReportRequest) void {
    const pool = hx.ctx.pool;
    const alloc = hx.alloc;
    const wall_ms = body.telemetry.wall_ms;

    const stage = StageResult{
        .content = body.response_text,
        .token_count = body.tokens,
        .wall_seconds = wall_ms / std.time.ms_per_s,
        .exit_ok = body.outcome == .processed,
        .failure = null,
        .time_to_first_token_ms = body.telemetry.time_to_first_token_ms,
    };

    var session = partialSession(alloc, lease, body.checkpoint);
    var event = partialEvent(lease.event_id);

    rows.markTerminal(pool, &session, &event, &stage, wall_ms);
    metering.recordStageActuals(pool, alloc, lease.tenant_id, .{
        .workspace_id = lease.workspace_id,
        .zombie_id = lease.zombie_id,
        .event_id = lease.event_id,
        .posture = parsePosture(lease.posture),
        .model = lease.model,
    }, 0, body.tokens, wall_ms, std.time.milliTimestamp() - @as(i64, @intCast(wall_ms)));
    rows.checkpointZombieSession(alloc, pool, &session) catch |err| {
        log.warn("report_checkpoint_failed", .{ .zombie_id = lease.zombie_id, .err = @errorName(err) });
    };
    redis_zombie.xackZombie(hx.ctx.queue, lease.zombie_id, lease.event_id) catch |err| {
        log.warn("report_xack_failed", .{ .zombie_id = lease.zombie_id, .event_id = lease.event_id, .err = @errorName(err) });
    };
    markLeaseReported(hx, body.lease_id);
    log.info("report_finalized", .{ .zombie_id = lease.zombie_id, .event_id = lease.event_id, .lease_id = body.lease_id });
}

/// Partial session for the leaf writers: `markTerminal` reads only `zombie_id`;
/// `checkpointZombieSession` reads `zombie_id` + `context_json`. Every other
/// field is unread, so `config` is left `undefined` — safe because no reached
/// code touches it — and the struct is NEVER deinit'd (its slices are
/// arena-scoped or borrowed, and `config.deinit()` on `undefined` would fault).
fn partialSession(alloc: std.mem.Allocator, lease: Lease, checkpoint: protocol.ReportCheckpoint) ZombieSession {
    return ZombieSession{
        .zombie_id = lease.zombie_id,
        .workspace_id = lease.workspace_id,
        // SAFETY: never read — markTerminal/checkpointZombieSession touch only
        // zombie_id + context_json, and this struct is never deinit'd.
        .config = undefined,
        .instructions = "",
        .context_json = buildContextJson(alloc, checkpoint),
        .source_markdown = "",
        .execution_id = null,
        .execution_started_at = 0,
    };
}

/// Partial event for `markTerminal`, which reads only `event_id`. The other
/// fields are unread fillers and the struct is NEVER deinit'd (deinit would
/// free the `@constCast` literals and fault).
fn partialEvent(event_id: []const u8) redis_zombie.ZombieEvent {
    const empty: []u8 = @constCast("");
    return .{
        .event_id = @constCast(event_id),
        .actor = empty,
        .event_type = empty,
        .workspace_id = empty,
        .request_json = empty,
        .created_at_ms = 0,
    };
}

/// Reproduce the `context_json` `updateSessionContext` would have written:
/// `{last_event_id, last_response}` with the response truncated identically, so
/// the checkpoint row equals the direct path's.
fn buildContextJson(alloc: std.mem.Allocator, checkpoint: protocol.ReportCheckpoint) []const u8 {
    const ContextUpdate = struct { last_event_id: []const u8, last_response: []const u8 };
    return std.json.Stringify.valueAlloc(alloc, ContextUpdate{
        .last_event_id = checkpoint.last_event_id,
        .last_response = event_loop_helpers.truncateForJson(checkpoint.last_response),
    }, .{}) catch "{}";
}

/// Map the stored posture label back to `Mode` for the telemetry span. Keyed on
/// the enum's own `label()` (RULE UFS — no literal); unknown → platform.
fn parsePosture(label: []const u8) tenant_provider.Mode {
    if (std.mem.eql(u8, label, tenant_provider.Mode.self_managed.label())) return .self_managed;
    return .platform;
}

fn markLeaseReported(hx: Hx, lease_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE fleet.runner_leases SET status = $2, updated_at = $3 WHERE id = $1::uuid
    , .{ lease_id, protocol.RUNNER_LEASE_STATUS_REPORTED, now_ms }) catch |err| {
        log.warn("report_lease_status_failed", .{ .lease_id = lease_id, .err = @errorName(err) });
    };
}
