//! agentsfleet — agent delivery control plane.
//! One Zig binary. Takes a spec. Ships a validated Pull Request (PR).
//!
//! Subcommands:
//!   serve      Start HTTP API server (default)
//!   doctor     Verify Postgres, git, agent config, and critical env
//!   migrate    Apply schema migrations and exit

const std = @import("std");
const clock = @import("common").clock;
const builtin = @import("builtin");
const logging = @import("log");
const log_sinks = logging.sinks;
const otel_logs = @import("observability/otel_logs.zig");
const otlp_config = @import("observability/otlp/config.zig");

const cli_commands = @import("cli/commands.zig");
const cmd_serve = @import("cmd/serve.zig");
const cmd_doctor = @import("cmd/doctor.zig");
const cmd_migrate = @import("cmd/migrate.zig");
const config_load = @import("config/load.zig");

const log = logging.scoped(.agentsfleetd);

const S_INFO = "info";
const S_DEBUG = "debug";
const S_ERR = "err";
const S_WARN = "warn";
const S_SCOPE_OTEL_LOGS = "otel_logs";
const S_SCOPE_OTEL_METRICS = "otel_metrics";
const S_SCOPE_OTEL_TRACES = "otel_traces";

var runtime_log_level = std.atomic.Value(u8).init(@intFromEnum(if (builtin.mode == .Debug) std.log.Level.debug else std.log.Level.info));

// pub: consumed by std at comptime via @import("root").std_options
pub const std_options: std.Options = .{
    .log_level = .debug,
    .logFn = agentsfleetdLog,
};

fn parseLogLevel(level_raw: []const u8) ?std.log.Level {
    if (std.ascii.eqlIgnoreCase(level_raw, S_DEBUG)) return .debug;
    if (std.ascii.eqlIgnoreCase(level_raw, S_INFO)) return .info;
    if (std.ascii.eqlIgnoreCase(level_raw, S_WARN) or std.ascii.eqlIgnoreCase(level_raw, "warning")) return .warn;
    if (std.ascii.eqlIgnoreCase(level_raw, S_ERR) or std.ascii.eqlIgnoreCase(level_raw, "error")) return .err;
    return null;
}

fn initRuntimeLogLevel(env_map: *const std.process.Environ.Map) void {
    const level_raw = env_map.get("LOG_LEVEL") orelse return;
    if (parseLogLevel(level_raw)) |lvl| {
        runtime_log_level.store(@intFromEnum(lvl), .release);
    }
}

fn shouldLog(level: std.log.Level) bool {
    const configured: std.log.Level = @enumFromInt(runtime_log_level.load(.acquire));
    return @intFromEnum(level) <= @intFromEnum(configured);
}

fn agentsfleetdLog(
    comptime level: std.log.Level,
    comptime scope: @TypeOf(.enum_literal),
    comptime fmt: []const u8,
    args: anytype,
) void {
    if (!shouldLog(level)) return;

    const scope_str = comptime if (scope == .default) "default" else @tagName(scope);
    const ts = clock.nowMillis();
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
    logging.writeStderrLine(line);
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
    if (!shouldExportLogScope(scope)) return;
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

fn shouldExportLogScope(scope: []const u8) bool {
    return !std.mem.eql(u8, scope, S_SCOPE_OTEL_LOGS) and
        !std.mem.eql(u8, scope, S_SCOPE_OTEL_METRICS) and
        !std.mem.eql(u8, scope, S_SCOPE_OTEL_TRACES);
}

fn registerProductionSinks() void {
    log_sinks.registerSink(.{ .emit = stderrSinkEmit, .ctx = log_sinks.statelessCtx() });
    log_sinks.registerSink(.{ .emit = otlpSinkEmit, .ctx = log_sinks.statelessCtx() });
}

/// Process exit code when the daemon's DebugAllocator reports a leak at teardown.
/// Debug builds only — see `leak_guard`; a release build runs the `smp_allocator`
/// (no leak accounting), so this abort cannot fire there. Distinct from the generic
/// `exit(1)` startup failures so an operator or CI run can tell a leak-abort from a
/// config error. 70 == BSD sysexits `EX_SOFTWARE`.
const EXIT_GPA_LEAK: u8 = 70;

/// Allocator selected by build mode (Invariant 5, mirrors `src/runner/main.zig`).
/// Debug keeps the leak-checking DebugAllocator; a release build uses the fast
/// thread-safe `smp_allocator` so the long-running daemon never pays the
/// DebugAllocator's per-allocation bookkeeping + locking cost in production. Leak
/// detection lives in the memleak gates (testing-allocator test graphs + the
/// boot→drain lifecycle test); the Prometheus endpoint exposes current resident
/// memory as the production signal. Pure so the choice is unit-testable.
const AllocatorKind = enum { debug_leak_checked, release_smp };

fn allocatorKind(mode: std.builtin.OptimizeMode) AllocatorKind {
    return if (mode == .Debug) .debug_leak_checked else .release_smp;
}

test "release builds select the smp allocator; Debug the leak-checked one (Invariant 5)" {
    try std.testing.expectEqual(AllocatorKind.debug_leak_checked, allocatorKind(.Debug));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseSafe));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseFast));
    try std.testing.expectEqual(AllocatorKind.release_smp, allocatorKind(.ReleaseSmall));
}

