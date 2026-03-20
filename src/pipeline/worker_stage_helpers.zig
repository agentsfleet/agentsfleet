const std = @import("std");
const pg = @import("pg");
const git = @import("../git/ops.zig");
const reliable = @import("../reliability/reliable_call.zig");
const worker_runtime = @import("worker_runtime.zig");
const scoring = @import("scoring.zig");
const obs_log = @import("../observability/logging.zig");
const langfuse = @import("../observability/langfuse.zig");
const state = @import("../state/machine.zig");
const types = @import("../types.zig");
const agents = @import("agents.zig");
const wst = @import("worker_stage_types.zig");

const CommitRetryCtx = struct {
    alloc: std.mem.Allocator,
    wt_path: []const u8,
    rel_path: []const u8,
    content: []const u8,
    msg: []const u8,
};

fn opCommitArtifact(ctx: CommitRetryCtx, _: u32) !void {
    return git.commitFile(ctx.alloc, ctx.wt_path, ctx.rel_path, ctx.content, ctx.msg, "UseZombie Bot", "bot@usezombie.dev");
}

pub fn commitArtifact(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    ctx: wst.RunContext,
    wt: *git.WorktreeHandle,
    running: *const std.atomic.Value(bool),
    deadline_ms: i64,
    rel_path: []const u8,
    content: []const u8,
    msg: []const u8,
    actor: types.Actor,
    attempt: u32,
) !void {
    try reliable.call(void, CommitRetryCtx{
        .alloc = alloc,
        .wt_path = wt.path,
        .rel_path = rel_path,
        .content = content,
        .msg = msg,
    }, opCommitArtifact, worker_runtime.retryOptionsForRun(@constCast(running), deadline_ms, 1, 300, 2_000, "git_commit_artifact"));

    const checksum = sha256Hex(content);
    const object_key = try std.fmt.allocPrint(alloc, "docs/runs/{s}/{s}", .{ ctx.run_id, std.fs.path.basename(rel_path) });

    const name = std.fs.path.basename(rel_path);
    try state.registerArtifact(conn, ctx.run_id, attempt, name, object_key, &checksum, actor);
}

pub fn loadScoreContextBestEffort(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    agent_id: []const u8,
) ![]const u8 {
    const config = scoring.queryScoringConfig(conn, alloc, workspace_id) catch |err| {
        obs_log.logWarnErr(.scoring, err, "scoring config lookup failed workspace_id={s}; using orientation block", .{workspace_id});
        return scoring.orientationContext(alloc);
    };
    return scoring.buildScoringContextForEcho(conn, alloc, workspace_id, agent_id, config);
}

/// Best-effort Langfuse trace emission. Uses the async exporter when installed
/// on worker startup, otherwise falls back to synchronous best-effort emission.
pub fn emitLangfuseTrace(
    alloc: std.mem.Allocator,
    ctx: wst.RunContext,
    stage_id: []const u8,
    role_id: []const u8,
    result: agents.AgentResult,
) void {
    langfuse.emitTraceAsyncOrFallback(alloc, .{
        .trace_id = ctx.trace_id,
        .run_id = ctx.run_id,
        .stage_id = stage_id,
        .role_id = role_id,
        .token_count = result.token_count,
        .wall_seconds = result.wall_seconds,
        .exit_ok = result.exit_ok,
        .timestamp_ms = std.time.milliTimestamp(),
    });
}

fn sha256Hex(data: []const u8) [64]u8 {
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(data, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}
