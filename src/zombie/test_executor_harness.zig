// Test fixture: spawns an executor child process and exposes a connected
// ExecutorClient for worker-side integration tests. Targets either
// `zombied-executor-harness` (default — comptime-gated to runner_harness.zig
// for scripted frame emission) or `zombied-executor-stub` (production
// NullClaw pipeline with the LLM provider swapped for a canned-response
// stub — used by redaction-harness tests that need the real observer +
// redactor adapter chain to fire).
//
// Usage (harness target, default):
//   var harness = try Harness.start(alloc, .{ .script_json = my_script });
// Usage (stub target):
//   var harness = try Harness.start(alloc, .{ .binary = .stub });
//
// `script_json` is only honoured for the harness target; the stub binary
// has no scripted-frame mechanism and ignores it.

const std = @import("std");
const builtin = @import("builtin");
const Allocator = std.mem.Allocator;

const executor_client = @import("../executor/client.zig");
const logging = @import("log");

const log = logging.scoped(.test_executor_harness);

const Harness = @This();

pub const BinaryTarget = enum {
    /// `zombied-executor-harness` — runner_harness.zig path, scripted frames.
    harness,
    /// `zombied-executor-stub` — production NullClaw pipeline with stub provider.
    stub,
};

const SOCKET_POLL_INTERVAL_MS: u64 = 25;
const SOCKET_POLL_BUDGET_MS: u64 = 3_000;

alloc: Allocator,
child: std.process.Child,
socket_path: []u8,
script_path: ?[]u8,
executor: executor_client.ExecutorClient,
connected: bool,

pub const Options = struct {
    /// Which executor binary to spawn. Default is the scripted harness;
    /// redaction-harness tests use `.stub`.
    binary: BinaryTarget = .harness,
    /// Pre-stringified harness script JSON. See runner_harness.zig docs for
    /// shape. Pass an empty slice for an empty-script run (no frames, exit_ok).
    /// Ignored when `binary == .stub`.
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

fn defaultBinPath(target: BinaryTarget) []const u8 {
    return switch (target) {
        .harness => "zig-out/bin/zombied-executor-harness",
        .stub => "zig-out/bin/zombied-executor-stub",
    };
}

fn binEnvVar(target: BinaryTarget) []const u8 {
    return switch (target) {
        .harness => "EXECUTOR_HARNESS_BIN",
        .stub => "EXECUTOR_STUB_BIN",
    };
}

pub fn start(alloc: Allocator, opts: Options) !Harness {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const bin_path = try resolveBinaryPath(alloc, opts.binary);
    defer alloc.free(bin_path);

    const paths = try uniquePaths(alloc, opts.binary);
    errdefer alloc.free(paths.socket_path);
    errdefer if (paths.script_path) |p| alloc.free(p);

    if (paths.script_path) |p| try writeFile(p, opts.script_json);
    errdefer if (paths.script_path) |p| std.fs.deleteFileAbsolute(p) catch {};

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
    if (self.script_path) |p| std.fs.deleteFileAbsolute(p) catch {};
    self.alloc.free(self.socket_path);
    if (self.script_path) |p| self.alloc.free(p);
}

/// Convenience accessor: pointer to the connected `ExecutorClient`. Worker
/// callers stash this in `EventLoopConfig.executor` for `writepath.run`.
pub fn executorPtr(self: *Harness) *executor_client.ExecutorClient {
    return &self.executor;
}

// ── Internals ───────────────────────────────────────────────────────────────

const PathPair = struct { socket_path: []u8, script_path: ?[]u8 };

fn resolveBinaryPath(alloc: Allocator, target: BinaryTarget) ![]u8 {
    const env_var = binEnvVar(target);
    const default_path = defaultBinPath(target);
    if (std.process.getEnvVarOwned(alloc, env_var)) |override| {
        const exists = blk: {
            _ = std.fs.cwd().statFile(override) catch break :blk false;
            break :blk true;
        };
        if (exists) return override;
        log.warn("bin_override_missing", .{ .path = override });
        alloc.free(override);
    } else |_| {}

    _ = std.fs.cwd().statFile(default_path) catch {
        log.warn("binary_not_found", .{ .path = default_path, .hint = "run zig build to install" });
        return error.SkipZigTest;
    };
    return alloc.dupe(u8, default_path);
}

fn uniquePaths(alloc: Allocator, target: BinaryTarget) !PathPair {
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
    const script_path: ?[]u8 = switch (target) {
        .harness => try std.fmt.allocPrint(alloc, "/tmp/zmb-harness-{s}.json", .{tag}),
        .stub => null,
    };
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
    if (paths.script_path) |p| try env_map.put("EXECUTOR_HARNESS_SCRIPT", p);
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
