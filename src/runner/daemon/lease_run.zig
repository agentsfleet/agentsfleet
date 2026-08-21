//! lease_run.zig — the per-lease execution lifecycle, extracted from `loop.zig`.
//!
//! `executeAndReport` runs ONE leased event end-to-end: prepare a per-lease
//! workspace, materialize the installed bundle, hydrate prior memory over the
//! trusted plane, fork the sandboxed child via `child_supervisor`, forward
//! live-tail activity + memory frames as it streams, and report the terminal
//! outcome to the control plane. `loop.pollAndProcess` calls into here once it
//! has a lease; the parent loop / drain machinery stays in `loop.zig`. The split
//! keeps both files focused and within the line budget (RULE FLL).

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const contract = @import("contract");
const constants = common;

const Config = @import("config.zig");
const client_mod = @import("control_plane_client.zig");
const client_errors = @import("../engine/client_errors.zig");
const child_supervisor = @import("../child_supervisor.zig");
const bundle_extract = @import("../bundle_extract.zig");
const forwarders = @import("forwarders.zig");
const renew_driver = @import("renew_driver.zig");
const RenewDriver = renew_driver.RenewDriver(*client_mod);
// splitFields stays in loop.zig (pub, unit-tested there) because the token-split
// wire width is a runner policy; the verdict→wire projection belongs to
// `report_mapping`. loop imports this file for executeAndReport — a runtime
// function reference, no comptime cycle.
const loop = @import("loop.zig");

const protocol = contract.protocol;
const report_mapping = contract.report_mapping;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

// Cause line for a pre-fork bundle-materialization failure. Static — the
// report request borrows it for the POST; nothing frees it.
const DETAIL_BUNDLE_DOWNLOAD = "the runner could not fetch the fleet bundle from the control plane";
const DETAIL_BUNDLE_MALFORMED = "the fleet bundle archive could not be read - it is malformed or carries an unsafe path";
const DETAIL_BUNDLE_MEMORY = "the runner ran out of memory materializing the fleet bundle - the bundle itself is intact";

/// Fans the supervisor's renewal tick out to the periodic work that rides it:
/// the activity batch's staleness flush, then the renewal decision itself.
/// `pub` for the sibling `lease_run_exec_test.zig` (the fork boundary keeps the
/// full `executeAndReport` arc in the integration lane; the fanout is proven here).
pub const TickFanout = struct {
    forwarder: *forwarders.ActivityForwarder,
    driver: *RenewDriver,

    fn onTick(ctx: *anyopaque, now_ms: i64, usage: child_supervisor.UsageSnapshot) child_supervisor.RenewDecision {
        const self: *TickFanout = @ptrCast(@alignCast(ctx));
        self.forwarder.flushIfStale(now_ms);
        return self.driver.tick(now_ms, usage);
    }

    pub fn hook(self: *TickFanout) child_supervisor.RenewHook {
        return .{ .ctx = self, .onTick = onTick, .tick_ms = constants.RENEWAL_TICK_MS };
    }
};

/// Forwards the sandboxed child's on-demand mint asks to the daemon broker over
/// the agt_r plane (M102 §3). Holds the lease binding the mint server-side: a
/// child-supplied workspace is impossible — `cp.mint` sends only `lease_id`, and
/// the daemon derives the workspace from it (Invariant 2). The minted token is
/// duped into the read loop's `alloc` and freed there after it frames the reply.
/// `pub` so `credential_mint_e2e_test.zig` drives the PRODUCTION forwarder over
/// the real control plane instead of a private re-implementation of it.
pub const MintForwarder = struct {
    cp: *client_mod,
    runner_token: []const u8,
    lease_id: []const u8,
    deadline_ms: u31,

    fn onMint(ctx: *anyopaque, alloc: std.mem.Allocator, integration: []const u8, scope: ?[]const u8) child_supervisor.CredentialOutcome {
        const self: *MintForwarder = @ptrCast(@alignCast(ctx));
        return switch (self.cp.mint(alloc, self.runner_token, self.lease_id, integration, scope, self.deadline_ms)) {
            .minted => |m| .{ .minted = .{ .token = m.token, .expires_at_ms = m.expires_at_ms } },
            .rejected => .rejected,
        };
    }

    pub fn hook(self: *MintForwarder) child_supervisor.MintHook {
        return .{ .ctx = self, .onMint = onMint };
    }
};

