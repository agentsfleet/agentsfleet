const std = @import("std");

const db = @import("../db/pool.zig");
const obs_log = @import("../observability/logging.zig");
const common = @import("common.zig");
const spec_validator = @import("spec_validator.zig");

const log = std.log.scoped(.zombied);

pub fn run(alloc: std.mem.Allocator) !void {
    var args = std.process.args();
    _ = args.next(); // binary
    _ = args.next(); // "run"

    var spec_path: ?[]const u8 = null;
    var watch_flag = false;

    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--watch")) {
            watch_flag = true;
        } else {
            spec_path = arg;
        }
    }

    const path = spec_path orelse {
        std.debug.print("usage: zombied run <spec_path> [--watch]\n", .{});
        std.process.exit(1);
    };

    log.info("run.start spec_path={s} watch={}", .{ path, watch_flag });

    const spec_content = std.fs.cwd().readFileAlloc(alloc, path, 512 * 1024) catch |err| {
        std.debug.print("error reading spec: {any}\n", .{err});
        std.process.exit(1);
    };
    defer alloc.free(spec_content);

    // M16_002 §1: Validate spec before any network call.
    {
        var validation = spec_validator.validate(alloc, spec_content, std.fs.cwd()) catch |err| {
            std.debug.print("error: spec validation failed: {any}\n", .{err});
            std.process.exit(1);
        };
        defer validation.deinit(alloc);

        // Print any warnings first (non-blocking).
        for (validation.warnings.items) |w| {
            std.debug.print("warning: {s}\n", .{w});
        }

        if (validation.failure) |f| {
            switch (f) {
                .empty => {
                    std.debug.print("error: spec is empty\n", .{});
                },
                .no_actionable_content => {
                    std.debug.print("error: spec has no actionable content\n", .{});
                },
                .unresolved_ref => |p| {
                    std.debug.print("error: referenced path not found: {s}\n", .{p});
                },
            }
            std.process.exit(1);
        }
    }

    const pool = db.initFromEnvForRole(alloc, .worker) catch |err| {
        std.debug.print("fatal: database init failed: {any}\n", .{err});
        std.process.exit(1);
    };
    defer pool.deinit();

    common.runCanonicalMigrations(pool) catch |err| {
        obs_log.logWarnErr(.zombied, err, "run.migration status=skipped", .{});
    };

    log.info("run.spec_loaded bytes={d}", .{spec_content.len});

    if (!watch_flag) {
        log.info("run.hint action=POST /v1/runs to trigger pipeline", .{});
        return;
    }

    // --watch: POST to API, then stream SSE output.
    const base_url = std.process.getEnvVarOwned(alloc, "ZOMBIED_API_URL") catch |err| {
        if (err == error.EnvironmentVariableNotFound) {
            std.debug.print("error: ZOMBIED_API_URL not set\n", .{});
            std.process.exit(1);
        }
        return err;
    };
    defer alloc.free(base_url);

    const api_key = std.process.getEnvVarOwned(alloc, "ZOMBIED_API_KEY") catch |err| {
        if (err == error.EnvironmentVariableNotFound) {
            std.debug.print("error: ZOMBIED_API_KEY not set\n", .{});
            std.process.exit(1);
        }
        return err;
    };
    defer alloc.free(api_key);

    const workspace_id = std.process.getEnvVarOwned(alloc, "ZOMBIED_WORKSPACE_ID") catch |err| {
        if (err == error.EnvironmentVariableNotFound) {
            std.debug.print("error: ZOMBIED_WORKSPACE_ID not set\n", .{});
            std.process.exit(1);
        }
        return err;
    };
    defer alloc.free(workspace_id);

    // M16_002: Read git HEAD SHA natively (no subprocess).
    const base_commit_sha = readGitHeadSha(alloc);
    defer if (base_commit_sha) |s| alloc.free(s);

    const post_result = postRunAndGetId(alloc, base_url, api_key, workspace_id, spec_content, base_commit_sha) catch |err| {
        std.debug.print("error: failed to start run: {any}\n", .{err});
        std.process.exit(1);
    };

    // M16_002 §2.4: Handle dedup response — print note and exit 0.
    if (post_result.is_dedup) {
        std.debug.print("note: duplicate submission — existing run {s} is already in progress\n", .{post_result.run_id});
        alloc.free(post_result.run_id);
        return;
    }

    const run_id = post_result.run_id;
    defer alloc.free(run_id);

    log.info("run.started run_id={s}", .{run_id});
    std.debug.print("watch: connecting to SSE stream for run {s}...\n", .{run_id});

    streamRunOutput(alloc, base_url, api_key, run_id) catch |err| {
        std.debug.print("error: stream failed: {any}\n", .{err});
        std.process.exit(1);
    };
}

const PostRunResult = struct {
    run_id: []u8,
    is_dedup: bool,
};

