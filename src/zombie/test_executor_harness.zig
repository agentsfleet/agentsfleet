// Test fixture: spawns `zombied-executor-harness` as a child process and
// exposes a connected ExecutorClient for worker-side integration tests.
//
// Usage:
//   var harness = try Harness.start(alloc, .{ .script_json = my_script });
//   defer harness.deinit();
//   const cfg = EventLoopConfig{ .executor = &harness.executor, ... };
//   _ = writepath.run(alloc, &session, &evt, cfg, &consec);
//
// The harness binary's runner branch is comptime-gated to runner_harness.zig
// (see docs/architecture/ §9 streaming substrate notes). When the script
// file references frames, the harness emits them via the production
// ProgressWriter — same wire shape, real socket round-trip — so the worker's
// streaming reader exercises the genuine code path.

const std = @import("std");
const builtin = @import("builtin");
const Allocator = std.mem.Allocator;

const executor_client = @import("../executor/client.zig");

const log = std.log.scoped(.test_executor_harness);

const Harness = @This();

const DEFAULT_BIN_PATH = "zig-out/bin/zombied-executor-harness";
const SOCKET_POLL_INTERVAL_MS: u64 = 25;
const SOCKET_POLL_BUDGET_MS: u64 = 3_000;

alloc: Allocator,
child: std.process.Child,
socket_path: []u8,
script_path: []u8,
executor: executor_client.ExecutorClient,
connected: bool,

pub const Options = struct {
    /// Pre-stringified harness script JSON. See runner_harness.zig docs for
    /// shape. Pass an empty slice for an empty-script run (no frames, exit_ok).
    script_json: []const u8 = "",
    /// Optional: override the rpc_version the harness advertises in its
    /// HELLO frame. Forwarded as EXECUTOR_HARNESS_RPC_VERSION. Used by the
    /// version-mismatch test; leave null for normal runs.
    rpc_version: ?u32 = null,
    /// Wait for the executor socket to come up + connect the client. Set
    /// false when the test wants to assert connect-time behaviour itself
    /// (e.g. version-mismatch tests inspecting the connect error).
    auto_connect: bool = true,
};

pub fn start(alloc: Allocator, opts: Options) !Harness {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const bin_path = try resolveBinaryPath(alloc);
    defer alloc.free(bin_path);

    const paths = try uniquePaths(alloc);
    errdefer alloc.free(paths.socket_path);
    errdefer alloc.free(paths.script_path);

    try writeFile(paths.script_path, opts.script_json);
    errdefer std.fs.deleteFileAbsolute(paths.script_path) catch {};

    var child = try spawnChild(alloc, bin_path, paths, opts);
    errdefer terminateChild(&child);

    var harness: Harness = .{
        .alloc = alloc,
        .child = child,
        .socket_path = paths.socket_path,
        .script_path = paths.script_path,
        .executor = executor_client.ExecutorClient.init(alloc, paths.socket_path),
        .connected = false,
    };

    // Always wait for the listener socket — the binary writes it during
    // its bind step, well before any handshake. auto_connect only governs
    // whether we then drive the client through HELLO; tests that want to
    // observe connect-time errors (e.g. version mismatch) opt out of the
    // automatic connect and call it themselves.
    try harness.waitForSocket();
    if (opts.auto_connect) {
        harness.executor.connect() catch |err| {
            return err;
        };
        harness.connected = true;
    }

    return harness;
}

pub fn deinit(self: *Harness) void {
    if (self.connected) self.executor.close();
    terminateChild(&self.child);
    std.fs.deleteFileAbsolute(self.socket_path) catch {};
    std.fs.deleteFileAbsolute(self.script_path) catch {};
    self.alloc.free(self.socket_path);
    self.alloc.free(self.script_path);
}

/// Convenience accessor: pointer to the connected `ExecutorClient`. Worker
/// callers stash this in `EventLoopConfig.executor` for `writepath.run`.
pub fn executorPtr(self: *Harness) *executor_client.ExecutorClient {
    return &self.executor;
}

// ── Internals ───────────────────────────────────────────────────────────────

const PathPair = struct { socket_path: []u8, script_path: []u8 };

