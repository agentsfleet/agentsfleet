const std = @import("std");

const db = @import("../db/pool.zig");
const obs_log = @import("../observability/logging.zig");
const common = @import("common.zig");

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

    // --watch: POST to API, then stream SSE output
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

    const run_id = postRunAndGetId(alloc, base_url, api_key, workspace_id, spec_content) catch |err| {
        std.debug.print("error: failed to start run: {any}\n", .{err});
        std.process.exit(1);
    };
    defer alloc.free(run_id);

    log.info("run.started run_id={s}", .{run_id});
    std.debug.print("watch: connecting to SSE stream for run {s}...\n", .{run_id});

    streamRunOutput(alloc, base_url, api_key, run_id) catch |err| {
        std.debug.print("error: stream failed: {any}\n", .{err});
        std.process.exit(1);
    };
}

fn postRunAndGetId(
    alloc: std.mem.Allocator,
    base_url: []const u8,
    api_key: []const u8,
    workspace_id: []const u8,
    spec_content: []const u8,
) ![]u8 {
    const url = try std.fmt.allocPrint(alloc, "{s}/v1/runs", .{base_url});
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{api_key});
    defer alloc.free(auth_header);

    const RunRequest = struct { workspace_id: []const u8, spec: []const u8 };
    const body = try std.json.Stringify.valueAlloc(alloc, RunRequest{
        .workspace_id = workspace_id,
        .spec = spec_content,
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

    if (result.status != .ok and result.status != .created) {
        std.debug.print("error: POST /v1/runs returned {d}\n{s}\n", .{ @intFromEnum(result.status), response_body.items });
        return error.RunStartFailed;
    }

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, response_body.items, .{});
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.InvalidResponse,
    };

    const rid_val = obj.get("run_id") orelse return error.MissingRunId;
    const rid_str = switch (rid_val) {
        .string => |s| s,
        else => return error.InvalidRunId,
    };

    return alloc.dupe(u8, rid_str);
}

fn streamRunOutput(
    alloc: std.mem.Allocator,
    base_url: []const u8,
    api_key: []const u8,
    run_id: []const u8,
) !void {
    const url = try std.fmt.allocPrint(alloc, "{s}/v1/runs/{s}:stream", .{ base_url, run_id });
    defer alloc.free(url);

    const auth_header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{api_key});
    defer alloc.free(auth_header);

    var client = std.http.Client{ .allocator = alloc };
    defer client.deinit();

    var response_body: std.ArrayList(u8) = .{};
    defer response_body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &response_body);

    const uri = try std.Uri.parse(url);
    const result = try client.fetch(.{
        .method = .GET,
        .location = .{ .uri = uri },
        .extra_headers = &.{
            .{ .name = "Authorization", .value = auth_header },
            .{ .name = "Accept", .value = "text/event-stream" },
        },
        .response_writer = &aw.writer,
    });

    if (result.status != .ok) {
        std.debug.print("error: stream returned {d}\n", .{@intFromEnum(result.status)});
        return error.StreamFailed;
    }

    // Parse and render SSE lines
    var it = std.mem.splitScalar(u8, response_body.items, '\n');
    var current_data: ?[]const u8 = null;
    var current_event: []const u8 = "message";

    while (it.next()) |line| {
        const trimmed = std.mem.trimRight(u8, line, "\r");
        if (trimmed.len == 0) {
            // Empty line = event boundary
            if (current_data) |data| {
                renderSseEvent(alloc, current_event, data);
            }
            current_data = null;
            current_event = "message";
            continue;
        }
        if (std.mem.startsWith(u8, trimmed, "data: ")) {
            current_data = trimmed["data: ".len..];
        } else if (std.mem.startsWith(u8, trimmed, "event: ")) {
            current_event = trimmed["event: ".len..];
        }
        // id: and comment lines are ignored for rendering
    }
}

fn renderSseEvent(alloc: std.mem.Allocator, event_type: []const u8, data: []const u8) void {
    if (std.mem.eql(u8, event_type, "run_complete")) {
        std.debug.print("[done] run complete: {s}\n", .{data});
        return;
    }
    if (!std.mem.eql(u8, event_type, "gate_result")) return;

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, data, .{}) catch return;
    defer parsed.deinit();

    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return,
    };

    const gate_name = if (obj.get("gate_name")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";
    const outcome = if (obj.get("outcome")) |v| switch (v) {
        .string => |s| s,
        else => "?",
    } else "?";
    const loop_n = if (obj.get("loop")) |v| switch (v) {
        .integer => |i| i,
        else => 0,
    } else 0;
    const wall_ms = if (obj.get("wall_ms")) |v| switch (v) {
        .integer => |i| i,
        else => 0,
    } else 0;

    std.debug.print("[{s}] {s} (loop {d}, {d}ms)\n", .{ gate_name, outcome, loop_n, wall_ms });
}
