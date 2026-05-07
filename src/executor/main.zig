//! Sandbox executor sidecar entry point.
//!
//! This is a separate binary (`zombied-executor`) that serves the executor
//! API over a Unix socket. It embeds NullClaw and owns the dynamic stage
//! execution path, including host-level Linux sandboxing (bubblewrap +
//! Landlock + cgroups v2).
//!
//! The worker no longer assumes dangerous agent execution lives in its
//! own process boundary.

const std = @import("std");
const logging = @import("log");
const builtin = @import("builtin");
const build_options = @import("build_options");
const transport = @import("transport.zig");
const handler_mod = @import("handler.zig");
const Session = @import("session.zig");
const SessionStore = @import("runtime/session_store.zig");
const lease_mod = @import("lease.zig");
const types = @import("types.zig");
const landlock = @import("landlock.zig");
const cgroup = @import("cgroup.zig");
const network = @import("network.zig");

const log = logging.scoped(.sandbox_executor);

var runtime_log_level = std.atomic.Value(u8).init(@intFromEnum(if (builtin.mode == .Debug) std.log.Level.debug else std.log.Level.info));

// pub: consumed by std at comptime via @import("root").std_options.
// Mirrors src/main.zig's zombiedLog so the executor binary's logs are
// logfmt-shaped (`ts_ms=… level=… scope=… event=… key=value …`) and
// parseable by the same aggregator pipeline as the worker. Without
// this, std falls back to defaultLog (`<level>(<scope>): msg`) and
// `error_code=UZ-…` alerts silently drop executor lines.
//
// No OTLP dual-write here — the executor sidecar is local-only; the
// worker's logFn is the single OTEL emitter.
pub const std_options: std.Options = .{
    .log_level = .debug,
    .logFn = executorLog,
};

fn parseLogLevel(level_raw: []const u8) ?std.log.Level {
    if (std.ascii.eqlIgnoreCase(level_raw, "debug")) return .debug;
    if (std.ascii.eqlIgnoreCase(level_raw, "info")) return .info;
    if (std.ascii.eqlIgnoreCase(level_raw, "warn") or std.ascii.eqlIgnoreCase(level_raw, "warning")) return .warn;
    if (std.ascii.eqlIgnoreCase(level_raw, "err") or std.ascii.eqlIgnoreCase(level_raw, "error")) return .err;
    return null;
}

fn initRuntimeLogLevel(alloc: std.mem.Allocator) void {
    const level_raw = std.process.getEnvVarOwned(alloc, "LOG_LEVEL") catch return;
    defer alloc.free(level_raw);
    if (parseLogLevel(level_raw)) |lvl| {
        runtime_log_level.store(@intFromEnum(lvl), .release);
    }
}

fn shouldLog(level: std.log.Level) bool {
    const configured: std.log.Level = @enumFromInt(runtime_log_level.load(.acquire));
    return @intFromEnum(level) <= @intFromEnum(configured);
}

fn executorLog(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime fmt: []const u8,
    args: anytype,
) void {
    if (!shouldLog(level)) return;
    const level_str = comptime switch (level) {
        .err => "err",
        .warn => "warn",
        .info => "info",
        .debug => "debug",
    };
    const scope_str = comptime if (scope == .default) "default" else @tagName(scope);
    const ts = std.time.milliTimestamp();
    var msg_buf: [4096]u8 = undefined;
    const msg = std.fmt.bufPrint(&msg_buf, fmt, args) catch return;
    var line_buf: [8192]u8 = undefined;
    const line = if (logging.isPretty())
        logging.formatPretty(&line_buf, ts, level, scope_str, msg)
    else
        // `msg` already arrives as a properly-quoted logfmt body
        // (`event=<n> key=value …`) from `logging.scoped(.x).<level>`.
        // Splice it as top-level keys instead of re-encoding inside
        // `msg="…"` — matches LOGGING_STANDARD §3 line 41 and keeps
        // Loki/LogQL queries one-pass over the record.
        std.fmt.bufPrint(
            &line_buf,
            "ts_ms={d} level={s} scope={s} {s}\n",
            .{ ts, level_str, scope_str, msg },
        ) catch return;
    const stderr = std.fs.File.stderr();
    stderr.writeAll(line) catch {};
}

const DEFAULT_SOCKET_PATH = "/run/zombie/executor.sock";
const DEFAULT_LEASE_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MEMORY_LIMIT_MB: u64 = 512;
const DEFAULT_CPU_LIMIT_PERCENT: u64 = 100;

