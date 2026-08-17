//! PATCH /v1/fleets/runners/{id} — platform-admin runner operator plane.

const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const id_format = @import("../../../types/id_format.zig");
const protocol = @import("contract").protocol;
const runner_events = @import("../../../fleet/runner_events.zig");
const policy_row = @import("../runner/assigned_policy_row.zig");
const reconcile = @import("../runner/heartbeat_reconcile.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.fleet_runner_patch);

const S_PATCH_BODY = "PATCH body must be exactly one of {\"action\":\"cordon|drain|revoke|self_test\"} or {\"assigned_policy\":{sandbox_tier, network_policy, registry_allowlist[], worker_count, extra_binds[]}}";
const S_RUNNER_NOT_FOUND = "Runner not found";
const S_REVOKED_IS_TERMINAL = "revoked runners cannot transition back to cordoned or draining";
const S_REVOKED_NO_POLICY = "revoked runners cannot be re-assigned a policy";
const S_EVENT_ID_MINT_FAILED = "runner event id generation failed";
const S_BAD_REGISTRY = "registry_allowlist entries must be host[:port] names";
const S_BAD_EXTRA_BINDS = "extra_binds entries must be absolute host paths outside the daemon-owned baseline and the sensitive set, with no traversal";
const S_POLICY_UPDATE_FAILED = "runner policy update failed";
const S_REVOKED_NO_SELFTEST = "revoked runners cannot be asked to self-test";

pub fn innerPatchFleetRunner(hx: Hx, req: *httpz.Request, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;
    const body = parseBody(hx, req) orelse return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const current = loadState(conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND);
        return;
    };
    if (body.assigned_policy) |requested| {
        applyPolicyAssignment(hx, conn, runner_id, current, requested);
        return;
    }
    // `self_test` records an ask rather than transitioning state, so it forks
    // ahead of `stateForAction` — which has no target state to give it.
    if (body.action.? == .self_test) {
        applySelfTestRequest(hx, conn, runner_id, current);
        return;
    }
    applyAdminAction(hx, conn, runner_id, current, body.action.?);
}

/// Record an operator's self-test ask. The reply is the recorded REQUEST, never
/// a verdict: the daemon picks the ask up on its next heartbeat and reports the
/// result on a later one, so blocking here would hang the dashboard on exactly
/// the offline host an operator most wants to test.
fn applySelfTestRequest(hx: Hx, conn: *pg.Conn, runner_id: []const u8, current: protocol.AdminState) void {
    if (current == .revoked) {
        hx.fail(ec.ERR_RUN_SELFTEST_REFUSED, S_REVOKED_NO_SELFTEST);
        return;
    }
    const now_ms = clock.nowMillis();
    const recorded = requestSelfTest(conn, runner_id, now_ms) catch |err| {
        switch (err) {
            error.RunnerGone => hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND),
            error.RevokedRace => hx.fail(ec.ERR_RUN_SELFTEST_REFUSED, S_REVOKED_NO_SELFTEST),
            else => common.internalDbError(hx.res, hx.req_id),
        }
        return;
    };

    log.debug("runner_selftest_requested", .{ .runner_id = runner_id, .requested_at = recorded });
    hx.ok(.ok, protocol.RunnerAdminPatchResponse{
        .id = runner_id,
        .admin_state = current,
        .selftest_requested_at = recorded,
    });
}

/// Stamp the ask, returning the instant recorded. A null row means the guard
/// rejected the write, so the row is re-read to say WHICH guard: a runner that
/// vanished and one revoked mid-request are different answers to the operator.
fn requestSelfTest(conn: *pg.Conn, runner_id: []const u8, now_ms: i64) !i64 {
    var q = PgQuery.from(conn.query(sql.PATCH_RUNNER_SELFTEST_REQUEST, .{
        runner_id,
        now_ms,
        @tagName(protocol.AdminState.revoked),
    }) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row != null) return now_ms;
    const after = try loadState(conn, runner_id) orelse return error.RunnerGone;
    if (after == .revoked) return error.RevokedRace;
    return error.DbError;
}