/// Execute one leased event in a sandboxed child and report the result to the
/// control plane, forwarding live-tail activity frames as the child streams them.
pub fn executeAndReport(
    io: std.Io,
    alloc: std.mem.Allocator,
    cp: *client_mod,
    runner_token: []const u8,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    payload: protocol.LeasePayload,
) void {
    log.debug("lease_acquired", .{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
    });

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    // Back off on a workspace-prep failure before returning: otherwise a
    // persistent failure (e.g. an unwritable workspace base) hot-spins the
    // worker's poll loop — amplified ×worker_count under the pool.
    const workspace_path = prepareWorkspace(io, &ws_buf, cfg.storage_home, payload.lease_id) orelse {
        sleepMs(io, constants.backoff.ms(0));
        return;
    };
    defer cleanupWorkspace(io, workspace_path);

    // Materialize the installed bundle's support files into the workspace before
    // the fork (no-bundle / skill-only leases are a no-op). A hard failure reports
    // a startup failure and skips execution (retry deferred — spec Failure Modes).
    if (!materializeBundle(io, alloc, cp, runner_token, cfg, workspace_path, payload)) return;

    var forwarder = forwarders.ActivityForwarder{ .alloc = alloc, .cp = cp, .runner_token = runner_token, .lease_id = payload.lease_id, .deadline_ms = cfg.cp_deadlines.activity_ms };
    defer forwarder.deinit();
    const sink = child_supervisor.ActivitySink{ .ctx = &forwarder, .forward = forwarders.ActivityForwarder.forward };
    var driver = RenewDriver.init(alloc, cp, runner_token, payload, cfg.cp_deadlines.renew_ms);
    var fanout = TickFanout{ .forwarder = &forwarder, .driver = &driver };
    var minter = MintForwarder{ .cp = cp, .runner_token = runner_token, .lease_id = payload.lease_id, .deadline_ms = cfg.cp_deadlines.default_ms };

    // Hydrate the fleet's prior memory over the trusted plane BEFORE the fork so
    // the child seeds its in-run store from it — the child makes no network call
    // and holds no token. A hydrate miss degrades to empty memory, never blocks.
    const hydrated = cp.memoryHydrate(alloc, runner_token, payload.event.fleet_id, cfg.cp_deadlines.default_ms) catch |err| blk: {
        log.warn("memory_hydrate_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .fleet_id = payload.event.fleet_id, .err = @errorName(err) });
        break :blk null;
    };
    defer if (hydrated) |h| h.deinit();
    const hydrated_memory: []const protocol.MemoryDelta = if (hydrated) |h| h.value.memory else &.{};

    var mem_forwarder = forwarders.MemoryForwarder{
        .alloc = alloc,
        .cp = cp,
        .runner_token = runner_token,
        .fleet_id = payload.event.fleet_id,
        .lease_id = payload.lease_id,
        .fencing_token = payload.fencing_token,
        .deadline_ms = cfg.cp_deadlines.default_ms,
    };
    const mem_sink = child_supervisor.MemorySink{ .ctx = &mem_forwarder, .forward = forwarders.MemoryForwarder.forward };

    const start_ms = clock.nowMillis();
    const result = child_supervisor.run(io, alloc, cfg, env_map, workspace_path, payload, hydrated_memory, sink, mem_sink, fanout.hook(), minter.hook());
    const wall_ms: u64 = @intCast(@max(0, clock.nowMillis() - start_ms));
    defer if (result.content.len > 0) alloc.free(result.content);
    defer {
        const detail = result.failureDetail();
        if (detail.len > 0) alloc.free(detail);
    }
    // Ship whatever the batch still holds before the terminal report.
    forwarder.flush();

    log.debug("execute_completed", .{ .lease_id = payload.lease_id, .exit_ok = result.succeeded(), .wall_ms = wall_ms });

    const splits = loop.splitFields(result);
    const report = report_mapping.toReport(result, .{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
        .fencing_token = payload.fencing_token,
        .wall_ms = wall_ms,
        .input_tokens = splits.input_tokens,
        .cached_input_tokens = splits.cached_input_tokens,
        .output_tokens = splits.output_tokens,
        .checkpoint_response = result.content,
    });
    cp.report(alloc, runner_token, report, cfg.cp_deadlines.report_ms) catch |err| {
        log.err("report_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = payload.lease_id, .err = @errorName(err) });
        sleepMs(io, constants.backoff.ms(0)); // back off so a down report endpoint can't hot-spin the pool
        return;
    };

    log.debug("report_submitted", .{ .lease_id = payload.lease_id, .outcome = @tagName(report.outcome) });
}

/// Materialize the leased bundle's support files into `workspace_path` before the
/// child forks. Returns true to proceed (no bundle, skill-only, or extracted OK);
/// false after reporting a startup failure (download/extract failed) so the caller
/// returns without executing. Retry deferred (spec Failure Modes).
fn materializeBundle(io: std.Io, alloc: std.mem.Allocator, cp: *client_mod, runner_token: []const u8, cfg: Config, workspace_path: []const u8, payload: protocol.LeasePayload) bool {
    const manifest = payload.bundle orelse return true;
    switch (bundle_extract.materialize(io, alloc, cp, runner_token, cfg.storage_home, workspace_path, manifest, cfg.cp_deadlines.default_ms)) {
        .ready => return true,
        .failed => |cause| {
            reportStartupFailure(alloc, cp, runner_token, payload, cfg.cp_deadlines.report_ms, detailFor(cause));
            return false;
        },
    }
}

/// One cause line per distinguishable failure, matching the runner's convention
/// (`child_supervisor_result`, `selftest`). A collapsed detail cannot tell an
/// operator whether to fix their bundle or their host. `pub` for the sibling
/// `lease_run_exec_test.zig`.
pub fn detailFor(cause: bundle_extract.MaterializeFailure) []const u8 {
    return switch (cause) {
        .download => DETAIL_BUNDLE_DOWNLOAD,
        .malformed => DETAIL_BUNDLE_MALFORMED,
        .memory => DETAIL_BUNDLE_MEMORY,
    };
}

/// Report a pre-execution bundle-materialization failure as a startup failure so
/// the event is finalized (`fleet_error` / `startup_posture`) instead of silently
/// expiring and redelivering forever. No child forked → no tokens to settle. Retry
/// is deferred; a failed report is logged and the lease expires for reclaim.
fn reportStartupFailure(alloc: std.mem.Allocator, cp: *client_mod, runner_token: []const u8, payload: protocol.LeasePayload, deadline_ms: u31, detail: []const u8) void {
    const result: contract.execution_result.ExecutionResult = .{
        .outcome = .{ .failed = .{ .class = .startup_posture, .detail = detail } },
    };
    cp.report(alloc, runner_token, report_mapping.toReport(result, .{
        .lease_id = payload.lease_id,
        .event_id = payload.event.event_id,
        .fencing_token = payload.fencing_token,
        .wall_ms = 0,
    }), deadline_ms) catch |err| {
        log.err("bundle_startup_report_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .lease_id = payload.lease_id, .err = @errorName(err) });
    };
}

/// Create a per-lease workspace directory. Writes into caller-owned `buf`; returns
/// a slice into `buf` (valid for caller's stack frame) or null on error.
/// `pub` for the sibling `lease_run_workspace_test.zig`.
pub fn prepareWorkspace(io: std.Io, buf: *[std.fs.max_path_bytes]u8, base: []const u8, lease_id: []const u8) ?[]const u8 {
    const path = std.fmt.bufPrint(buf, "{s}/{s}", .{ base, lease_id }) catch {
        log.err("workspace_path_fmt_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .lease_id = lease_id });
        return null;
    };
    std.Io.Dir.createDirAbsolute(io, path, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            log.err("workspace_mkdir_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = path, .err = @errorName(err) });
            return null;
        },
    };
    return path;
}

/// Delete the per-lease workspace directory tree; failure is logged and ignored.
/// `pub` for the sibling `lease_run_workspace_test.zig`.
pub fn cleanupWorkspace(io: std.Io, path: []const u8) void {
    std.Io.Dir.cwd().deleteTree(io, path) catch |err| {
        log.warn("workspace_cleanup_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = path, .err = @errorName(err) });
    };
}

/// Sleep for `ms` milliseconds.
fn sleepMs(io: std.Io, ms: u64) void {
    io.sleep(std.Io.Duration.fromMilliseconds(@intCast(ms)), .awake) catch return;
}

// ── Tests ────────────────────────────────────────────────────────────────
// The full lifecycle needs a control plane and a sandboxed child; what is
// provable here is the workspace half — the filesystem contract every lease
// passes through before and after execution — and the no-bundle early return.

const testing = std.testing;

/// Absolute scratch base, per the sibling `bundle_extract_test.zig` pattern:
/// `prepareWorkspace` builds absolute paths, so a relative tmp dir cannot
/// stand in for the base.
const TEST_WS_BASE = "/tmp/agentsfleet-lease-run-test";

test "prepareWorkspace creates the lease directory and survives re-preparation" {
    const io = testing.io;
    std.Io.Dir.createDirAbsolute(io, TEST_WS_BASE, .default_dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => return err,
    };
    defer std.Io.Dir.cwd().deleteTree(io, TEST_WS_BASE) catch {};

    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = prepareWorkspace(io, &buf, TEST_WS_BASE, "lease-workspace-test") orelse
        return error.TestExpectedEqual;
    try testing.expect(std.mem.endsWith(u8, path, "lease-workspace-test"));

    // A redelivered lease prepares the same path again; PathAlreadyExists is
    // absorbed rather than failing the lease.
    var buf2: [std.fs.max_path_bytes]u8 = undefined;
    _ = prepareWorkspace(io, &buf2, TEST_WS_BASE, "lease-workspace-test") orelse
        return error.TestExpectedEqual;

    cleanupWorkspace(io, path);
    try testing.expectError(
        error.FileNotFound,
        std.Io.Dir.cwd().access(io, TEST_WS_BASE ++ "/lease-workspace-test", .{}),
    );
}

test "prepareWorkspace refuses a path past the buffer instead of truncating" {
    // A truncated path would be PREPARED — just somewhere else — and the child
    // would then run in a directory nothing cleans up. Refusal is the contract.
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const base = "x" ** (std.fs.max_path_bytes - 8);
    try testing.expectEqual(
        @as(?[]const u8, null),
        prepareWorkspace(testing.io, &buf, base, "lease-overflow-probe"),
    );
}

test "cleanupWorkspace tolerates a workspace that is already gone" {
    // Reaching the return is the assertion — the warn branch must swallow the
    // miss, because cleanup runs on every exit path including the ones where
    // preparation itself failed.
    cleanupWorkspace(testing.io, "/nonexistent/agentsfleet-lease-test");
}

test "materializeBundle is a no-op for a lease without a bundle" {
    // The early return: no bundle manifest means proceed without touching the
    // network. cp is never dereferenced on this path, so a placeholder works.
    const payload = protocol.LeasePayload{
        .lease_id = "lease-no-bundle",
        .fencing_token = 1,
        .lease_expires_at = 1_700_000_120_000,
        .secret_delivery = .@"inline",
        .event = .{
            .event_id = "1700000000000-0",
            .fleet_id = "0190aaaa-bbbb-7ccc-8ddd-000000000001",
            .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
            .actor = "steer:test",
            .event_type = .chat,
            .request_json = "{}",
            .created_at = 1_700_000_000_000,
        },
        .policy = .{},
    };
    var cp: client_mod = undefined;
    try testing.expect(materializeBundle(
        testing.io,
        testing.allocator,
        &cp,
        "agt_r-test-token",
        undefined,
        "/tmp/unused",
        payload,
    ));
}
