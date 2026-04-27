//! Shared startup preflight helpers for serve, worker, and reconcile commands.
//! Each function logs structured output and returns errors — callers decide
//! exit policy and PostHog tracking.

const std = @import("std");
const posthog = @import("posthog");

const db = @import("../db/pool.zig");
const git_ops = @import("../git/ops.zig");
const obs_log = @import("../observability/logging.zig");
const otel_logs = @import("../observability/otel_logs.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const common = @import("common.zig");

const log = std.log.scoped(.preflight);

// ---------------------------------------------------------------------------
// PostHog client
// ---------------------------------------------------------------------------

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
        obs_log.logWarnErr(.preflight, err, "startup.posthog_init status=fail reason=analytics_disabled", .{});
        alloc.free(api_key.?);
        return .{ .client = null, .api_key_owned = null };
    };

    return .{ .client = client, .api_key_owned = api_key };
}

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
        log.info("startup.otel_logs status=ok", .{});
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
        log.info("startup.otel_traces status=ok", .{});
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
    log.info("startup.db_connect role={s} status=start", .{@tagName(role)});
    const pool = db.initFromEnvForRole(alloc, role) catch |err| {
        log.err("startup.db_connect role={s} status=fail error_code=UZ-STARTUP-003 err={s}", .{ @tagName(role), @errorName(err) });
        return err;
    };
    log.info("startup.db_connect role={s} status=ok", .{@tagName(role)});
    return pool;
}

// ---------------------------------------------------------------------------
// Migration safety
// ---------------------------------------------------------------------------

pub fn checkMigrations(pool: *db.Pool, migrate_on_start: bool) anyerror!void {
    log.info("startup.migration_check status=start", .{});
    common.enforceServeMigrationSafety(pool, migrate_on_start) catch |err| {
        switch (err) {
            common.MigrationGuardError.MigrationPending => log.err(
                "startup.migration_check status=fail error_code=UZ-STARTUP-005 err=pending_migrations hint=run zombied migrate or set MIGRATE_ON_START=1",
                .{},
            ),
            common.MigrationGuardError.MigrationFailed => log.err(
                "startup.migration_check status=fail error_code=UZ-STARTUP-005 err=migration_failure_state hint=inspect schema_migration_failures then rerun zombied migrate",
                .{},
            ),
            common.MigrationGuardError.MigrationSchemaAhead => log.err(
                "startup.migration_check status=fail error_code=UZ-STARTUP-005 err=schema_ahead hint=deploy matching binary",
                .{},
            ),
            common.MigrationGuardError.MigrationLockUnavailable => log.err(
                "startup.migration_check status=fail error_code=UZ-STARTUP-005 err=migration_lock_unavailable hint=another node is migrating",
                .{},
            ),
            else => log.err("startup.migration_check status=fail error_code=UZ-STARTUP-005 err={s}", .{@errorName(err)}),
        }
        return err;
    };
    log.info("startup.migration_check status=ok", .{});
}

pub fn parseMigrateOnStart(alloc: std.mem.Allocator) !bool {
    return common.migrateOnStartEnabledFromEnv(alloc) catch |err| {
        log.err("startup.migration_check status=fail error_code=UZ-STARTUP-005 err=invalid_MIGRATE_ON_START err_detail={s}", .{@errorName(err)});
        return err;
    };
}

// ---------------------------------------------------------------------------
// Cache root + git artifact cleanup
// ---------------------------------------------------------------------------

const CleanupStats = struct {
    removed_worktrees: u32,
    failed_worktree_removals: u32,
    pruned_bare_repos: u32,
    failed_bare_prunes: u32,
};

pub fn prepareCacheRoot(alloc: std.mem.Allocator, cache_root: []const u8, phase: []const u8) CleanupStats {
    std.fs.makeDirAbsolute(cache_root) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => obs_log.logWarnErr(.preflight, err, "startup.cache_root_create status=fail path={s}", .{cache_root}),
    };
    const stats = git_ops.cleanupRuntimeArtifacts(alloc, cache_root, "/tmp");
    log.info(
        "{s}.cleanup removed_worktrees={d} failed_worktrees={d} pruned_bare={d} failed_prunes={d}",
        .{ phase, stats.removed_worktrees, stats.failed_worktree_removals, stats.pruned_bare_repos, stats.failed_bare_prunes },
    );
    return .{
        .removed_worktrees = stats.removed_worktrees,
        .failed_worktree_removals = stats.failed_worktree_removals,
        .pruned_bare_repos = stats.pruned_bare_repos,
        .failed_bare_prunes = stats.failed_bare_prunes,
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
