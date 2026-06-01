//! usezombie — agent delivery control plane.
//! One Zig binary. Takes a spec. Ships a validated PR.
//!
//! Subcommands:
//!   serve      Start HTTP API server (default)
//!   doctor     Verify Postgres, git, agent config, and critical env
//!   migrate    Apply schema migrations and exit

const std = @import("std");
const builtin = @import("builtin");
const logging = @import("log");
const log_sinks = logging.sinks;
const otel_logs = @import("observability/otel_logs.zig");

const cli_commands = @import("cli/commands.zig");
const cmd_serve = @import("cmd/serve.zig");
const cmd_doctor = @import("cmd/doctor.zig");
const cmd_migrate = @import("cmd/migrate.zig");
const config_load = @import("config/load.zig");

const log = logging.scoped(.zombied);

const S_INFO = "info";
const S_DEBUG = "debug";
const S_ERR = "err";
const S_WARN = "warn";

var runtime_log_level = std.atomic.Value(u8).init(@intFromEnum(if (builtin.mode == .Debug) std.log.Level.debug else std.log.Level.info));

// pub: consumed by std at comptime via @import("root").std_options
pub const std_options: std.Options = .{
    .log_level = .debug,
    .logFn = zombiedLog,
};

fn parseLogLevel(level_raw: []const u8) ?std.log.Level {
    if (std.ascii.eqlIgnoreCase(level_raw, S_DEBUG)) return .debug;
    if (std.ascii.eqlIgnoreCase(level_raw, S_INFO)) return .info;
    if (std.ascii.eqlIgnoreCase(level_raw, S_WARN) or std.ascii.eqlIgnoreCase(level_raw, "warning")) return .warn;
    if (std.ascii.eqlIgnoreCase(level_raw, S_ERR) or std.ascii.eqlIgnoreCase(level_raw, "error")) return .err;
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

    const scope_str = comptime if (scope == .default) "default" else @tagName(scope);
    const ts = std.time.milliTimestamp();
    var msg_buf: [4096]u8 = undefined;
    const msg = std.fmt.bufPrint(&msg_buf, fmt, args) catch return;

    // Pre-init fallback: until `main()` registers the production sinks
    // (stderr + OTLP), route directly to stderr so the very-early
    // emits during `applyEnvSources` and the GPA bootstrap remain
    // observable. Once sinks are installed, the registry path takes
    // over and this branch never fires again.
    if (!log_sinks.sinksRegistered()) {
        writePreInitStderr(level, scope_str, ts, msg);
        return;
    }

    log_sinks.emitToSinks(level, scope_str, ts, msg);
}

fn writePreInitStderr(level: std.log.Level, scope_str: []const u8, ts: i64, msg: []const u8) void {
    const level_str = switch (level) {
        .err => S_ERR,
        .warn => S_WARN,
        .info => S_INFO,
        .debug => S_DEBUG,
    };
    var line_buf: [8192]u8 = undefined;
    const line = if (logging.isPretty())
        logging.formatPretty(&line_buf, ts, level, scope_str, msg)
    else
        logging.writeLogfmtEnvelope(&line_buf, ts, level_str, scope_str, msg);
    const stderr = std.fs.File.stderr();
    stderr.writeAll(line) catch {};
}

fn stderrSinkEmit(
    ctx: *anyopaque,
    level: std.log.Level,
    scope: []const u8,
    ts_ms: i64,
    body: []const u8,
) void {
    _ = ctx;
    writePreInitStderr(level, scope, ts_ms, body);
}

fn otlpSinkEmit(
    ctx: *anyopaque,
    level: std.log.Level,
    scope: []const u8,
    ts_ms: i64,
    body: []const u8,
) void {
    _ = ctx;
    _ = ts_ms;
    const level_str = switch (level) {
        .err => S_ERR,
        .warn => S_WARN,
        .info => S_INFO,
        .debug => S_DEBUG,
    };
    // `msg` already arrives as a properly-quoted logfmt body
    // (`event=<n> key=value …`) from `logging.scoped(.x).<level>`.
    // OTLP receives the body verbatim — exporter splices envelope keys.
    otel_logs.enqueue(level_str, scope, body);
}

fn registerProductionSinks() void {
    log_sinks.registerSink(.{ .emit = stderrSinkEmit, .ctx = log_sinks.statelessCtx() });
    log_sinks.registerSink(.{ .emit = otlpSinkEmit, .ctx = log_sinks.statelessCtx() });
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{ .thread_safe = true }){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();
    config_load.applyEnvSources(alloc) catch |err| {
        logging.fatalStderr("fatal: failed loading env sources: {}\n", .{err});
        std.process.exit(1);
    };
    initRuntimeLogLevel(alloc);
    logging.initPrettyMode(alloc);
    // Production log sinks (stderr + OTLP). Until this call, zombiedLog
    // falls through to direct stderr write so the env-load logs above
    // still reach the operator. After this, every emit fans out through
    // the registry — same observable behavior, plus a hook for tests.
    registerProductionSinks();

    if (builtin.mode == .Debug) {
        log.warn("startup.debug_build hint=not_for_production", .{});
    }

    const argv = try std.process.argsAlloc(alloc);
    defer std.process.argsFree(alloc, argv);
    const cmd = cli_commands.parseSubcommandFromArgv(argv) catch |err| switch (err) {
        error.UnknownSubcommand => {
            // Fail loudly so a stale script invoking a removed subcommand
            // doesn't silently start the HTTP server.
            const bad = if (argv.len > 1) argv[1] else "";
            logging.fatalStderr(
                "zombied: unknown subcommand: {s}\n" ++
                    "usage: zombied [serve|doctor|migrate]\n",
                .{bad},
            );
            std.process.exit(1);
        },
    };
    switch (cmd) {
        .serve => try cmd_serve.run(alloc),
        .doctor => try cmd_doctor.run(alloc),
        .migrate => try cmd_migrate.run(alloc),
    }
}

test "parseLogLevel accepts common values" {
    try std.testing.expectEqual(@as(?std.log.Level, .debug), parseLogLevel("DEBUG"));
    try std.testing.expectEqual(@as(?std.log.Level, .info), parseLogLevel("info"));
    try std.testing.expectEqual(@as(?std.log.Level, .warn), parseLogLevel("warning"));
    try std.testing.expectEqual(@as(?std.log.Level, .err), parseLogLevel("error"));
    try std.testing.expectEqual(@as(?std.log.Level, null), parseLogLevel("trace"));
}