fn resolveBinaryPath(alloc: Allocator) ![]u8 {
    if (std.process.getEnvVarOwned(alloc, "EXECUTOR_HARNESS_BIN")) |override| {
        const exists = blk: {
            _ = std.fs.cwd().statFile(override) catch break :blk false;
            break :blk true;
        };
        if (exists) return override;
        log.warn("harness.bin_override_missing path={s}", .{override});
        alloc.free(override);
    } else |_| {}

    _ = std.fs.cwd().statFile(DEFAULT_BIN_PATH) catch {
        log.warn("harness.binary_not_found path={s} (run `zig build` to install)", .{DEFAULT_BIN_PATH});
        return error.SkipZigTest;
    };
    return alloc.dupe(u8, DEFAULT_BIN_PATH);
}

fn uniquePaths(alloc: Allocator) !PathPair {
    var seed_buf: [8]u8 = undefined;
    std.crypto.random.bytes(&seed_buf);
    var hex_buf: [16]u8 = undefined;
    const charset = "0123456789abcdef";
    for (seed_buf, 0..) |b, i| {
        hex_buf[i * 2] = charset[b >> 4];
        hex_buf[i * 2 + 1] = charset[b & 0xF];
    }
    const tag: []const u8 = hex_buf[0..];
    const socket_path = try std.fmt.allocPrint(alloc, "/tmp/zmb-harness-{s}.sock", .{tag});
    errdefer alloc.free(socket_path);
    const script_path = try std.fmt.allocPrint(alloc, "/tmp/zmb-harness-{s}.json", .{tag});
    return .{ .socket_path = socket_path, .script_path = script_path };
}

fn writeFile(path: []const u8, contents: []const u8) !void {
    var file = try std.fs.cwd().createFile(path, .{ .truncate = true });
    defer file.close();
    try file.writeAll(contents);
}

fn spawnChild(
    alloc: Allocator,
    bin_path: []const u8,
    paths: PathPair,
    opts: Options,
) !std.process.Child {
    var env_map = try std.process.getEnvMap(alloc);
    errdefer env_map.deinit();
    try env_map.put("EXECUTOR_SOCKET_PATH", paths.socket_path);
    try env_map.put("EXECUTOR_HARNESS_SCRIPT", paths.script_path);
    if (opts.rpc_version) |v| {
        var ver_buf: [16]u8 = undefined;
        const s = std.fmt.bufPrint(&ver_buf, "{d}", .{v}) catch unreachable;
        try env_map.put("EXECUTOR_HARNESS_RPC_VERSION", s);
    }
    // Force a permissive network policy and skip Linux sandboxing in tests —
    // the harness path strips NullClaw entirely so no policy decision matters.
    try env_map.put("EXECUTOR_NETWORK_POLICY", "allow_all");

    const argv = [_][]const u8{bin_path};
    var child = std.process.Child.init(&argv, alloc);
    child.env_map = &env_map;
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;
    try child.spawn();
    // env_map is owned by the parent until spawn returns; the child has
    // copied/inherited the env by now. Clean up after spawn.
    env_map.deinit();
    return child;
}

fn waitForSocket(self: *Harness) !void {
    const deadline = std.time.milliTimestamp() + @as(i64, @intCast(SOCKET_POLL_BUDGET_MS));
    while (std.time.milliTimestamp() < deadline) {
        const present = blk: {
            _ = std.fs.cwd().statFile(self.socket_path) catch break :blk false;
            break :blk true;
        };
        if (present) return;
        std.Thread.sleep(SOCKET_POLL_INTERVAL_MS * std.time.ns_per_ms);
    }
    return error.HarnessSocketTimeout;
}

fn terminateChild(child: *std.process.Child) void {
    // Send SIGTERM and reap the child so its OS-level entry doesn't linger
    // as a zombie and the std.process.Child handle frees its internals.
    // Without wait(), a long test run accumulates zombies and the test
    // allocator detects leaked Child internals at suite shutdown.
    _ = child.kill() catch {};
    _ = child.wait() catch {};
}

test "harness skips when binary missing and no override" {
    const alloc = std.testing.allocator;
    // Ensure neither the canonical path nor an override resolves: clear
    // the env var and run from /tmp where the relative path won't exist.
    var env_map = try std.process.getEnvMap(alloc);
    defer env_map.deinit();
    if (env_map.get("EXECUTOR_HARNESS_BIN") != null) return error.SkipZigTest;

    var tmp_dir = std.testing.tmpDir(.{});
    defer tmp_dir.cleanup();
    var saved = try std.fs.cwd().openDir(".", .{});
    defer saved.close();
    try tmp_dir.dir.setAsCwd();
    defer saved.setAsCwd() catch {};

    const result = Harness.start(alloc, .{});
    try std.testing.expectError(error.SkipZigTest, result);
}
