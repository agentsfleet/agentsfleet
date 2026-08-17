//! POST /v1/runners/me/heartbeats — runner liveness + policy delivery.
//!
//! Authed by `runnerBearer` (the principal carries `runner_id`). Every reply
//! carries the row's current assigned policy and degraded verdict, so an
//! operator's dashboard change reaches the host within one heartbeat and a
//! host visit is never needed. `drain`/`stop` statuses arrive with the
//! fleet-failover slice. Side effect: bump `fleet.runners.last_seen_at`
//! (liveness is written here, not on every authed call, per docs/AUTH.md).

const std = @import("std");
const constants = @import("common");
const sql = @import("sql.zig");
const clock = constants.clock;
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const metrics_runner = @import("../../../observability/metrics_runner.zig");
const id_format = @import("../../../types/id_format.zig");
const runner_events = @import("../../../fleet/runner_events.zig");
const policy_row = @import("assigned_policy_row.zig");
const reconcile = @import("heartbeat_reconcile.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_heartbeat);
const LOG_EVENT_HEARTBEAT_BUMP_FAILED = "heartbeat_bump_failed";
const LOG_EVENT_CAPABILITY_PERSIST_FAILED = "capability_persist_failed";
const LOG_EVENT_SELFTEST_PERSIST_FAILED = "selftest_persist_failed";
const LOG_EVENT_SELFTEST_REJECTED = "selftest_report_rejected";

pub fn innerRunnerHeartbeat(hx: Hx, req: *httpz.Request) void {
    const runner_id = hx.principal.runner_id orelse {
        // runnerBearer guarantees this is set; defensive only.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const incoming = switch (parseCapabilityReport(hx, req)) {
        .responded => return,
        .none => @as(?protocol.CapabilityReport, null),
        .report => |r| @as(?protocol.CapabilityReport, r),
    };
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // The policy read is load-bearing (the reply is the delivery channel), so
    // it fails loud — the runner retries and keeps its last-applied policy for
    // in-flight work. The verdict + liveness writes below stay best-effort.
    const row = readPolicyRow(hx.alloc, conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        // Token authenticated but the row is gone (revoked + reaped) — fail
        // closed rather than 200 a phantom runner.
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner not found");
        return;
    };

    // Reconcile assigned against achievable — this beat's report when one rode
    // in, else the stored one — and carry the fresh verdict back to the host.
    const effective_cap = incoming orelse policy_row.decodeCapability(hx.alloc, row.capability_json);
    const verdict = reconcile.reconcile(row.assigned, effective_cap);
    persistVerdict(hx, conn, runner_id, incoming, row, verdict);

    // A verdict rode in on this beat. Stored after the capability write so a
    // malformed one cannot cost the beat its reconciliation, and best-effort for
    // the same reason liveness is: a self-test that fails to persist is re-run
    // by the operator, while a failed heartbeat parks real work.
    const reported = persistSelftest(hx, conn, runner_id, req);

    bumpLastSeen(hx, conn, runner_id);
    metrics_runner.touchRunnerSeen(runner_id); // in-memory liveness for /metrics
    hx.ok(.ok, protocol.HeartbeatResponse{
        .status = .ok,
        .assigned_policy = row.assigned,
        .degraded = verdict.degraded,
        .degraded_reason = verdict.reason,
        // Suppressed on the beat that just reported one: the write above cleared
        // the request, so echoing it back would ask the host to immediately
        // re-run the probe it has this second finished.
        .selftest_requested = row.selftest_requested and !reported,
    });
}

/// Store a reported verdict, if this beat carried a well-formed one. Returns
/// whether anything was written, which is what suppresses the request echo.
///
/// Silent on a malformed verdict, exactly like the capability report: a runner
/// token must not be able to fail a liveness beat by sending nonsense.
fn persistSelftest(hx: Hx, conn: *pg.Conn, runner_id: []const u8, req: *httpz.Request) bool {
    const raw = req.body() orelse return false;
    if (raw.len == 0) return false;
    const parsed = std.json.parseFromSliceLeaky(protocol.HeartbeatRequest, hx.alloc, raw, .{ .ignore_unknown_fields = true }) catch return false;
    const report = parsed.selftest orelse return false;
    switch (protocol.selftestReportRejection(report)) {
        .none => {},
        .unbounded => {
            log.warn(LOG_EVENT_SELFTEST_REJECTED, .{ .error_code = ec.ERR_RUN_SELFTEST_REPORT_INVALID, .runner_id = runner_id, .err = "verdict exceeds its bounds" });
            return false;
        },
        .all_ok_disagrees => {
            log.warn(LOG_EVENT_SELFTEST_REJECTED, .{ .error_code = ec.ERR_RUN_SELFTEST_REPORT_INVALID, .runner_id = runner_id, .err = "all_ok disagrees with the reported checks" });
            return false;
        },
    }

    const checks_json = std.json.Stringify.valueAlloc(hx.alloc, report.checks, .{}) catch return false;
    const now_ms = clock.nowMillis();
    _ = conn.exec(sql.UPDATE_RUNNER_SELFTEST, .{
        runner_id,
        checks_json,
        report.all_ok,
        report.sandbox_tier,
        report.network_policy,
        now_ms,
    }) catch |err| {
        log.warn(LOG_EVENT_SELFTEST_PERSIST_FAILED, .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .runner_id = runner_id, .err = @errorName(err) });
        return false;
    };
    log.debug("runner_selftest_reported", .{ .runner_id = runner_id, .all_ok = report.all_ok, .checks = report.checks.len });
    return true;
}

