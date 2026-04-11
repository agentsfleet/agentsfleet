const std = @import("std");
const posthog = @import("posthog");

const db = @import("../db/pool.zig");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const oidc_auth = @import("../auth/oidc.zig");
const http_server = @import("../http/server.zig");
const http_handler = @import("../http/handler.zig");
const auth_sessions = @import("../auth/sessions.zig");
const queue_redis = @import("../queue/redis.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const preflight = @import("preflight.zig");
const common = @import("common.zig");
const error_codes = @import("../errors/codes.zig");

const log = std.log.scoped(.zombied);

var shutdown_requested = std.atomic.Value(bool).init(false);
var stop_server_fn: *const fn () void = http_server.stop;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

const ServeArgError = error{
    InvalidServeArgument,
    MissingPortValue,
    InvalidPortValue,
};

fn parseServeArgOverrides() ServeArgError!?u16 {
    var it = std.process.args();
    _ = it.next(); // binary name
    _ = it.next(); // subcommand
    return parseServeArgs(&it);
}

fn parseServeArgs(it: anytype) ServeArgError!?u16 {
    var override_port: ?u16 = null;
    while (it.next()) |arg| {
        if (std.mem.eql(u8, arg, "--port")) {
            const port_raw = it.next() orelse return ServeArgError.MissingPortValue;
            override_port = parsePortValue(port_raw) orelse return ServeArgError.InvalidPortValue;
            continue;
        }
        if (std.mem.startsWith(u8, arg, "--port=")) {
            const port_raw = arg["--port=".len..];
            override_port = parsePortValue(port_raw) orelse return ServeArgError.InvalidPortValue;
            continue;
        }
        return ServeArgError.InvalidServeArgument;
    }
    return override_port;
}

fn parsePortValue(raw: []const u8) ?u16 {
    const parsed = std.fmt.parseInt(u16, raw, 10) catch return null;
    if (parsed == 0) return null;
    return parsed;
}

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher() void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    stop_server_fn();
}