/// Test-only env var read in harness builds; lets integration tests
/// drive the rpc_version_mismatch fast-fail path without producing a
/// second binary. Stripped from production via the comptime branch.
const HARNESS_RPC_VERSION_ENV_VAR = "EXECUTOR_HARNESS_RPC_VERSION";

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{ .thread_safe = true }){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    initRuntimeLogLevel(alloc);
    logging.initPrettyMode(alloc);

    // getEnvVarOwned returns a fresh allocation; the default branch borrows a
    // const literal. Track ownership so the env-set path doesn't leak at
    // shutdown when the GPA tallies live allocations.
    const socket_path_owned: ?[]u8 = std.process.getEnvVarOwned(alloc, "EXECUTOR_SOCKET_PATH") catch null;
    defer if (socket_path_owned) |p| alloc.free(p);
    const socket_path: []const u8 = socket_path_owned orelse DEFAULT_SOCKET_PATH;
    const lease_timeout_ms = parseU64Env(alloc, "EXECUTOR_LEASE_TIMEOUT_MS", DEFAULT_LEASE_TIMEOUT_MS);
    const memory_limit_mb = parseU64Env(alloc, "EXECUTOR_MEMORY_LIMIT_MB", DEFAULT_MEMORY_LIMIT_MB);
    const cpu_limit_percent = parseU64Env(alloc, "EXECUTOR_CPU_LIMIT_PERCENT", DEFAULT_CPU_LIMIT_PERCENT);
    const net_policy = network.policyFromEnv(alloc);

    if (build_options.executor_harness) {
        const v_u64 = parseU64Env(alloc, HARNESS_RPC_VERSION_ENV_VAR, 0);
        if (v_u64 > 0 and v_u64 <= std.math.maxInt(u32)) {
            const v: u32 = @intCast(v_u64);
            transport.setHelloVersionOverride(v);
            log.warn("rpc_version_override", .{ .version = v, .path = "test_harness" });
        }
    }

    log.info("executor_started", .{ .socket = socket_path, .lease_timeout_ms = lease_timeout_ms, .network_policy = @tagName(net_policy) });

    // Report host backend capabilities.
    if (builtin.os.tag == .linux) {
        log.info("capabilities_detected", .{ .landlock = landlock.isAvailable(), .cgroups_v2 = cgroup.isAvailable() });
    } else {
        log.warn("host_backend_degraded", .{ .host = "non_linux" });
    }

    var store = SessionStore.init(alloc);
    defer store.deinit();

    var lease_manager = lease_mod.LeaseManager.init(&store);
    const lease_thread = try std.Thread.spawn(.{}, lease_mod.LeaseManager.run, .{&lease_manager});

    var rpc_handler = handler_mod.Handler.init(alloc, &store, lease_timeout_ms, .{
        .memory_limit_mb = memory_limit_mb,
        .cpu_limit_percent = cpu_limit_percent,
    }, net_policy);

    // Wrap the handler method for the transport layer. The conn_fd
    // arrives non-null for live socket frames so StartStage can stream
    // progress notifications back; tests bypass the transport entirely
    // and call `handleFrame` directly with the 2-arg form.
    const frame_handler = struct {
        var g_handler: *handler_mod.Handler = undefined;
        fn handle(a: std.mem.Allocator, payload: []const u8, conn_fd: ?std.posix.socket_t) anyerror![]u8 {
            return g_handler.handleFrameWithFd(a, payload, conn_fd);
        }
    };
    frame_handler.g_handler = &rpc_handler;

    var server = transport.Server.init(alloc, socket_path, frame_handler.handle);
    server.bind() catch |err| {
        log.err("bind_failed", .{ .path = socket_path, .err = @errorName(err) });
        std.process.exit(1);
    };
    defer server.stop();

    // Install signal handler for graceful shutdown.
    const posix_handler = struct {
        var g_server: *transport.Server = undefined;
        var g_lease: *lease_mod.LeaseManager = undefined;

        fn onSignal(sig: i32) callconv(.c) void {
            _ = sig;
            g_server.stop();
            g_lease.stop();
        }
    };
    posix_handler.g_server = &server;
    posix_handler.g_lease = &lease_manager;

    if (builtin.os.tag != .windows) {
        const act = std.posix.Sigaction{
            .handler = .{ .handler = posix_handler.onSignal },
            .mask = std.posix.sigemptyset(),
            .flags = 0,
        };
        std.posix.sigaction(std.posix.SIG.TERM, &act, null);
        std.posix.sigaction(std.posix.SIG.INT, &act, null);
    }

    log.info("serving", .{ .socket = socket_path });
    server.serve();

    lease_manager.stop();
    lease_thread.join();
    log.info("shutdown", .{});
}

fn parseU64Env(alloc: std.mem.Allocator, name: []const u8, default: u64) u64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default;
    defer alloc.free(raw);
    return std.fmt.parseInt(u64, std.mem.trim(u8, raw, " \t\r\n"), 10) catch default;
}

// Pull in tests from all executor modules.
test {
    _ = @import("types.zig");
    _ = @import("context_budget.zig");
    _ = @import("protocol.zig");
    _ = @import("transport.zig");
    _ = @import("session.zig");
    _ = @import("session_test.zig");
    _ = @import("runtime/session_store.zig");
    _ = @import("runtime/secret_substitution.zig");
    _ = @import("runtime/policy_http_request.zig");
    _ = @import("runtime/policy_http_request_test.zig");
    _ = @import("runner_progress.zig");
    _ = @import("runner_progress_test.zig");
    _ = @import("runner_progress_redact_test.zig");
    _ = @import("handler.zig");
    _ = @import("handler_create_execution_test.zig");
    _ = @import("lease.zig");
    _ = @import("landlock.zig");
    _ = @import("cgroup.zig");
    _ = @import("network.zig");
    _ = @import("executor_network_policy.zig");
    _ = @import("executor_metrics.zig");
    _ = @import("json_helpers.zig");
    _ = @import("client.zig");
    _ = @import("runner.zig");
    _ = @import("integration_test.zig");
    _ = @import("crash_test.zig");
    _ = @import("runner_test.zig");
    _ = @import("sandbox_edge_test.zig");
    _ = @import("executor_limits_test.zig");
    _ = @import("crash_concurrent_test.zig");
    _ = @import("crash_recovery_test.zig");
    _ = @import("handler_edge_test.zig");
    _ = @import("handler_negative_test.zig");
    _ = @import("runner_security_test.zig");
    _ = @import("client_test.zig");
    _ = @import("client_credentials_test.zig");
    _ = @import("resource_metrics_test.zig");
    _ = @import("resource_security_test.zig");
}
