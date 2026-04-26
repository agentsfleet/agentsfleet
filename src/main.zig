//! UseZombie — agent delivery control plane.
//! One Zig binary. Takes a spec. Ships a validated PR.
//!
//! Subcommands:
//!   serve      Start HTTP API server (default)
//!   worker     Start worker loop
//!   doctor     Verify Postgres, git, agent config, and critical env
//!   migrate    Apply schema migrations and exit
//!   reconcile  Dead-letter stale outbox rows (cron/scheduled)

const std = @import("std");
const builtin = @import("builtin");
const otel_logs = @import("observability/otel_logs.zig");

const cli_commands = @import("cli/commands.zig");
const cmd_serve = @import("cmd/serve.zig");
const cmd_worker = @import("cmd/worker.zig");
const cmd_doctor = @import("cmd/doctor.zig");
const cmd_migrate = @import("cmd/migrate.zig");
const cmd_reconcile = @import("cmd/reconcile.zig");
const config_load = @import("config/load.zig");

const log = std.log.scoped(.zombied);

var runtime_log_level = std.atomic.Value(u8).init(@intFromEnum(if (builtin.mode == .Debug) std.log.Level.debug else std.log.Level.info));

pub const std_options: std.Options = .{
    .log_level = .debug,
    .logFn = zombiedLog,
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

fn zombiedLog(
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
    const line = std.fmt.bufPrint(
        &line_buf,
        "ts_ms={d} level={s} scope={s} msg={f}\n",
        .{ ts, level_str, scope_str, std.json.fmt(msg, .{}) },
    ) catch return;
    const stderr = std.fs.File.stderr();
    stderr.writeAll(line) catch {};

    // Dual-write: enqueue to OTLP log exporter (no-op when not installed)
    otel_logs.enqueue(level_str, scope_str, msg);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{ .thread_safe = true }){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();
    config_load.applyEnvSources(alloc) catch |err| {
        std.debug.print("fatal: failed loading env sources: {}\n", .{err});
        std.process.exit(1);
    };
    initRuntimeLogLevel(alloc);

    if (builtin.mode == .Debug) {
        log.warn("startup.debug_build hint=not_for_production", .{});
    }

    const cmd = cli_commands.parseSubcommandFromProcessArgs() catch |err| switch (err) {
        error.UnknownSubcommand => {
            // Fail loudly so a stale script invoking a removed subcommand
            // doesn't silently start the HTTP server.
            var argv = std.process.args();
            _ = argv.next();
            const bad = argv.next() orelse "";
            std.debug.print(
                "zombied: unknown subcommand: {s}\n" ++
                    "usage: zombied [serve|worker|doctor|migrate|reconcile]\n",
                .{bad},
            );
            std.process.exit(1);
        },
    };
    switch (cmd) {
        .serve => try cmd_serve.run(alloc),
        .worker => try cmd_worker.run(alloc),
        .doctor => try cmd_doctor.run(alloc),
        .migrate => try cmd_migrate.run(alloc),
        .reconcile => try cmd_reconcile.run(alloc),
    }
}

test "parseLogLevel accepts common values" {
    try std.testing.expectEqual(@as(?std.log.Level, .debug), parseLogLevel("DEBUG"));
    try std.testing.expectEqual(@as(?std.log.Level, .info), parseLogLevel("info"));
    try std.testing.expectEqual(@as(?std.log.Level, .warn), parseLogLevel("warning"));
    try std.testing.expectEqual(@as(?std.log.Level, .err), parseLogLevel("error"));
    try std.testing.expectEqual(@as(?std.log.Level, null), parseLogLevel("trace"));
}

test {
    _ = @import("types.zig");
    _ = @import("secrets/crypto.zig");
    _ = @import("db/pool.zig");
    _ = @import("db/pg_query.zig");
    _ = @import("db/sql_splitter.zig");
    _ = @import("config/env_vars.zig");
    _ = @import("config/load.zig");
    _ = @import("config/balance_policy.zig");
    _ = @import("zombie/config.zig");
    _ = @import("zombie/yaml_frontmatter.zig");
    _ = @import("http/route_matchers.zig");
    _ = @import("zombie/activity_stream.zig");
    _ = @import("zombie/activity_cursor.zig");
    _ = @import("zombie/event_loop.zig");
    _ = @import("zombie/event_loop_secrets.zig");
    _ = @import("zombie/event_loop_secrets_test.zig");
    _ = @import("zombie/metering.zig");
    _ = @import("zombie/control_stream.zig");
    _ = @import("zombie/control_stream_test.zig");
    _ = @import("cmd/worker_watcher.zig");
    _ = @import("sys/errno.zig");
    _ = @import("sys/error.zig");
    _ = @import("util/strings/string_joiner.zig");
    _ = @import("util/strings/string_builder.zig");
    _ = @import("util/strings/smol_str.zig");
    _ = @import("hmac_sig");
    _ = @import("crypto/hmac_sig_test.zig");
    _ = @import("zombie/webhook_verify.zig");
    _ = @import("zombie/webhook_verify_test.zig");
    _ = @import("cli/commands.zig");
    _ = @import("auth/sessions.zig");
    _ = @import("auth/claims.zig");
    _ = @import("auth/jwks.zig");
    _ = @import("observability/trace.zig");
    _ = @import("observability/otel_export.zig");
    _ = @import("observability/otel_logs.zig");
    _ = @import("state/outbox_reconciler.zig");
    _ = @import("state/tenant_billing.zig");
    _ = @import("state/heroku_names.zig");
    _ = @import("state/heroku_names_test.zig");
    _ = @import("state/signup_bootstrap.zig");
    _ = @import("state/signup_bootstrap_store.zig");
    _ = @import("state/signup_bootstrap_test.zig");
    _ = @import("state/vault.zig");
    _ = @import("state/vault_test.zig");
    _ = @import("executor/types.zig");
    _ = @import("executor/protocol.zig");
    _ = @import("executor/transport.zig");
    _ = @import("executor/session.zig");
    _ = @import("executor/executor_metrics.zig");
    _ = @import("executor/landlock.zig");
    _ = @import("executor/cgroup.zig");
    _ = @import("executor/network.zig");
    _ = @import("executor/executor_network_policy.zig");
    _ = @import("executor/lease.zig");
    _ = @import("executor/client.zig");
    _ = @import("http/handlers/handler_auth_primitives_test.zig");
    _ = @import("http/handlers/byok_handlers_unit_test.zig");
    _ = @import("http/handlers/error_response_test.zig");
    _ = @import("http/handlers/hx_test.zig");
    _ = @import("http/handlers/memory/handler_test.zig");
    _ = @import("http/handlers/memory/shapes_test.zig");
    _ = @import("cmd/serve_test.zig");
    _ = @import("queue/redis.zig");
    _ = @import("queue/redis_pubsub_test.zig");
    _ = @import("reliability/backoff.zig");
    // M14_001: Persistent Zombie Memory — role isolation tests + executor unit tests
    _ = @import("memory/zombie_memory_role_test.zig");
    _ = @import("executor/zombie_memory.zig");
    // M2_001: Zombie CRUD, activity, router, worker
    _ = @import("http/handlers/zombies/api.zig");
    _ = @import("http/handlers/zombies/api_integration_test.zig");
    _ = @import("http/handlers/zombies/create.zig");
    _ = @import("http/handlers/zombies/list.zig");
    _ = @import("http/handlers/zombies/kill.zig");
    _ = @import("http/handlers/zombies/patch.zig");
    _ = @import("http/handlers/zombies/activity.zig");
    // M18_001: zombie execution telemetry
    _ = @import("state/zombie_telemetry_store.zig");
    _ = @import("http/handlers/zombies/telemetry.zig");
    _ = @import("http/handlers/zombies/telemetry_test.zig");
    _ = @import("http/handlers/zombies/telemetry_integration_test.zig");
    _ = @import("http/handlers/workspaces/dashboard_integration_test.zig");
    _ = @import("http/handlers/tenant_workspaces.zig");
    _ = @import("http/handlers/tenant_workspaces_integration_test.zig");
    _ = @import("http/router_test.zig");
    // M9_001: Integration grant + execute API
    _ = @import("http/handlers/actions/execute.zig");
    _ = @import("http/handlers/proxy/outbound.zig");
    _ = @import("http/handlers/proxy/outbound_test.zig");
    _ = @import("http/handlers/integration_grants/handler.zig");
    _ = @import("http/handlers/api_keys/agent.zig");
    _ = @import("http/handlers/api_keys/tenant.zig");
    _ = @import("http/handlers/api_keys/list.zig");
    _ = @import("http/handlers/api_keys/tenant_integration_test.zig");
    _ = @import("http/handlers/tenant_billing_integration_test.zig");
    _ = @import("http/handlers/webhooks/grant_approval.zig");
    _ = @import("http/handlers/webhooks/clerk_integration_test.zig");
    _ = @import("zombie/notifications/grant_notifier.zig");
    _ = @import("http/route_matchers.zig");
    _ = @import("http/handlers/zombies/steer.zig");
    // M23_001: Zombie Steer — live steering + execution tracking
    _ = @import("http/handlers/zombies/steer_integration_test.zig");
    _ = @import("zombie/event_loop_execution_tracking_test.zig");
    // Cross-workspace IDOR regression tests (RULE WAUTH)
    _ = @import("http/handlers/cross_workspace_idor_test.zig");
    _ = @import("cmd/worker_zombie.zig");
    _ = @import("cmd/worker/state.zig");
    // M6_001: AI Firewall Policy Engine
    _ = @import("zombie/firewall/domain_policy.zig");
    _ = @import("zombie/firewall/endpoint_policy.zig");
    _ = @import("zombie/firewall/injection_detector.zig");
    _ = @import("zombie/firewall/content_scanner.zig");
    _ = @import("zombie/firewall/firewall.zig");
    _ = @import("zombie/firewall/firewall_test.zig");
    _ = @import("zombie/firewall/firewall_robustness_test.zig");
    _ = @import("zombie/firewall/firewall_greptile_test.zig");
    // M8_001: Slack plugin
    _ = @import("state/workspace_integrations.zig");
    _ = @import("types/id_format.zig");
    _ = @import("types/id_format_test.zig");
    _ = @import("http/handlers/slack/oauth.zig");
    _ = @import("http/handlers/slack/oauth_client.zig");
    _ = @import("http/handlers/slack/events.zig");
    _ = @import("http/handlers/slack/interactions.zig");
    _ = @import("http/handlers/slack/error_code_pins_test.zig");
}