pub fn run(alloc: std.mem.Allocator) !void {
    preflight.initOtelLogs(alloc);
    defer preflight.deinitOtelLogs();
    preflight.initOtelTraces(alloc);
    defer preflight.deinitOtelTraces();
    log.info("startup.serve status=start", .{});

    const serve_port_override = parseServeArgOverrides() catch |err| {
        switch (err) {
            ServeArgError.InvalidServeArgument => log.err("startup.args_parse status=fail reason=invalid_argument", .{}),
            ServeArgError.MissingPortValue => log.err("startup.args_parse status=fail reason=missing_port_value", .{}),
            ServeArgError.InvalidPortValue => log.err("startup.args_parse status=fail reason=invalid_port_value", .{}),
        }
        std.process.exit(2);
    };

    log.info("startup.env_check status=start", .{});
    env_vars.enforceFromEnv(alloc) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_API and DATABASE_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API and REDIS_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API must use rediss://", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER must use rediss://", .{}),
        }
        std.process.exit(1);
    };
    log.info("startup.env_check status=ok", .{});

    log.info("startup.config_load status=start", .{});
    var serve_cfg = runtime_config.ServeConfig.load(alloc) catch |err| {
        switch (err) {
            runtime_config.ValidationError.MissingApiKey,
            runtime_config.ValidationError.InvalidApiKeyList,
            runtime_config.ValidationError.MissingOidcJwksUrl,
            runtime_config.ValidationError.InvalidOidcProvider,
            runtime_config.ValidationError.MissingEncryptionMasterKey,
            runtime_config.ValidationError.InvalidEncryptionMasterKey,
            runtime_config.ValidationError.InvalidPort,
            runtime_config.ValidationError.InvalidApiHttpThreads,
            runtime_config.ValidationError.InvalidApiHttpWorkers,
            runtime_config.ValidationError.InvalidApiMaxClients,
            runtime_config.ValidationError.InvalidApiMaxInFlightRequests,
            runtime_config.ValidationError.InvalidReadyMaxQueueDepth,
            runtime_config.ValidationError.InvalidReadyMaxQueueAgeMs,
            runtime_config.ValidationError.InvalidKekVersion,
            runtime_config.ValidationError.MissingEncryptionMasterKeyV2,
            runtime_config.ValidationError.InvalidEncryptionMasterKeyV2,
            => {
                runtime_config.ServeConfig.printValidationError(@errorCast(err));
                log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)});
            },
            else => log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    log.info("startup.config_load status=ok", .{});

    const api_pool = preflight.connectDbPool(alloc, .api) catch std.process.exit(1);
    defer api_pool.deinit();

    log.info("startup.redis_connect role=api status=start", .{});
    var api_queue = queue_redis.Client.connectFromEnv(alloc, .api) catch |err| {
        log.err("startup.redis_connect role=api status=fail error_code=" ++ error_codes.ERR_STARTUP_REDIS_CONNECT ++ " err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer api_queue.deinit();
    api_queue.ensureConsumerGroup() catch |err| {
        log.err("startup.redis_group role=api status=fail error_code=" ++ error_codes.ERR_STARTUP_REDIS_GROUP ++ " err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("startup.redis_connect role=api status=ok", .{});

    const migrate_on_start = preflight.parseMigrateOnStart(alloc) catch std.process.exit(1);
    preflight.checkMigrations(api_pool, migrate_on_start) catch std.process.exit(1);

    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "startup");

    var sessions = auth_sessions.SessionStore.init(alloc);
    defer sessions.deinit();

    var ctx = http_handler.Context{
        .pool = api_pool,
        .queue = &api_queue,
        .alloc = alloc,
        .api_keys = serve_cfg.api_keys,
        .oidc = null,
        .auth_sessions = &sessions,
        .app_url = serve_cfg.app_url,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = serve_cfg.api_max_in_flight_requests,
        .ready_max_queue_depth = serve_cfg.ready_max_queue_depth,
        .ready_max_queue_age_ms = serve_cfg.ready_max_queue_age_ms,
        .posthog = null,
    };
    metrics.setApiInFlightRequests(0);

    const ph = preflight.initPostHog(alloc);
    defer ph.deinit(alloc);
    ctx.posthog = ph.client;

    if (serve_cfg.oidc_enabled) {
        log.info("startup.oidc_init status=start provider={s}", .{@tagName(serve_cfg.oidc_provider)});
    }
    var oidc = if (serve_cfg.oidc_enabled) oidc_auth.Verifier.init(alloc, .{
        .provider = serve_cfg.oidc_provider,
        .jwks_url = serve_cfg.oidc_jwks_url orelse "",
        .issuer = serve_cfg.oidc_issuer,
        .audience = serve_cfg.oidc_audience,
    }) else null;
    defer if (oidc) |*v| v.deinit();
    if (oidc) |*v| {
        ctx.oidc = v;
        log.info("startup.oidc_init status=ok", .{});
    }

    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    var signal_thread: ?std.Thread = null;
    var event_thread: ?std.Thread = null;
    errdefer {
        shutdown_requested.store(true, .release);
        event_bus.stop();
        if (signal_thread) |*t| t.join();
        if (event_thread) |*t| t.join();
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("http.server_starting port={d} api_threads={d} api_workers={d} api_max_clients={d} api_max_in_flight={d}", .{
        serve_cfg.port,
        serve_cfg.api_http_threads,
        serve_cfg.api_http_workers,
        serve_cfg.api_max_clients,
        serve_cfg.api_max_in_flight_requests,
    });
    posthog_events.trackServerStarted(ph.client, serve_cfg.port, 0);
    http_server.serve(&ctx, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        obs_log.logErr(.zombied, err, "http.server_exit status=fail", .{});
    };

    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();
    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "shutdown");
}

fn testStopServerHook() void {
    if (stop_server_test_counter) |counter| {
        _ = counter.fetchAdd(1, .acq_rel);
    }
}

test "integration: signalWatcher stops server on shutdown" {
    shutdown_requested.store(false, .release);

    var stop_calls = std.atomic.Value(u32).init(0);
    stop_server_test_counter = &stop_calls;
    stop_server_fn = testStopServerHook;
    defer {
        stop_server_fn = http_server.stop;
        stop_server_test_counter = null;
        shutdown_requested.store(false, .release);
    }

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    std.Thread.sleep(15 * std.time.ns_per_ms);
    shutdown_requested.store(true, .release);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}

// --- T1: Happy path — parseServeArgs with valid --port ---

const TestArgIterator = struct {
    args: []const []const u8,
    index: usize = 0,

    fn next(self: *TestArgIterator) ?[]const u8 {
        if (self.index >= self.args.len) return null;
        const arg = self.args[self.index];
        self.index += 1;
        return arg;
    }
};

test "parseServeArgs returns null when no args" {
    var it = TestArgIterator{ .args = &.{} };
    const result = try parseServeArgs(&it);
    try std.testing.expect(result == null);
}

test "parseServeArgs parses --port with space" {
    var it = TestArgIterator{ .args = &.{ "--port", "8080" } };
    const result = try parseServeArgs(&it);
    try std.testing.expectEqual(@as(?u16, 8080), result);
}

test "parseServeArgs parses --port= form" {
    var it = TestArgIterator{ .args = &.{"--port=3001"} };
    const result = try parseServeArgs(&it);
    try std.testing.expectEqual(@as(?u16, 3001), result);
}

// --- T2: Edge cases ---

test "parseServeArgs rejects port 0" {
    var it = TestArgIterator{ .args = &.{ "--port", "0" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, parseServeArgs(&it));
}

test "parseServeArgs rejects port=0 form" {
    var it = TestArgIterator{ .args = &.{"--port=0"} };
    try std.testing.expectError(ServeArgError.InvalidPortValue, parseServeArgs(&it));
}

test "parseServeArgs rejects non-numeric port" {
    var it = TestArgIterator{ .args = &.{ "--port", "abc" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, parseServeArgs(&it));
}

test "parseServeArgs rejects overflow port" {
    var it = TestArgIterator{ .args = &.{ "--port", "99999" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, parseServeArgs(&it));
}

test "parseServeArgs rejects negative port" {
    var it = TestArgIterator{ .args = &.{ "--port", "-1" } };
    try std.testing.expectError(ServeArgError.InvalidPortValue, parseServeArgs(&it));
}

// --- T3: Error paths ---

test "parseServeArgs returns error for missing port value" {
    var it = TestArgIterator{ .args = &.{"--port"} };
    try std.testing.expectError(ServeArgError.MissingPortValue, parseServeArgs(&it));
}

test "parseServeArgs returns error for unknown arg" {
    var it = TestArgIterator{ .args = &.{"--verbose"} };
    try std.testing.expectError(ServeArgError.InvalidServeArgument, parseServeArgs(&it));
}

test "parseServeArgs returns error for unknown arg after valid port" {
    var it = TestArgIterator{ .args = &.{ "--port", "3000", "--extra" } };
    try std.testing.expectError(ServeArgError.InvalidServeArgument, parseServeArgs(&it));
}

test "parsePortValue parses valid ports" {
    try std.testing.expectEqual(@as(?u16, 3000), parsePortValue("3000"));
    try std.testing.expectEqual(@as(?u16, 1), parsePortValue("1"));
    try std.testing.expectEqual(@as(?u16, 65535), parsePortValue("65535"));
}

test "parsePortValue rejects invalid values" {
    try std.testing.expect(parsePortValue("0") == null);
    try std.testing.expect(parsePortValue("abc") == null);
    try std.testing.expect(parsePortValue("99999") == null);
    try std.testing.expect(parsePortValue("") == null);
    try std.testing.expect(parsePortValue("-5") == null);
}