fn postRunAndGetId(
    alloc: std.mem.Allocator,
    base_url: []const u8,
    api_key: []const u8,
    workspace_id: []const u8,
    spec_content: []const u8,
    base_commit_sha: ?[]const u8,
) !PostRunResult {
    const url = try std.fmt.allocPrint(alloc, "{s}/v1/runs", .{base_url});
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{api_key});
    defer alloc.free(auth_header);

    const RunRequest = struct {
        workspace_id: []const u8,
        spec_markdown: []const u8,
        mode: []const u8,
        requested_by: []const u8,
        idempotency_key: []const u8,
        base_commit_sha: ?[]const u8,
    };

    // Use spec_content as the markdown; idempotency_key defaults to a timestamp-based value.
    const idempotency_key = try std.fmt.allocPrint(alloc, "cli-{d}", .{std.time.milliTimestamp()});
    defer alloc.free(idempotency_key);

    const body = try std.json.Stringify.valueAlloc(alloc, RunRequest{
        .workspace_id = workspace_id,
        .spec_markdown = spec_content,
        .mode = "auto",
        .requested_by = "cli",
        .idempotency_key = idempotency_key,
        .base_commit_sha = base_commit_sha,
    }, .{});
    defer alloc.free(body);

    var client = std.http.Client{ .allocator = alloc };
    defer client.deinit();

    var response_body: std.ArrayList(u8) = .{};
    defer response_body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &response_body);

    const uri = try std.Uri.parse(url);
    const result = try client.fetch(.{
        .method = .POST,
        .location = .{ .uri = uri },
        .extra_headers = &.{
            .{ .name = "Authorization", .value = auth_header },
            .{ .name = "Content-Type", .value = "application/json" },
        },
        .payload = body,
        .response_writer = &aw.writer,
    });

    if (result.status != .ok and result.status != .created and result.status != .accepted) {
        std.debug.print("error: POST /v1/runs returned {d}\n{s}\n", .{ @intFromEnum(result.status), response_body.items });
        return error.RunStartFailed;
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, response_body.items, .{});
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.InvalidResponse,
    };

    // M16_002 §2.4: Detect dedup via dedup_hit field in response body.
    const is_dedup = if (obj.get("dedup_hit")) |v| switch (v) {
        .bool => |b| b,
        else => false,
    } else false;

    const rid_val = obj.get("run_id") orelse return error.MissingRunId;
    const rid_str = switch (rid_val) {
        .string => |s| s,
        else => return error.InvalidRunId,
    };

    return PostRunResult{
        .run_id = try alloc.dupe(u8, rid_str),
        .is_dedup = is_dedup,
    };
}

/// Walk up from CWD (or use $GIT_DIR env) to open the git metadata directory.
/// Returns an open Dir; caller must close it. Returns null if no .git found.
fn openGitDir(alloc: std.mem.Allocator) ?std.fs.Dir {
    // $GIT_DIR overrides the default .git search.
    if (std.process.getEnvVarOwned(alloc, "GIT_DIR") catch null) |path| {
        defer alloc.free(path);
        return std.fs.openDirAbsolute(path, .{}) catch null;
    }
    // Walk up from CWD looking for a .git subdirectory.
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const cwd_path = std.fs.cwd().realpath(".", &buf) catch return null;
    var current: []const u8 = cwd_path;
    while (true) {
        var dir = std.fs.openDirAbsolute(current, .{}) catch return null;
        defer dir.close();
        if (dir.openDir(".git", .{}) catch null) |git_dir| return git_dir;
        const parent = std.fs.path.dirname(current) orelse return null;
        if (std.mem.eql(u8, parent, current)) return null; // reached filesystem root
        current = parent;
    }
}

/// M16_002: Read git HEAD SHA via native file I/O (no subprocess).
/// Walks up the directory tree to find .git/; respects $GIT_DIR.
/// Returns an owned slice or null on any error.
fn readGitHeadSha(alloc: std.mem.Allocator) ?[]u8 {
    var git_dir = openGitDir(alloc) orelse return null;
    defer git_dir.close();

    const head_content = git_dir.readFileAlloc(alloc, "HEAD", 256) catch return null;
    defer alloc.free(head_content);

    const trimmed = std.mem.trimRight(u8, head_content, "\r\n ");

    // Symbolic ref: "ref: refs/heads/<branch>"
    const ref_prefix = "ref: ";
    if (std.mem.startsWith(u8, trimmed, ref_prefix)) {
        const ref_path = trimmed[ref_prefix.len..];
        const sha_content = git_dir.readFileAlloc(alloc, ref_path, 128) catch {
            return readPackedRef(alloc, git_dir, ref_path);
        };
        defer alloc.free(sha_content);
        const sha = std.mem.trimRight(u8, sha_content, "\r\n ");
        if (sha.len >= 40) return alloc.dupe(u8, sha[0..40]) catch null;
        return null;
    }

    // Detached HEAD — trimmed value is the SHA directly.
    if (trimmed.len >= 40) return alloc.dupe(u8, trimmed[0..40]) catch null;
    return null;
}

/// Scan packed-refs for the given ref and return its SHA.
fn readPackedRef(alloc: std.mem.Allocator, git_dir: std.fs.Dir, ref_name: []const u8) ?[]u8 {
    const packed_content = git_dir.readFileAlloc(alloc, "packed-refs", 64 * 1024) catch return null;
    defer alloc.free(packed_content);

    var lines = std.mem.splitScalar(u8, packed_content, '\n');
    while (lines.next()) |line| {
        if (std.mem.startsWith(u8, line, "#")) continue;
        // Format: "<sha> <ref_name>"
        const space = std.mem.indexOfScalar(u8, line, ' ') orelse continue;
        const sha = line[0..space];
        const name = std.mem.trimRight(u8, line[space + 1 ..], "\r");
        if (std.mem.eql(u8, name, ref_name) and sha.len >= 40) {
            return alloc.dupe(u8, sha[0..40]) catch null;
        }
    }
    return null;
}

const run_watch = @import("run_watch.zig");
const streamRunOutput = run_watch.streamRunOutput;