/// What one heartbeat body carried. A malformed report must never fail a
/// liveness beat: unreadable reads as "no report this beat" and the stored
/// report keeps reconciling. Only the oversize path has already responded.
const ReportParse = union(enum) { none, report: protocol.CapabilityReport, responded };

fn parseCapabilityReport(hx: Hx, req: *httpz.Request) ReportParse {
    const raw = req.body() orelse return .none;
    if (raw.len == 0) return .none;
    if (!common.checkBodySize(req, hx.res, raw, hx.req_id)) return .responded;
    const parsed = std.json.parseFromSliceLeaky(protocol.HeartbeatRequest, hx.alloc, raw, .{ .ignore_unknown_fields = true }) catch return .none;
    const report = parsed.capability_report orelse return .none;
    // An out-of-bounds controllers list is a malformed report, not a
    // persistence-amplification channel — same lenient "no report this beat".
    if (!protocol.capabilityReportBounded(report)) return .none;
    return .{ .report = report };
}

/// The row as the reconciliation needs it. Slices are re-parsed or duped into
/// the request arena — nothing borrows the query result past this function.
const PolicyRow = struct {
    assigned: ?protocol.AssignedPolicy,
    capability_json: ?[]const u8,
    stored_degraded: bool,
    stored_reason: ?[]const u8,
    /// An operator's outstanding ask, as a decided boolean rather than the raw
    /// timestamp: the reply carries "is one pending", and nothing downstream has
    /// a use for when it was made.
    selftest_requested: bool,
};

fn readPolicyRow(alloc: std.mem.Allocator, conn: *pg.Conn, runner_id: []const u8) !?PolicyRow {
    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_ASSIGNED_POLICY, .{runner_id}) catch return error.DbError);
    defer q.deinit();
    const row = (q.next() catch return error.DbError) orelse return null;
    const assigned = policy_row.decodePolicy(
        alloc,
        row.get([]const u8, 0) catch return error.DbError,
        row.get(?[]const u8, 1) catch return error.DbError,
        row.get(?[]const u8, 2) catch return error.DbError,
        row.get(i32, 3) catch return error.DbError,
        row.get(?[]const u8, 8) catch return error.DbError,
    );
    const reason_raw = row.get(?[]const u8, 5) catch return error.DbError;
    const cap_raw = row.get(?[]const u8, 6) catch return error.DbError;
    return PolicyRow{
        .assigned = assigned,
        .capability_json = if (cap_raw) |c| try alloc.dupe(u8, c) else null,
        .stored_degraded = row.get(bool, 4) catch return error.DbError,
        .stored_reason = if (reason_raw) |r| try alloc.dupe(u8, r) else null,
        .selftest_requested = (row.get(?i64, 7) catch return error.DbError) != null,
    };
}