fn applyAdminAction(hx: Hx, conn: *pg.Conn, runner_id: []const u8, current: protocol.AdminState, action: protocol.RunnerAdminAction) void {
    // `self_test` names no target state and is forked away by the caller. A 400
    // rather than `unreachable`: if a later edit reorders that fork, an operator
    // gets a refusal instead of a panicked worker thread.
    const target = stateForAction(action) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return;
    };
    if (current == .revoked and target != .revoked) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_REVOKED_IS_TERMINAL);
        return;
    }
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch {
        common.internalOperationError(hx.res, S_EVENT_ID_MINT_FAILED, hx.req_id);
        return;
    };
    defer hx.alloc.free(event_row_id);

    updateState(conn, runner_id, target, event_row_id) catch |err| {
        switch (err) {
            error.RunnerGone => hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND),
            error.RevokedRace => hx.fail(ec.ERR_INVALID_REQUEST, S_REVOKED_IS_TERMINAL),
            else => common.internalDbError(hx.res, hx.req_id),
        }
        return;
    };

    // No cache to invalidate: the committed `admin_state` IS the verdict every
    // machine reads on the runner's next request (`cmd/serve_runner_lookup.zig`),
    // so this transition takes effect fleet-wide the moment it commits.
    log.debug("runner_admin_state_changed", .{ .runner_id = runner_id, .admin_state = @tagName(target) });
    writeResponse(hx, runner_id, target);
}

/// Re-assign the runner's policy. The write is idempotent (a same-values PATCH
/// updates nothing and emits no event) and the new assignment reaches the host
/// on its next heartbeat — no host visit, no restart. The row's verdict is
/// re-reconciled against the stored capability report in the same request, so
/// the degraded flag (and the lease gate reading it) never lags the assignment.
fn applyPolicyAssignment(hx: Hx, conn: *pg.Conn, runner_id: []const u8, current: protocol.AdminState, requested: protocol.AssignedPolicy) void {
    if (current == .revoked) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_REVOKED_NO_POLICY);
        return;
    }
    if (!protocol.registryAllowlistValid(requested.registry_allowlist)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BAD_REGISTRY);
        return;
    }
    // The daemon re-validates this same list before `buildArgv`; neither side
    // trusts the other's check. Refusing here as well means an unsafe entry is
    // never STORED, so it cannot reach a host that skips its own validation —
    // and the operator learns at the dashboard rather than via a degraded
    // runner one heartbeat later.
    if (!protocol.extraBindsValid(requested.extra_binds)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BAD_EXTRA_BINDS);
        return;
    }
    var stored = requested;
    stored.worker_count = std.math.clamp(stored.worker_count, protocol.MIN_WORKER_COUNT, protocol.MAX_WORKER_COUNT);
    const registry_json = std.json.Stringify.valueAlloc(hx.alloc, stored.registry_allowlist, .{}) catch {
        // mudball-ok: OOM-only failure stringifying an already-validated payload; detail stays plain English
        common.internalOperationError(hx.res, S_POLICY_UPDATE_FAILED, hx.req_id);
        return;
    };
    const extra_binds_json = std.json.Stringify.valueAlloc(hx.alloc, stored.extra_binds, .{}) catch {
        // mudball-ok: OOM-only failure stringifying an already-validated payload; detail stays plain English
        common.internalOperationError(hx.res, S_POLICY_UPDATE_FAILED, hx.req_id);
        return;
    };
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch {
        // mudball-ok: id mint is OOM/entropy-only; same plain detail as the admin-action path
        common.internalOperationError(hx.res, S_EVENT_ID_MINT_FAILED, hx.req_id);
        return;
    };
    defer hx.alloc.free(event_row_id);

    // Reconcile the NEW assignment against the row's stored capability report
    // BEFORE the write, so the verdict rides the same statement. A read
    // failure yields a null report, which reconciles to degraded — the
    // fail-closed answer, never an assumed-healthy one.
    const verdict = reconcile.reconcile(stored, readCapability(hx, conn, runner_id));

    {
        var q = PgQuery.from(conn.query(sql.PATCH_RUNNER_ASSIGNED_POLICY, .{
            runner_id,
            @tagName(stored.sandbox_tier),
            @tagName(stored.network_policy),
            registry_json,
            @as(i32, @intCast(stored.worker_count)),
            clock.nowMillis(),
            event_row_id,
            @tagName(protocol.RunnerEventType.runner_policy_assigned),
            runner_events.META_SANDBOX_TIER,
            runner_events.META_NETWORK_POLICY,
            runner_events.META_REGISTRY_ALLOWLIST,
            runner_events.META_WORKER_COUNT,
            verdict.degraded,
            verdict.reason,
            extra_binds_json,
        }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        });
        defer q.deinit();
        // A null row is a no-op re-assignment (values already match) — success.
        _ = q.next() catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    }

    log.debug("runner_policy_assigned", .{ .runner_id = runner_id, .sandbox_tier = @tagName(stored.sandbox_tier), .network_policy = @tagName(stored.network_policy), .worker_count = stored.worker_count, .degraded = verdict.degraded });
    hx.ok(.ok, protocol.RunnerAdminPatchResponse{ .id = runner_id, .admin_state = current, .assigned_policy = stored });
}

