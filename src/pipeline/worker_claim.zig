const std = @import("std");
const pg = @import("pg");
const billing = @import("../state/billing.zig");
const state = @import("../state/machine.zig");
const agents = @import("agents.zig");
const github_auth = @import("../auth/github.zig");
const err_classify = @import("../reliability/error_classify.zig");
const topology = @import("topology.zig");
const profile_resolver = @import("profile_resolver.zig");
const worker_state_mod = @import("worker_state.zig");
const worker_runtime = @import("worker_runtime.zig");
const worker_rate_limiter = @import("worker_rate_limiter.zig");
const worker_stage_executor = @import("worker_stage_executor.zig");
const metrics = @import("../observability/metrics.zig");
const events = @import("../events/bus.zig");
const prompt_events = @import("../observability/prompt_events.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const http_common = @import("../http/handlers/common.zig");
const obs_log = @import("../observability/logging.zig");
const log = std.log.scoped(.worker);

pub const ProcessConfig = struct {
    pool: *pg.Pool,
    execute: worker_stage_executor.ExecuteConfig,
};

fn beginTx(conn: *pg.Conn) !void {
    var tx = try conn.query("BEGIN", .{});
    tx.deinit();
}

fn commitTx(conn: *pg.Conn) !void {
    var tx = try conn.query("COMMIT", .{});
    tx.deinit();
}

fn rollbackTx(conn: *pg.Conn) void {
    var tx = conn.query("ROLLBACK", .{}) catch return;
    tx.deinit();
}

pub fn processNextRun(
    alloc: std.mem.Allocator,
    cfg: ProcessConfig,
    worker_state: *worker_state_mod.WorkerState,
    prompts: *const agents.PromptFiles,
    profile: *const topology.Profile,
    token_cache: *github_auth.TokenCache,
    tenant_limiter: *worker_rate_limiter.TenantRateLimiter,
    queued_run_id: []const u8,
) !void {
    var claim_arena = std.heap.ArenaAllocator.init(alloc);
    defer claim_arena.deinit();
    const claim_alloc = claim_arena.allocator();

    var conn = try cfg.pool.acquire();
    defer cfg.pool.release(conn);

    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

    var result = try conn.query(
        \\SELECT r.run_id, r.workspace_id, r.spec_id, r.tenant_id, r.attempt, r.request_id,
        \\       r.trace_id, r.requested_by, w.repo_url, w.default_branch,
        \\       s.file_path
        \\FROM runs r
        \\JOIN workspaces w ON w.workspace_id = r.workspace_id
        \\JOIN specs s ON s.spec_id = r.spec_id
        \\WHERE r.run_id = $1 AND r.state = 'SPEC_QUEUED' AND w.paused = false
        \\LIMIT 1
        \\FOR UPDATE OF r SKIP LOCKED
    , .{queued_run_id});
    defer result.deinit();

    const row = (try result.next()) orelse {
        try commitTx(conn);
        tx_open = false;
        return;
    };

    const run_id = try claim_alloc.dupe(u8, try row.get([]u8, 0));
    const workspace_id = try claim_alloc.dupe(u8, try row.get([]u8, 1));
    const spec_id = try claim_alloc.dupe(u8, try row.get([]u8, 2));
    const tenant_id = try claim_alloc.dupe(u8, try row.get([]u8, 3));
    const attempt = @as(u32, @intCast(try row.get(i32, 4)));
    const request_id_raw = try row.get(?[]u8, 5);
    const request_id = try claim_alloc.dupe(u8, request_id_raw orelse "-");
    const trace_id_raw = try row.get(?[]u8, 6);
    const trace_id = try claim_alloc.dupe(u8, trace_id_raw orelse "-");
    const requested_by_raw = try row.get(?[]u8, 7);
    const requested_by = try claim_alloc.dupe(u8, requested_by_raw orelse "");
    const repo_url = try claim_alloc.dupe(u8, try row.get([]u8, 8));
    const default_branch = try claim_alloc.dupe(u8, try row.get([]u8, 9));
    const spec_path = try claim_alloc.dupe(u8, try row.get([]u8, 10));
    _ = http_common.setTenantSessionContext(conn, tenant_id);

    result.drain() catch |err| {
        obs_log.logWarnErr(.worker, err, "claim query drain failed run_id={s}", .{run_id});
    };

    _ = try state.transition(conn, run_id, .RUN_PLANNED, .orchestrator, .PLAN_COMPLETE, "claimed by worker");

    try commitTx(conn);
    tx_open = false;

    var workspace_profile: ?topology.Profile = profile_resolver.loadWorkspaceActiveProfile(alloc, conn, workspace_id) catch |err| blk: {
        obs_log.logWarnErr(.worker, err, "active profile load failed; fallback to default workspace_id={s}", .{workspace_id});
        break :blk null;
    };
    defer if (workspace_profile) |*p| p.deinit();

    const effective_profile: *const topology.Profile = if (workspace_profile) |*p| p else profile;
    const using_fallback = workspace_profile == null;
    log.info("workspace profile resolved workspace_id={s} profile={s} fallback_default_v1={}", .{
        workspace_id,
        effective_profile.profile_id,
        using_fallback,
    });

    try worker_state_mod.beginRunIfActive(worker_state);
    defer worker_state.endRun();

    log.info("claimed run run_id={s} request_id={s} trace_id={s} attempt={d}", .{ run_id, request_id, trace_id, attempt });
    var claimed_detail: [192]u8 = undefined;
    const claimed_detail_slice = std.fmt.bufPrint(
        &claimed_detail,
        "request_id={s} trace_id={s} attempt={d}",
        .{ request_id, trace_id, attempt },
    ) catch "run_claimed";
    events.emit("run_claimed", run_id, claimed_detail_slice);
    prompt_events.emitBestEffort(conn, .{
        .event_type = .prompt_eval,
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .agent_id = effective_profile.profile_id,
        .config_version_id = null,
        .metadata_json = "{\"phase\":\"start\"}",
        .ts_ms = std.time.milliTimestamp(),
    });

    var run_failed = false;
    worker_stage_executor.executeRun(
        alloc,
        cfg.execute,
        &worker_state.running,
        prompts,
        effective_profile,
        conn,
        token_cache,
        .{
            .run_id = run_id,
            .request_id = request_id,
            .trace_id = trace_id,
            .workspace_id = workspace_id,
            .spec_id = spec_id,
            .tenant_id = tenant_id,
            .requested_by = requested_by,
            .repo_url = repo_url,
            .default_branch = default_branch,
            .spec_path = spec_path,
            .attempt = attempt,
            .agent_id = effective_profile.profile_id,
        },
        tenant_limiter,
    ) catch |err| {
        run_failed = true;
        prompt_events.emitBestEffort(conn, .{
            .event_type = .prompt_performance,
            .workspace_id = workspace_id,
            .tenant_id = tenant_id,
            .agent_id = effective_profile.profile_id,
            .config_version_id = null,
            .metadata_json = "{\"status\":\"failed\"}",
            .ts_ms = std.time.milliTimestamp(),
        });
        if (err == worker_runtime.WorkerError.ShutdownRequested or err == worker_runtime.WorkerError.RunDeadlineExceeded) {
            const reason_note = if (err == worker_runtime.WorkerError.RunDeadlineExceeded) "run deadline exceeded" else "shutdown requested";
            billing.finalizeRunForBilling(
                claim_alloc,
                conn,
                workspace_id,
                run_id,
                attempt,
                .non_billable,
            ) catch |billing_err| {
                obs_log.logWarnErr(.worker, billing_err, "billing finalize failed run_id={s}", .{run_id});
            };
            _ = state.transition(conn, run_id, .BLOCKED, .orchestrator, .AGENT_TIMEOUT, reason_note) catch |tx_err| {
                obs_log.logWarnErr(.worker, tx_err, "shutdown transition failed run_id={s}", .{run_id});
            };
            if (err == worker_runtime.WorkerError.RunDeadlineExceeded) {
                events.emit("run_deadline_exceeded", run_id, reason_note);
            }
            metrics.incRunsBlocked();
            return err;
        }

        const classified = err_classify.classify(err, null);
        var note_buf: [192]u8 = undefined;
        const note = std.fmt.bufPrint(&note_buf, "class={s} err={s}", .{ @tagName(classified.class), @errorName(err) }) catch @errorName(err);
        log.err("run failed run_id={s} class={s} retryable={} err={s}", .{ run_id, @tagName(classified.class), classified.retryable, @errorName(err) });
        var failed_detail: [224]u8 = undefined;
        const failed_detail_slice = std.fmt.bufPrint(
            &failed_detail,
            "request_id={s} trace_id={s} class={s} retryable={} err={s}",
            .{ request_id, trace_id, @tagName(classified.class), classified.retryable, @errorName(err) },
        ) catch "run_failed";
        events.emit("run_failed", run_id, failed_detail_slice);
        posthog_events.trackRunFailed(
            cfg.execute.posthog,
            posthog_events.distinctIdOrSystem(requested_by),
            run_id,
            workspace_id,
            @errorName(err),
            0,
        );
        billing.finalizeRunForBilling(
            claim_alloc,
            conn,
            workspace_id,
            run_id,
            attempt,
            .non_billable,
        ) catch |billing_err| {
            obs_log.logWarnErr(.worker, billing_err, "billing finalize failed run_id={s}", .{run_id});
        };
        _ = state.transition(conn, run_id, .BLOCKED, .orchestrator, classified.reason_code, note) catch |tx_err| {
            obs_log.logWarnErr(.worker, tx_err, "failure transition failed run_id={s}", .{run_id});
        };
        metrics.incRunsBlocked();
    };

    if (!run_failed) {
        prompt_events.emitBestEffort(conn, .{
            .event_type = .prompt_performance,
            .workspace_id = workspace_id,
            .tenant_id = tenant_id,
            .agent_id = effective_profile.profile_id,
            .config_version_id = null,
            .metadata_json = "{\"status\":\"completed\"}",
            .ts_ms = std.time.milliTimestamp(),
        });
    }
}