/// Write what this beat changed: a fresh report always lands with its verdict;
/// otherwise only a moved verdict writes (the statement's guard keeps a steady
/// state write-free). Best-effort — a failed write self-heals next beat.
fn persistVerdict(hx: Hx, conn: *pg.Conn, runner_id: []const u8, incoming: ?protocol.CapabilityReport, row: PolicyRow, verdict: reconcile.Verdict) void {
    const now_ms = clock.nowMillis();
    if (incoming) |report| {
        const report_json = std.json.Stringify.valueAlloc(hx.alloc, report, .{}) catch {
            log.warn(LOG_EVENT_CAPABILITY_PERSIST_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = "report allocation failed" });
            return;
        };
        _ = conn.exec(sql.UPDATE_RUNNER_CAPABILITY_AND_VERDICT, .{ runner_id, report_json, now_ms, verdict.degraded, verdict.reason }) catch |err| {
            log.warn(LOG_EVENT_CAPABILITY_PERSIST_FAILED, .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .runner_id = runner_id, .err = @errorName(err) });
        };
        log.debug("runner_capability_reported", .{ .runner_id = runner_id, .landlock = report.landlock, .seccomp = report.seccomp, .cgroup_controllers = report.cgroup_controllers.len, .bubblewrap = report.bubblewrap, .egress_enforcement = report.egress_enforcement });
    } else if (verdict.degraded != row.stored_degraded or !reasonEql(verdict.reason, row.stored_reason)) {
        _ = conn.exec(sql.UPDATE_RUNNER_VERDICT, .{ runner_id, verdict.degraded, verdict.reason, now_ms }) catch |err| {
            log.warn("verdict_persist_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .runner_id = runner_id, .err = @errorName(err) });
        };
    }
    if (verdict.degraded and !row.stored_degraded) {
        log.warn("runner_degraded", .{ .error_code = ec.ERR_EXEC_ASSIGNMENT_UNACHIEVABLE, .runner_id = runner_id, .reason = verdict.reason orelse "unspecified" });
    } else if (!verdict.degraded and row.stored_degraded) {
        log.debug("runner_degraded_cleared", .{ .runner_id = runner_id });
    }
}

fn reasonEql(a: ?[]const u8, b: ?[]const u8) bool {
    if (a == null and b == null) return true;
    if (a == null or b == null) return false;
    return std.mem.eql(u8, a.?, b.?);
}

/// Best-effort liveness bump — a DB blip must not fail the heartbeat reply.
fn bumpLastSeen(hx: Hx, conn: *pg.Conn, runner_id: []const u8) void {
    const now_ms = clock.nowMillis();
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch |err| {
        log.warn("heartbeat_online_event_id_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
        bumpOnly(conn, runner_id, now_ms);
        return;
    };
    defer hx.alloc.free(event_row_id);
    _ = conn.exec(sql.HEARTBEAT_WITH_TRANSITION_EVENT, .{
        runner_id,
        now_ms,
        event_row_id,
        @tagName(protocol.RunnerEventType.runner_online),
        runner_events.META_LAST_SEEN_AT,
        protocol.RUNNER_LAST_SEEN_NEVER,
        constants.RUNNER_OFFLINE_AFTER_MS,
    }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .runner_id = runner_id, .err = @errorName(err) });
        bumpOnly(conn, runner_id, now_ms);
    };
}

fn bumpOnly(conn: anytype, runner_id: []const u8, now_ms: i64) void {
    _ = conn.exec(sql.TOUCH_RUNNER_LAST_SEEN, .{ runner_id, now_ms }) catch |err| {
        log.warn(LOG_EVENT_HEARTBEAT_BUMP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .runner_id = runner_id, .err = @errorName(err) });
    };
}