pub fn main(init: std.process.Init) void {
    const io = init.io;
    const env_map = init.environ_map;

    var gpa = std.heap.DebugAllocator(.{ .thread_safe = true }){};
    // First defer → runs LAST (LIFO), after every `defer` below has freed into
    // `alloc`. In Debug a leak at shutdown aborts loudly (nonzero exit + logged
    // verdict). In release `alloc` is `smp_allocator`, so nothing was allocated
    // through `gpa`; its `deinit` is a no-op that also keeps the binding live
    // (the `comptime` branch elides the Debug arm entirely in release builds).
    defer if (comptime allocatorKind(builtin.mode) == .debug_leak_checked) {
        logging.leak_guard.check(gpa.deinit(), "daemon") catch std.process.exit(EXIT_GPA_LEAK);
    } else {
        _ = gpa.deinit();
    };
    const alloc = switch (allocatorKind(builtin.mode)) {
        .debug_leak_checked => gpa.allocator(),
        .release_smp => std.heap.smp_allocator,
    };
    var dotenv_overlay = config_load.applyEnvSources(io, env_map, alloc) catch |err| {
        logging.fatalStderr("fatal: failed loading env sources: {}\n", .{err});
        std.process.exit(1);
    };
    defer if (dotenv_overlay) |*m| m.deinit();
    // In dev, `.env.local` overlays the process env; everything downstream reads
    // the merged map. In prod (no overlay) this is the process env_map as-is.
    const eff_env: *const std.process.Environ.Map = if (dotenv_overlay) |*m| m else env_map;
    initRuntimeLogLevel(eff_env);
    // Borrowed read — initPrettyMode consumes the value during the call only.
    // Zig 0.16 removed std.posix.isatty; probe the TTY via the threaded io.
    const stderr_is_tty = std.Io.File.stderr().isTty(io) catch false;
    logging.initPrettyMode(eff_env.get("AGENTSFLEET_LOG_PRETTY"), stderr_is_tty);
    // Production log sinks (stderr + OpenTelemetry Protocol (OTLP)). Until this
    // call, agentsfleetdLog
    // falls through to direct stderr write so the env-load logs above
    // still reach the operator. After this, every emit fans out through
    // the registry — same observable behavior, plus a hook for tests.
    registerProductionSinks();

    if (builtin.mode == .Debug) {
        log.warn("startup.debug_build hint=not_for_production", .{});
    }

    // Zig 0.16 removed std.process.argsAlloc/std.os.argv; args (and io + the
    // environment block) arrive through std.process.Init.
    const argv = init.minimal.args.toSlice(init.arena.allocator()) catch |err| {
        logging.fatalStderr("agentsfleetd: failed reading argv: {}\n", .{err});
        std.process.exit(1);
    };
    const cmd = cli_commands.parseSubcommandFromArgv(argv) catch |err| switch (err) {
        error.UnknownSubcommand => {
            // Fail loudly so a stale script invoking a removed subcommand
            // doesn't silently start the HTTP server.
            const bad = if (argv.len > 1) argv[1] else "";
            logging.fatalStderr(
                "agentsfleetd: unknown subcommand: {s}\n" ++
                    "usage: agentsfleetd [serve|doctor|migrate]\n",
                .{bad},
            );
            std.process.exit(1);
        },
    };
    (switch (cmd) {
        .serve => cmd_serve.run(io, eff_env, argv, alloc),
        .doctor => cmd_doctor.run(io, eff_env, argv, alloc),
        .migrate => cmd_migrate.run(io, eff_env, alloc),
    }) catch |err| {
        logging.fatalStderr("agentsfleetd: subcommand failed: {}\n", .{err});
        std.process.exit(1);
    };
}

test "parseLogLevel accepts common values" {
    try std.testing.expectEqual(@as(?std.log.Level, .debug), parseLogLevel("DEBUG"));
    try std.testing.expectEqual(@as(?std.log.Level, .info), parseLogLevel("info"));
    try std.testing.expectEqual(@as(?std.log.Level, .warn), parseLogLevel("warning"));
    try std.testing.expectEqual(@as(?std.log.Level, .err), parseLogLevel("error"));
    try std.testing.expectEqual(@as(?std.log.Level, null), parseLogLevel("trace"));
}

test "test_exporter_failure_scope_bypasses_otlp_log_sink" {
    const cfg: otlp_config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:0",
        .instance_id = "i",
        .api_key = "k",
    };
    otel_logs.testSetInstalled(cfg);
    defer otel_logs.testClear();

    otlpSinkEmit(log_sinks.statelessCtx(), .warn, S_SCOPE_OTEL_LOGS, 0, "failed");
    otlpSinkEmit(log_sinks.statelessCtx(), .warn, S_SCOPE_OTEL_METRICS, 0, "failed");
    otlpSinkEmit(log_sinks.statelessCtx(), .warn, S_SCOPE_OTEL_TRACES, 0, "failed");
    try std.testing.expectEqual(@as(usize, 0), otel_logs.testPendingCount());

    otlpSinkEmit(log_sinks.statelessCtx(), .warn, "http", 0, "request_failed");
    try std.testing.expectEqual(@as(usize, 1), otel_logs.testPendingCount());
}
