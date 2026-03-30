//! Sandbox executor sidecar entry point.
//!
//! This is a separate binary (`zombied-executor`) that serves the executor
//! API over a Unix socket. It embeds NullClaw and owns the dynamic stage
//! execution path, including host-level Linux sandboxing (bubblewrap +
//! Landlock + cgroups v2).
//!
//! The worker no longer assumes dangerous agent execution lives in its
//! own process boundary (§3.3).

const std = @import("std");
const builtin = @import("builtin");
const transport = @import("transport.zig");
const handler_mod = @import("handler.zig");
const session_mod = @import("session.zig");
const lease_mod = @import("lease.zig");
const types = @import("types.zig");
const landlock = @import("landlock.zig");
const cgroup = @import("cgroup.zig");
const network = @import("network.zig");

const log = std.log.scoped(.sandbox_executor);

const DEFAULT_SOCKET_PATH = "/run/zombie/executor.sock";
const DEFAULT_LEASE_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MEMORY_LIMIT_MB: u64 = 512;
const DEFAULT_CPU_LIMIT_PERCENT: u64 = 100;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{ .thread_safe = true }){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const socket_path = envOrDefault(alloc, "EXECUTOR_SOCKET_PATH", DEFAULT_SOCKET_PATH);
    const lease_timeout_ms = parseU64Env(alloc, "EXECUTOR_LEASE_TIMEOUT_MS", DEFAULT_LEASE_TIMEOUT_MS);
    const memory_limit_mb = parseU64Env(alloc, "EXECUTOR_MEMORY_LIMIT_MB", DEFAULT_MEMORY_LIMIT_MB);
    const cpu_limit_percent = parseU64Env(alloc, "EXECUTOR_CPU_LIMIT_PERCENT", DEFAULT_CPU_LIMIT_PERCENT);
    const net_policy = network.policyFromEnv(alloc);

    log.info("startup.executor socket={s} lease_timeout_ms={d} network_policy={s}", .{
        socket_path,
        lease_timeout_ms,
        @tagName(net_policy),
    });

    // Report host backend capabilities.
    if (builtin.os.tag == .linux) {
        log.info("startup.capabilities landlock={} cgroups_v2={}", .{
            landlock.isAvailable(),
            cgroup.isAvailable(),
        });
    } else {
        log.warn("startup.non_linux host_backend=degraded", .{});
    }

    var store = session_mod.SessionStore.init(alloc);
    defer store.deinit();

    var lease_manager = lease_mod.LeaseManager.init(&store);
    const lease_thread = try std.Thread.spawn(.{}, lease_mod.LeaseManager.run, .{&lease_manager});

    var rpc_handler = handler_mod.Handler.init(alloc, &store, lease_timeout_ms, .{
        .memory_limit_mb = memory_limit_mb,
        .cpu_limit_percent = cpu_limit_percent,
    }, net_policy);

    // Wrap the handler method for the transport layer.
    const frame_handler = struct {
        var g_handler: *handler_mod.Handler = undefined;
        fn handle(a: std.mem.Allocator, payload: []const u8) anyerror![]u8 {
            return g_handler.handleFrame(a, payload);
        }
    };
    frame_handler.g_handler = &rpc_handler;

    var server = transport.Server.init(alloc, socket_path, frame_handler.handle);
    server.bind() catch |err| {
        log.err("startup.bind_failed path={s} err={s}", .{ socket_path, @errorName(err) });
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

    log.info("executor.serving socket={s}", .{socket_path});
    server.serve();

    lease_manager.stop();
    lease_thread.join();
    log.info("executor.shutdown", .{});
}

fn envOrDefault(alloc: std.mem.Allocator, name: []const u8, default: []const u8) []const u8 {
    return std.process.getEnvVarOwned(alloc, name) catch default;
}

fn parseU64Env(alloc: std.mem.Allocator, name: []const u8, default: u64) u64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default;
    defer alloc.free(raw);
    return std.fmt.parseInt(u64, std.mem.trim(u8, raw, " \t\r\n"), 10) catch default;
}

// Pull in tests from all executor modules.
test {
    _ = @import("types.zig");
    _ = @import("protocol.zig");
    _ = @import("transport.zig");
    _ = @import("session.zig");
    _ = @import("handler.zig");
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
    _ = @import("client_credentials_test.zig");
}
