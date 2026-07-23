//! Shared startup preflight helpers for serve and worker commands.
//! Each function logs structured output and returns errors — callers decide
//! exit policy and PostHog tracking.

const std = @import("std");
const constants = @import("common");
const posthog = @import("posthog");

const db = @import("../db/pool.zig");

const EnvMap = constants.env.Map;
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const otlp_config = @import("../observability/otlp/config.zig");
const otel_logs = @import("../observability/otel_logs.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const otel_metrics = @import("../observability/otel_metrics.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const common = @import("common.zig");
const credential_broker = @import("../credentials/broker.zig");
const credentials_integration = @import("../credentials/integration.zig");
const serve_broker = @import("../credentials/serve_broker.zig");

const log = logging.scoped(.preflight);

// ---------------------------------------------------------------------------
// PostHog client
// ---------------------------------------------------------------------------

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
const S_STARTUP_MIGRATION_CHECK_FAILED = "startup.migration_check_failed";

pub const PostHogResult = struct {
    const Self = @This();

    client: ?*posthog.PostHogClient,
    api_key_owned: ?[]const u8,

    pub fn deinit(self: Self, alloc: std.mem.Allocator) void {
        if (self.client) |c| c.deinit();
        if (self.api_key_owned) |k| alloc.free(k);
    }
};

pub fn initPostHog(env_map: *const EnvMap, alloc: std.mem.Allocator) PostHogResult {
    const api_key: ?[]const u8 = constants.env.owned(env_map, alloc, "POSTHOG_API_KEY") catch null;
    if (api_key == null) return .{ .client = null, .api_key_owned = null };

    const client = posthog.init(alloc, constants.globalIo(), .{
        .api_key = api_key.?,
        .host = "https://us.i.posthog.com",
        .flush_interval_ms = 10_000,
        .flush_at = 20,
        .max_retries = 3,
    }) catch |err| {
        log.warn("startup.posthog_init_failed", .{ .err = @errorName(err), .reason = "analytics_disabled" });
        alloc.free(api_key.?);
        return .{ .client = null, .api_key_owned = null };
    };

    return .{ .client = client, .api_key_owned = api_key };
}

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
const TelemetryResult = struct {
    const Self = @This();

    telemetry: telemetry_mod.Telemetry,
    ph: PostHogResult,

    pub fn deinit(self: Self, alloc: std.mem.Allocator) void {
        self.ph.deinit(alloc);
    }

    pub fn ptr(self: *Self) *telemetry_mod.Telemetry {
        return &self.telemetry;
    }
};

pub fn initTelemetry(env_map: *const EnvMap, alloc: std.mem.Allocator) TelemetryResult {
    const ph = initPostHog(env_map, alloc);
    return .{ .telemetry = telemetry_mod.Telemetry.initProd(ph.client), .ph = ph };
}

// ---------------------------------------------------------------------------
// OTLP log exporter
// ---------------------------------------------------------------------------

/// Why an exporter is dark after a failed install: the flush thread never
/// started, so the signal is silently dropped for the process lifetime. Shared
/// by all three exporters (RULE UFS).
const R_FLUSH_SPAWN_FAILED = "flush_thread_spawn_failed";

pub fn initOtelLogs(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) void {
    if (otlp_config.configFromEnv(env_map, alloc)) |cfg| {
        switch (otel_logs.install(io, cfg)) {
            .installed => log.info("startup.otel_logs_ok", .{}),
            .already_running => log.info("startup.otel_logs_already_running", .{}),
            .spawn_failed => log.warn("startup.otel_logs_failed", .{ .reason = R_FLUSH_SPAWN_FAILED }),
        }
    }
}

pub fn deinitOtelLogs() void {
    if (otel_logs.isInstalled()) {
        otel_logs.uninstall();
    }
}

// ---------------------------------------------------------------------------
// OTLP trace exporter
// ---------------------------------------------------------------------------

pub fn initOtelTraces(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) void {
    if (otlp_config.configFromEnv(env_map, alloc)) |cfg| {
        switch (otel_traces.install(io, cfg)) {
            .installed => log.info("startup.otel_traces_ok", .{}),
            .already_running => log.info("startup.otel_traces_already_running", .{}),
            .spawn_failed => log.warn("startup.otel_traces_failed", .{ .reason = R_FLUSH_SPAWN_FAILED }),
        }
    }
}

pub fn deinitOtelTraces() void {
    if (otel_traces.isInstalled()) {
        otel_traces.uninstall();
    }
}

// ---------------------------------------------------------------------------
// OTLP metric exporter
// ---------------------------------------------------------------------------

pub fn initOtelMetrics(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) void {
    if (otlp_config.configFromEnv(env_map, alloc)) |cfg| {
        switch (otel_metrics.install(io, cfg)) {
            .installed => log.info("startup.otel_metrics_ok", .{}),
            .already_running => log.info("startup.otel_metrics_already_running", .{}),
            .spawn_failed => log.warn("startup.otel_metrics_failed", .{ .reason = R_FLUSH_SPAWN_FAILED }),
        }
    } else {
        // Self-serve signal: the disabled reason lives in the startup log, not a
        // ticket — same shared GRAFANA_OTLP_* gate as traces/logs.
        log.info("startup.otel_metrics_disabled", .{ .reason = "no GRAFANA_OTLP_ENDPOINT" });
    }
}

pub fn deinitOtelMetrics() void {
    if (otel_metrics.isInstalled()) {
        otel_metrics.uninstall();
    }
}

/// Install all three OTLP exporters (logs/traces/metrics) under the shared
/// GRAFANA_OTLP_* gate. Pair with `deinitOtelExporters` via `defer`.
pub fn initOtelExporters(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) void {
    initOtelLogs(io, env_map, alloc);
    initOtelTraces(io, env_map, alloc);
    initOtelMetrics(io, env_map, alloc);
}

/// Uninstall all three OTLP exporters (reverse order).
pub fn deinitOtelExporters() void {
    deinitOtelMetrics();
    deinitOtelTraces();
    deinitOtelLogs();
}

// ---------------------------------------------------------------------------
// Database pool
// ---------------------------------------------------------------------------

pub fn connectDbPool(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, role: db.DbRole) !*db.Pool {
    log.info("startup.db_connect_start", .{ .role = @tagName(role) });
    const pool = db.initFromEnvForRole(io, env_map, alloc, role) catch |err| {
        log.err("startup.db_connect_failed", .{
            .role = @tagName(role),
            .error_code = error_codes.ERR_STARTUP_DB_CONNECT,
            .err = @errorName(err),
        });
        return err;
    };
    log.info("startup.db_connect_ok", .{ .role = @tagName(role) });
    return pool;
}

// ---------------------------------------------------------------------------
// Migration safety
// ---------------------------------------------------------------------------

pub fn checkMigrations(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, pool: *db.Pool, migrate_on_start: bool) anyerror!void {
    log.info("startup.migration_check_start", .{});
    common.enforceServeMigrationSafety(io, env_map, alloc, pool, migrate_on_start) catch |err| {
        const mc_code = error_codes.ERR_STARTUP_MIGRATION_CHECK;
        switch (err) {
            common.MigrationGuardError.MigrationPending => log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
                .error_code = mc_code,
                .reason = "pending_migrations",
                .hint = "run agentsfleetd migrate or set MIGRATE_ON_START=1",
            }),
            common.MigrationGuardError.MigrationFailed => log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
                .error_code = mc_code,
                .reason = "migration_failure_state",
                .hint = "inspect schema_migration_failures then rerun agentsfleetd migrate",
            }),
            common.MigrationGuardError.MigrationSchemaAhead => log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
                .error_code = mc_code,
                .reason = "schema_ahead",
                .hint = "deploy matching binary",
            }),
            common.MigrationGuardError.MigrationLockUnavailable => log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
                .error_code = mc_code,
                .reason = "migration_lock_unavailable",
                .hint = "another node is migrating",
            }),
            else => log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
                .error_code = mc_code,
                .err = @errorName(err),
            }),
        }
        return err;
    };
    log.info("startup.migration_check_ok", .{});
}

