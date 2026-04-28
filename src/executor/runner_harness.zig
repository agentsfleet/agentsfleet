//! Test-only harness path for the executor runner.
//!
//! Activated at build time by `-Dexecutor_harness=true`, which produces the
//! `zombied-executor-harness` binary. The production binary
//! (`zombied-executor`) is built with the flag false, so the comptime
//! branch in `runner.execute` is stripped and none of this code ships.
//!
//! Purpose: integration tests that need deterministic frame emission and
//! synthetic completion without burning tokens on a real LLM. The harness
//! reads a script from `EXECUTOR_HARNESS_SCRIPT` (absolute path to a JSON
//! file), emits the listed progress frames via the same `progress_writer`
//! the production runner uses, and returns a synthetic ExecutionResult.
//!
//! Script shape (all fields optional except `frames`):
//! {
//!   "frames": [
//!     { "kind": "tool_call_started",   "name": "http.request", "args_redacted": "{\"url\":\"...\"}" },
//!     { "kind": "tool_call_progress",  "name": "http.request", "elapsed_ms": 2000, "delay_before_ms": 2000 },
//!     { "kind": "agent_response_chunk","text": "Hello" },
//!     { "kind": "tool_call_completed", "name": "http.request", "ms": 250 }
//!   ],
//!   "result": { "content": "ok", "exit_ok": true }
//! }
//!
//! `delay_before_ms` is honored before the frame is emitted — used by the
//! tool_call_progress heartbeat tests that assert ≥3 frames at ~2s
//! intervals over a 6s tool call. Without a script, the harness returns
//! an empty success without emitting anything.

const std = @import("std");
const types = @import("types.zig");
const progress_callbacks = @import("progress_callbacks.zig");
const progress_writer_mod = @import("progress_writer.zig");

const log = std.log.scoped(.executor_runner_harness);

const SCRIPT_ENV_VAR = "EXECUTOR_HARNESS_SCRIPT";
const MAX_SCRIPT_BYTES: usize = 1 * 1024 * 1024;

pub fn execute(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    context: ?std.json.Value,
    progress: ?*const progress_writer_mod,
) types.ExecutionResult {
    _ = workspace_path;
    _ = agent_config;
    _ = tools_spec;
    _ = context;

    log.info("harness.execute message_len={d}", .{if (message) |m| m.len else 0});

    const script_path = std.process.getEnvVarOwned(alloc, SCRIPT_ENV_VAR) catch {
        // No script set — emit nothing, return empty success. Lets the
        // foundation be exercised without a script (smoke tests etc.).
        return .{ .content = "", .exit_ok = true };
    };
    defer alloc.free(script_path);

    return runScript(alloc, script_path, progress) catch |err| blk: {
        log.warn("harness.script_failed err={s}", .{@errorName(err)});
        break :blk .{ .content = "", .exit_ok = false, .failure = .startup_posture };
    };
}

fn runScript(
    alloc: std.mem.Allocator,
    script_path: []const u8,
    progress: ?*const progress_writer_mod,
) !types.ExecutionResult {
    const file = try std.fs.cwd().openFile(script_path, .{});
    defer file.close();
    const raw = try file.readToEndAlloc(alloc, MAX_SCRIPT_BYTES);
    defer alloc.free(raw);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, raw, .{});
    defer parsed.deinit();

    const root = parsed.value;
    if (root != .object) return error.InvalidScript;

    if (root.object.get("frames")) |frames_v| {
        if (frames_v != .array) return error.InvalidScript;
        for (frames_v.array.items) |frame_v| {
            try emitOne(alloc, frame_v, progress);
        }
    }

    // readResult returns content borrowed from `parsed`; dupe before
    // the defer above tears parsed down so the caller (handler) can
    // safely embed the string in its JSON response.
    const result = readResult(root.object.get("result"));
    const owned = try alloc.dupe(u8, result.content);
    return .{
        .content = owned,
        .exit_ok = result.exit_ok,
        .failure = result.failure,
        .token_count = result.token_count,
        .wall_seconds = result.wall_seconds,
    };
}

fn emitOne(
    alloc: std.mem.Allocator,
    frame_v: std.json.Value,
    progress: ?*const progress_writer_mod,
) !void {
    if (frame_v != .object) return error.InvalidScript;
    const obj = frame_v.object;

    if (obj.get("delay_before_ms")) |d| {
        if (d == .integer and d.integer > 0) {
            std.Thread.sleep(@as(u64, @intCast(d.integer)) * std.time.ns_per_ms);
        }
    }

    const kind_v = obj.get("kind") orelse return error.InvalidScript;
    if (kind_v != .string) return error.InvalidScript;
    const kind = progress_callbacks.FrameKind.fromSlice(kind_v.string) orelse return error.InvalidScript;

    const frame: progress_callbacks.ProgressFrame = switch (kind) {
        .tool_call_started => .{ .tool_call_started = .{
            .name = try requireString(obj, "name"),
            .args_redacted = try requireString(obj, "args_redacted"),
        } },
        .agent_response_chunk => .{ .agent_response_chunk = .{
            .text = try requireString(obj, "text"),
        } },
        .tool_call_completed => .{ .tool_call_completed = .{
            .name = try requireString(obj, "name"),
            .ms = try requireInt(obj, "ms"),
        } },
        .tool_call_progress => .{ .tool_call_progress = .{
            .name = try requireString(obj, "name"),
            .elapsed_ms = try requireInt(obj, "elapsed_ms"),
        } },
    };

    if (progress) |w| w.write(frame);
    _ = alloc;
}

fn requireString(obj: std.json.ObjectMap, key: []const u8) ![]const u8 {
    const v = obj.get(key) orelse return error.InvalidScript;
    if (v != .string) return error.InvalidScript;
    return v.string;
}

fn requireInt(obj: std.json.ObjectMap, key: []const u8) !i64 {
    const v = obj.get(key) orelse return error.InvalidScript;
    if (v != .integer) return error.InvalidScript;
    return v.integer;
}

fn readResult(result_v_opt: ?std.json.Value) types.ExecutionResult {
    const result_v = result_v_opt orelse return .{ .content = "", .exit_ok = true };
    if (result_v != .object) return .{ .content = "", .exit_ok = true };
    const obj = result_v.object;
    const content = if (obj.get("content")) |c| (if (c == .string) c.string else "") else "";
    const exit_ok = if (obj.get("exit_ok")) |e| (e == .bool and e.bool) else true;
    return .{ .content = content, .exit_ok = exit_ok };
}