fn readCapability(hx: Hx, conn: *pg.Conn, runner_id: []const u8) ?protocol.CapabilityReport {
    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_CAPABILITY, .{runner_id}) catch return null);
    defer q.deinit();
    const row = (q.next() catch return null) orelse return null;
    const raw = row.get(?[]const u8, 0) catch return null;
    return policy_row.decodeCapability(hx.alloc, raw);
}

fn parseBody(hx: Hx, req: *httpz.Request) ?protocol.RunnerAdminPatchRequest {
    const raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    };
    if (raw.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    }
    if (!common.checkBodySize(req, hx.res, raw, hx.req_id)) return null;
    // Leaky on the request arena: the assigned_policy variant carries slices
    // that must outlive this function; they die with the request.
    const parsed = std.json.parseFromSliceLeaky(protocol.RunnerAdminPatchRequest, hx.alloc, raw, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    };
    const exactly_one = (parsed.action != null) != (parsed.assigned_policy != null);
    if (!exactly_one) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_PATCH_BODY);
        return null;
    }
    return parsed;
}

/// The state each action moves the runner to, or null for an action that moves
/// none. Exhaustive by construction: a new action added to the enum fails to
/// compile here until someone decides which side of that line it falls on.
fn stateForAction(action: protocol.RunnerAdminAction) ?protocol.AdminState {
    return switch (action) {
        .cordon => .cordoned,
        .drain => .draining,
        .revoke => .revoked,
        .self_test => null,
    };
}

fn loadState(conn: *pg.Conn, runner_id: []const u8) !?protocol.AdminState {
    var q = PgQuery.from(conn.query(sql.SELECT_RUNNER_ADMIN_STATE, .{runner_id}) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row == null) return null;
    const raw = row.?.get([]u8, 0) catch return error.DbError;
    return std.meta.stringToEnum(protocol.AdminState, raw) orelse error.DbRowShape;
}

fn updateState(
    conn: *pg.Conn,
    runner_id: []const u8,
    target: protocol.AdminState,
    event_row_id: []const u8,
) !void {
    const now_ms = clock.nowMillis();
    const bypass_revoked_guard = target == .revoked;
    const event_type = runner_events.eventTypeForAdminState(target);
    var q = PgQuery.from(conn.query(sql.PATCH_RUNNER_ADMIN_STATE, .{
        runner_id,
        @tagName(target),
        now_ms,
        bypass_revoked_guard,
        @tagName(protocol.AdminState.revoked),
        event_row_id,
        @tagName(event_type),
        runner_events.META_FROM_ADMIN_STATE,
        runner_events.META_TO_ADMIN_STATE,
    }) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row != null) return;
    const after = try loadState(conn, runner_id) orelse return error.RunnerGone;
    if (after == target) return;
    if (after == .revoked and target != .revoked) return error.RevokedRace;
    return error.DbError;
}

fn writeResponse(hx: Hx, runner_id: []const u8, admin_state: protocol.AdminState) void {
    hx.ok(.ok, protocol.RunnerAdminPatchResponse{ .id = runner_id, .admin_state = admin_state });
}