pub fn parseMigrateOnStart(env_map: *const EnvMap, alloc: std.mem.Allocator) !bool {
    return common.migrateOnStartEnabledFromEnv(env_map, alloc) catch |err| {
        log.err(S_STARTUP_MIGRATION_CHECK_FAILED, .{
            .error_code = error_codes.ERR_STARTUP_MIGRATION_CHECK,
            .reason = "invalid_MIGRATE_ON_START",
            .err = @errorName(err),
        });
        return err;
    };
}

// ---------------------------------------------------------------------------
// Signal handlers
// ---------------------------------------------------------------------------

pub fn installSignalHandlers(handler: *const fn (std.posix.SIG) callconv(.c) void) void {
    const action = std.posix.Sigaction{
        .handler = .{ .handler = handler },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &action, null);
    std.posix.sigaction(std.posix.SIG.TERM, &action, null);
}

// ---------------------------------------------------------------------------
// On-demand credential broker (M102 §3)
// ---------------------------------------------------------------------------

/// Structured log message for a credential-broker boot failure (RULE UFS — one
/// spelling across the three alloc/init guard sites).
const S_CREDENTIAL_BROKER_INIT_FAILED: []const u8 = "startup.credential_broker_init_failed";

/// Owns the heap-allocated credential-broker singleton + its HTTP boundary + the
/// duped platform key. `deinit` tears them down at shutdown; all fields optional
/// so a partially-built (or failed) install still cleans up exactly what it set.
pub const CredentialBrokerHandle = struct {
    alloc: std.mem.Allocator,
    broker: ?*credential_broker = null,
    exchange: ?*serve_broker.HttpClientExchange = null,
    github_app: ?credentials_integration.GithubApp = null,
    zoho_app: ?credentials_integration.OauthApp = null,
    jira_app: ?credentials_integration.OauthApp = null,
    linear_app: ?credentials_integration.OauthApp = null,

    pub fn deinit(self: *CredentialBrokerHandle) void {
        if (self.broker) |b| {
            b.deinit();
            self.alloc.destroy(b);
        }
        if (self.exchange) |e| self.alloc.destroy(e);
        if (self.github_app) |a| {
            self.alloc.free(a.app_id);
            self.alloc.free(a.private_key_pem);
            if (a.app_slug) |s| self.alloc.free(s);
        }
        freeOauthApp(self.alloc, self.zoho_app);
        freeOauthApp(self.alloc, self.jira_app);
        freeOauthApp(self.alloc, self.linear_app);
        self.* = undefined;
    }
};

