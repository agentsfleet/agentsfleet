//! Shared startup preflight helpers for serve and worker commands.
//! Each function logs structured output and returns errors — callers decide
//! exit policy and PostHog tracking.

const std = @import("std");
const posthog = @import("posthog");

const db = @import("../db/pool.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const otel_logs = @import("../observability/otel_logs.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const common = @import("common.zig");

const log = logging.scoped(.preflight);

// ---------------------------------------------------------------------------
// PostHog client
// ---------------------------------------------------------------------------

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const PostHogResult = struct {
    client: ?*posthog.PostHogClient,
    api_key_owned: ?[]const u8,

    pub fn deinit(self: PostHogResult, alloc: std.mem.Allocator) void {
        if (self.client) |c| c.deinit();
        if (self.api_key_owned) |k| alloc.free(k);
    }
};

pub fn initPostHog(alloc: std.mem.Allocator) PostHogResult {
    const api_key = std.process.getEnvVarOwned(alloc, "POSTHOG_API_KEY") catch null;
    if (api_key == null) return .{ .client = null, .api_key_owned = null };

    const client = posthog.init(alloc, .{
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
    telemetry: telemetry_mod.Telemetry,
    ph: PostHogResult,

    pub fn deinit(self: TelemetryResult, alloc: std.mem.Allocator) void {
        self.ph.deinit(alloc);
    }

    pub fn ptr(self: *TelemetryResult) *telemetry_mod.Telemetry {
        return &self.telemetry;
    }
};

pub fn initTelemetry(alloc: std.mem.Allocator) TelemetryResult {
    const ph = initPostHog(alloc);
    return .{ .telemetry = telemetry_mod.Telemetry.initProd(ph.client), .ph = ph };
}

// ---------------------------------------------------------------------------
// OTLP log exporter
// ---------------------------------------------------------------------------

pub fn initOtelLogs(alloc: std.mem.Allocator) void {
    if (otel_logs.configFromEnv(alloc)) |cfg| {
        otel_logs.install(cfg);
        log.info("startup.otel_logs_ok", .{});
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

pub fn initOtelTraces(alloc: std.mem.Allocator) void {
    if (otel_logs.configFromEnv(alloc)) |cfg| {
        otel_traces.install(cfg);
        log.info("startup.otel_traces_ok", .{});
    }
}

pub fn deinitOtelTraces() void {
    if (otel_traces.isInstalled()) {
        otel_traces.uninstall();
    }
}

// ---------------------------------------------------------------------------
// Database pool
// ---------------------------------------------------------------------------

pub fn connectDbPool(alloc: std.mem.Allocator, role: db.DbRole) !*db.Pool {
    log.info("startup.db_connect_start", .{ .role = @tagName(role) });
    const pool = db.initFromEnvForRole(alloc, role) catch |err| {
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

pub fn checkMigrations(pool: *db.Pool, migrate_on_start: bool) anyerror!void {
    log.info("startup.migration_check_start", .{});
    common.enforceServeMigrationSafety(pool, migrate_on_start) catch |err| {
        const mc_code = error_codes.ERR_STARTUP_MIGRATION_CHECK;
        switch (err) {
            common.MigrationGuardError.MigrationPending => log.err("startup.migration_check_failed", .{
                .error_code = mc_code,
                .reason = "pending_migrations",
                .hint = "run zombied migrate or set MIGRATE_ON_START=1",
            }),
            common.MigrationGuardError.MigrationFailed => log.err("startup.migration_check_failed", .{
                .error_code = mc_code,
                .reason = "migration_failure_state",
                .hint = "inspect schema_migration_failures then rerun zombied migrate",
            }),
            common.MigrationGuardError.MigrationSchemaAhead => log.err("startup.migration_check_failed", .{
                .error_code = mc_code,
                .reason = "schema_ahead",
                .hint = "deploy matching binary",
            }),
            common.MigrationGuardError.MigrationLockUnavailable => log.err("startup.migration_check_failed", .{
                .error_code = mc_code,
                .reason = "migration_lock_unavailable",
                .hint = "another node is migrating",
            }),
            else => log.err("startup.migration_check_failed", .{
                .error_code = mc_code,
                .err = @errorName(err),
            }),
        }
        return err;
    };
    log.info("startup.migration_check_ok", .{});
}

pub fn parseMigrateOnStart(alloc: std.mem.Allocator) !bool {
    return common.migrateOnStartEnabledFromEnv(alloc) catch |err| {
        log.err("startup.migration_check_failed", .{
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

pub fn installSignalHandlers(handler: *const fn (i32) callconv(.c) void) void {
    const action = std.posix.Sigaction{
        .handler = .{ .handler = handler },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &action, null);
    std.posix.sigaction(std.posix.SIG.TERM, &action, null);
}

// Tests in preflight_test.zig
comptime {
    _ = @import("preflight_test.zig");
}
