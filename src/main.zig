//! UseZombie — agent delivery control plane.
//! One Zig binary. Takes a spec. Ships a validated PR.
//!
//! Subcommands:
//!   serve    Start HTTP API server (default)
//!   worker   Start worker loop
//!   doctor   Verify Postgres, git, agent config, and critical env
//!   run      One-shot: process a spec file without HTTP server
//!   migrate    Apply schema migrations and exit
//!   reconcile  Dead-letter stale outbox rows (cron/scheduled)

const std = @import("std");
const builtin = @import("builtin");

const cli_commands = @import("cli/commands.zig");
const cmd_serve = @import("cmd/serve.zig");
const cmd_worker = @import("cmd/worker.zig");
const cmd_doctor = @import("cmd/doctor.zig");
const cmd_run = @import("cmd/run.zig");
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

    const cmd = cli_commands.parseSubcommandFromProcessArgs();
    switch (cmd) {
        .serve => try cmd_serve.run(alloc),
        .worker => try cmd_worker.run(alloc),
        .doctor => try cmd_doctor.run(alloc),
        .run => try cmd_run.run(alloc),
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
    _ = @import("state/machine.zig");
    _ = @import("secrets/crypto.zig");
    _ = @import("db/pool.zig");
    _ = @import("config/env_vars.zig");
    _ = @import("config/load.zig");
    _ = @import("harness/control_plane.zig");
    _ = @import("cli/commands.zig");
    _ = @import("auth/sessions.zig");
    _ = @import("auth/claims.zig");
    _ = @import("auth/jwks.zig");
    _ = @import("observability/trace.zig");
    _ = @import("observability/otel_export.zig");
    _ = @import("observability/langfuse.zig");
    _ = @import("state/outbox_reconciler.zig");
    _ = @import("state/workspace_billing.zig");
}