fn freeOauthApp(alloc: std.mem.Allocator, app: ?credentials_integration.OauthApp) void {
    if (app) |a| {
        alloc.free(a.client_id);
        alloc.free(a.client_secret);
    }
}

/// Boot the on-demand credential broker singleton and publish it on `broker_out`.
/// serve.zig stays integration-agnostic (RULE CFG): WHICH integrations carry a
/// platform key + how to load them lives in `serve_broker.buildDeps`, never here
/// or in the boot path. The broker + its HTTP boundary are heap-owned so the
/// published pointer is stable for the process. Degrades closed: an alloc/init
/// failure leaves `broker_out` untouched (the mint endpoint 503s — never a crash),
/// and the returned handle still frees whatever was built.
pub fn installCredentialBroker(
    alloc: std.mem.Allocator,
    io: std.Io,
    pool: *db.Pool,
    admin_ws_id: []const u8,
    broker_out: *?*credential_broker,
    slug_out: *?[]const u8,
) CredentialBrokerHandle {
    var handle = CredentialBrokerHandle{ .alloc = alloc };
    const exchange = alloc.create(serve_broker.HttpClientExchange) catch {
        log.warn(S_CREDENTIAL_BROKER_INIT_FAILED, .{ .error_code = error_codes.ERR_STARTUP_ENV_ALLOC, .err = "exchange_alloc" });
        return handle;
    };
    exchange.* = .{ .io = io };
    handle.exchange = exchange;

    const built = serve_broker.buildDeps(alloc, pool, exchange, admin_ws_id);
    handle.github_app = built.github_app;
    handle.zoho_app = built.zoho_app;
    handle.jira_app = built.jira_app;
    handle.linear_app = built.linear_app;
    // M102 §5 — the connect install URL's App slug rides the GithubApp (one github
    // carrier, not a second handle field). Non-secret; null degrades connect closed.
    slug_out.* = if (built.github_app) |a| a.app_slug else null;

    const broker = alloc.create(credential_broker) catch {
        log.warn(S_CREDENTIAL_BROKER_INIT_FAILED, .{ .error_code = error_codes.ERR_STARTUP_ENV_ALLOC, .err = "broker_alloc" });
        return handle;
    };
    broker.* = credential_broker.init(alloc, credentials_integration.REGISTRY, built.deps) catch |err| {
        alloc.destroy(broker);
        log.warn(S_CREDENTIAL_BROKER_INIT_FAILED, .{ .error_code = error_codes.ERR_STARTUP_ENV_ALLOC, .err = @errorName(err) });
        return handle;
    };
    handle.broker = broker;
    broker_out.* = broker;
    return handle;
}

// Tests in preflight_test.zig
comptime {
    _ = @import("preflight_test.zig");
}
