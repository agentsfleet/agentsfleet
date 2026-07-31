const std = @import("std");
const constants = @import("common");

const db = @import("../db/pool.zig");
const oidc_auth = @import("../auth/oidc.zig");
const env_vars = @import("../config/env_vars.zig");
const validate = @import("../config/runtime_validate.zig");
const queue_redis = @import("../queue/redis.zig");
const redis_config = @import("../queue/redis_config.zig");
const common = @import("common.zig");
const doctor_args = @import("doctor_args.zig");
const doctor_render = @import("doctor_render.zig");
const logging = @import("log");

const log = logging.scoped(.agentsfleetd);

const EnvMap = constants.env.Map;
const DoctorArgError = doctor_args.DoctorArgError;
const CheckResult = doctor_render.CheckResult;
const appendCheck = doctor_render.appendCheck;
const appendFmtCheck = doctor_render.appendFmtCheck;
const renderText = doctor_render.renderText;
const renderJson = doctor_render.renderJson;
const parseDoctorArgs = doctor_args.parseDoctorArgs;

const S_DOCTOR_DB_CONNECT_START = "doctor.db_connect_start";
const S_DOCTOR_REDIS_CONNECT_START = "doctor.redis_connect_start";
const S_DOCTOR_DB_CONNECT_OK = "doctor.db_connect_ok";
const S_OIDC_PROVIDER = "oidc_provider";
const S_API = "api";
const S_ENCRYPTION_MASTER_KEY = "encryption_master_key";
const S_AUTH_SESSION_CODE_PEPPER = "auth_session_code_pepper";
const S_AUDIT_LOG_PEPPER = "audit_log_pepper";
const S_DOCTOR_SCHEMA_GATE_FAILED = "doctor.schema_gate_failed";
const S_SCHEMA_GATE_COMPAT = "schema_gate_compat";
const S_DB_API_CONFIG = "db_api_config";
const S_DOCTOR_REDIS_CONNECT_FAILED = "doctor.redis_connect_failed";
const S_OIDC_JWKS_REACHABILITY = "oidc_jwks_reachability";
const S_T_R_N = " \t\r\n";
const S_DOCTOR_REDIS_CONNECT_OK = "doctor.redis_connect_ok";
const S_DOCTOR_DB_CONNECT_FAILED = "doctor.db_connect_failed";

const MigrationSchemaGateError = error{
    FailedMigrations,
    SchemaAhead,
    PendingMigrations,
};

fn schemaGateReasonCode(err: ?MigrationSchemaGateError) []const u8 {
    if (err) |e| {
        return switch (e) {
            MigrationSchemaGateError.FailedMigrations => "SCHEMA_FAILED_MIGRATIONS",
            MigrationSchemaGateError.SchemaAhead => "SCHEMA_AHEAD_OF_BINARY",
            MigrationSchemaGateError.PendingMigrations => "SCHEMA_BEHIND_BINARY",
        };
    }
    return "SCHEMA_COMPATIBLE";
}

fn ensureSchemaCompatible(state: db.MigrationState) MigrationSchemaGateError!void {
    if (state.has_failed_migrations) return MigrationSchemaGateError.FailedMigrations;
    if (state.has_newer_schema_version) return MigrationSchemaGateError.SchemaAhead;
    if (state.applied_versions < state.expected_versions) return MigrationSchemaGateError.PendingMigrations;
}

pub fn run(io: std.Io, env_map: *const EnvMap, argv: []const [:0]const u8, alloc: std.mem.Allocator) !void {
    log.info("doctor.start", .{});
    var ok = true;
    var stdout_buf: [8192]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &stdout_buf);
    const stdout = &stdout_w.interface;
    var results: std.ArrayList(CheckResult) = .empty;
    // Details are owned copies; both renderers borrow them, so the free runs last.
    defer doctor_render.freeResults(alloc, &results);

    const options = try parseOptionsOrExit(alloc, argv, stdout);

    var role_urls = try env_vars.loadFromEnv(env_map, alloc);
    defer role_urls.deinit();
    const redis_api_url = role_urls.redis_api;

    try checkRoleEnv(ctx(io, env_map, alloc, &results, &ok), role_urls);
    try checkDbApi(ctx(io, env_map, alloc, &results, &ok));
    if (options.schema_gate) try checkSchemaGate(ctx(io, env_map, alloc, &results, &ok));
    try checkRedisApi(ctx(io, env_map, alloc, &results, &ok), redis_api_url);
    try checkSecretKey(ctx(io, env_map, alloc, &results, &ok), "ENCRYPTION_MASTER_KEY", S_ENCRYPTION_MASTER_KEY);
    try checkSecretKey(ctx(io, env_map, alloc, &results, &ok), "AUTH_SESSION_CODE_PEPPER", S_AUTH_SESSION_CODE_PEPPER);
    try checkSecretKey(ctx(io, env_map, alloc, &results, &ok), "AUDIT_LOG_PEPPER", S_AUDIT_LOG_PEPPER);
    try checkOidc(ctx(io, env_map, alloc, &results, &ok));

    switch (options.format) {
        .text => try renderText(stdout, results.items, ok),
        .json => try renderJson(stdout, results.items, ok),
    }
    try stdout.flush();
    if (ok) {
        log.info("doctor.finish_ok", .{});
    } else {
        log.err("doctor.finish_failed", .{});
    }
    if (!ok) std.process.exit(1);
}

fn parseOptionsOrExit(alloc: std.mem.Allocator, argv: []const [:0]const u8, stdout: anytype) !doctor_args.DoctorOptions {
    // argv[0]=binary, argv[1]=subcommand; the rest are doctor flags.
    var extra_args: std.ArrayList([]const u8) = .empty;
    defer extra_args.deinit(alloc);
    if (argv.len > 2) for (argv[2..]) |arg| {
        try extra_args.append(alloc, arg);
    };
    return parseDoctorArgs(extra_args.items) catch |err| {
        switch (err) {
            DoctorArgError.InvalidDoctorArgument => try stdout.print("fatal: invalid doctor argument\n", .{}),
            DoctorArgError.MissingFormatValue => try stdout.print("fatal: --format requires a value (text|json)\n", .{}),
            DoctorArgError.InvalidFormatValue => try stdout.print("fatal: invalid --format value (use text|json)\n", .{}),
        }
        try stdout.flush();
        std.process.exit(2);
    };
}

/// Shared per-check context — one bundle instead of five parameters per check.
const CheckCtx = struct {
    io: std.Io,
    env_map: *const EnvMap,
    alloc: std.mem.Allocator,
    results: *std.ArrayList(CheckResult),
    ok: *bool,
};

fn ctx(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator, results: *std.ArrayList(CheckResult), ok: *bool) CheckCtx {
    return .{ .io = io, .env_map = env_map, .alloc = alloc, .results = results, .ok = ok };
}

fn checkRoleEnv(c: CheckCtx, role_urls: env_vars.EnvVars) !void {
    env_vars.validateLoaded(role_urls) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => try appendCheck(c.alloc, c.results, "role_env_required", false, "DATABASE_URL_API required", c.ok),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => try appendCheck(c.alloc, c.results, "role_env_redis_required", false, "REDIS_URL_API required", c.ok),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => try appendCheck(c.alloc, c.results, "redis_api_tls", false, "REDIS_URL_API must use rediss://", c.ok),
        }
        return;
    };
    try appendCheck(c.alloc, c.results, "env_vars_contract", true, "API DB/Redis URLs configured with Redis TLS", c.ok);
}

fn checkDbApi(c: CheckCtx) !void {
    log.info(S_DOCTOR_DB_CONNECT_START, .{ .role = S_API });
    const pool = db.initFromEnvForRole(c.io, c.env_map, c.alloc, .api) catch |err| {
        log.err(S_DOCTOR_DB_CONNECT_FAILED, .{ .role = S_API, .err = @errorName(err) });
        try appendCheck(c.alloc, c.results, S_DB_API_CONFIG, false, "DATABASE_URL_API not set/invalid", c.ok);
        return;
    };
    pool.deinit();
    log.info(S_DOCTOR_DB_CONNECT_OK, .{ .role = S_API });
    try appendCheck(c.alloc, c.results, S_DB_API_CONFIG, true, "API database config", c.ok);
}

fn checkSchemaGate(c: CheckCtx) !void {
    log.info("doctor.schema_gate_start", .{});
    const pool = db.initFromEnvForRole(c.io, c.env_map, c.alloc, .migrator) catch |err| {
        log.err(S_DOCTOR_SCHEMA_GATE_FAILED, .{ .stage = "connect", .err = @errorName(err) });
        try appendCheck(c.alloc, c.results, "schema_gate_config", false, "DATABASE_URL_MIGRATOR not set/invalid", c.ok);
        return;
    };
    defer pool.deinit();

    const migrations = common.canonicalMigrations();
    const state = db.inspectMigrationState(pool, &migrations) catch |err| {
        log.err(S_DOCTOR_SCHEMA_GATE_FAILED, .{ .stage = "inspect", .err = @errorName(err) });
        try appendCheck(c.alloc, c.results, "schema_gate_state", false, "Unable to inspect migration state", c.ok);
        return;
    };

    ensureSchemaCompatible(state) catch |err| {
        const reason = schemaGateReasonCode(err);
        try appendFmtCheck(
            c.alloc,
            c.results,
            S_SCHEMA_GATE_COMPAT,
            false,
            c.ok,
            "schema_gate status=fail expected_versions={d} applied_versions={d} reason_code={s}",
            .{ state.expected_versions, state.applied_versions, reason },
        );
        return;
    };

    log.info("doctor.schema_gate_ok", .{ .expected = state.expected_versions, .applied = state.applied_versions });
    try appendFmtCheck(
        c.alloc,
        c.results,
        S_SCHEMA_GATE_COMPAT,
        true,
        c.ok,
        "schema_gate status=ok expected_versions={d} applied_versions={d} reason_code={s}",
        .{ state.expected_versions, state.applied_versions, schemaGateReasonCode(null) },
    );
}

fn checkRedisApi(c: CheckCtx, redis_api_url: ?[]const u8) !void {
    log.info(S_DOCTOR_REDIS_CONNECT_START, .{ .role = S_API });
    var client = queue_redis.Client.connectFromEnv(c.io, c.env_map, c.alloc, .api) catch |err| {
        log.err(S_DOCTOR_REDIS_CONNECT_FAILED, .{ .role = S_API, .err = @errorName(err) });
        try appendCheck(c.alloc, c.results, "redis_api_config", false, "REDIS_URL_API not set/invalid", c.ok);
        return;
    };
    defer client.deinit();
    client.readyCheck() catch {
        try appendCheck(c.alloc, c.results, "redis_api_ready", false, "Redis API readiness (PING + XGROUP)", c.ok);
        return;
    };
    const expected = if (redis_api_url) |u| redis_config.usernameFromUrl(u) else null;
    if (expected) |user| {
        const actual = client.aclWhoAmI() catch {
            try appendCheck(c.alloc, c.results, "redis_api_acl_probe", false, "Redis API ACL identity probe failed (ACL WHOAMI)", c.ok);
            return;
        };
        defer c.alloc.free(actual);
        if (!std.mem.eql(u8, actual, user)) {
            try appendCheck(c.alloc, c.results, "redis_api_acl_mismatch", false, "Redis API ACL user mismatch expected URL user", c.ok);
            return;
        }
    }
    log.info(S_DOCTOR_REDIS_CONNECT_OK, .{ .role = S_API });
    try appendCheck(c.alloc, c.results, "redis_api_ready_acl", true, "Redis API readiness + ACL identity", c.ok);
}

/// One shared 64-hex secret check — doctor red exactly where the loader is
/// red, via the same predicate; formerly a triplicated block.
fn checkSecretKey(c: CheckCtx, comptime env_name: []const u8, check_id: []const u8) !void {
    const key: ?[]const u8 = constants.env.owned(c.env_map, c.alloc, env_name) catch null;
    if (key) |k| {
        defer c.alloc.free(k);
        if (validate.isValid64HexKey(k)) {
            try appendCheck(c.alloc, c.results, check_id, true, env_name ++ " set", c.ok);
        } else {
            try appendCheck(c.alloc, c.results, check_id, false, env_name ++ " must be 64 hex chars", c.ok);
        }
    } else {
        try appendCheck(c.alloc, c.results, check_id, false, env_name ++ " not set", c.ok);
    }
}

fn checkOidc(c: CheckCtx) !void {
    const oidc_provider_raw: ?[]const u8 = constants.env.owned(c.env_map, c.alloc, "OIDC_PROVIDER") catch null;
    defer if (oidc_provider_raw) |v| c.alloc.free(v);
    const oidc_provider = blk: {
        const raw = oidc_provider_raw orelse break :blk oidc_auth.Provider.clerk;
        break :blk oidc_auth.parseProvider(std.mem.trim(u8, raw, S_T_R_N)) catch {
            try appendCheck(c.alloc, c.results, S_OIDC_PROVIDER, false, "OIDC_PROVIDER is invalid", c.ok);
            break :blk null;
        };
    };

    // Mirror the loader's enable-gate (issuer non-empty) via the SAME helper
    // BEFORE probing, so the doctor never green-lights a URL the daemon would
    // reject at boot (e.g. OIDC_JWKS_URL set but OIDC_ISSUER missing).
    const issuer: ?[]const u8 = constants.env.owned(c.env_map, c.alloc, "OIDC_ISSUER") catch null;
    defer if (issuer) |v| c.alloc.free(v);
    const explicit_jwks: ?[]const u8 = constants.env.owned(c.env_map, c.alloc, "OIDC_JWKS_URL") catch null;
    defer if (explicit_jwks) |v| c.alloc.free(v);
    const oidc_requested = issuer != null or explicit_jwks != null or oidc_provider_raw != null;

    if (!oidc_auth.isEnabled(issuer)) {
        const detail = if (oidc_requested)
            "OIDC_ISSUER required and non-empty whenever any OIDC var is set"
        else
            "Set OIDC_ISSUER — OIDC is required (the env-var API-key bootstrap was removed)";
        try appendCheck(c.alloc, c.results, "auth_config", false, detail, c.ok);
        return;
    }
    // Enabled: resolve via the SAME helper the daemon uses, then probe.
    const resolved_jwks: ?[]const u8 = oidc_auth.resolveJwksUrl(c.alloc, explicit_jwks, issuer) catch null;
    defer if (resolved_jwks) |v| c.alloc.free(v);
    if (resolved_jwks) |url| {
        if (oidc_provider) |provider| {
            try appendFmtCheck(c.alloc, c.results, S_OIDC_PROVIDER, true, c.ok, "OIDC_PROVIDER={s}", .{@tagName(provider)});
        }
        var verifier = try oidc_auth.Verifier.init(c.alloc, .{ .provider = oidc_provider orelse .clerk, .jwks_url = url });
        defer verifier.deinit();
        var jwks_ok = true;
        verifier.checkJwksConnectivity() catch {
            // URL is public -- print it so a misconfigured issuer is greppable.
            try appendFmtCheck(c.alloc, c.results, S_OIDC_JWKS_REACHABILITY, false, c.ok, "OIDC JWKS fetch failed ({s})", .{url});
            jwks_ok = false;
        };
        if (jwks_ok) {
            try appendCheck(c.alloc, c.results, S_OIDC_JWKS_REACHABILITY, true, "OIDC JWKS reachable", c.ok);
        }
    }
}

test "doctor ACL check reads the username via the queue parser's extraction" {
    try std.testing.expectEqualStrings("api_user", redis_config.usernameFromUrl("redis://api_user:pw@cache.local:6379").?);
    try std.testing.expectEqualStrings("worker_user", redis_config.usernameFromUrl("rediss://worker_user:pw@cache.local:6379").?);
    try std.testing.expect(redis_config.usernameFromUrl("rediss://cache.local:6379") == null);
}

test "schema gate reason and compatibility mapping are deterministic" {
    try std.testing.expectEqualStrings("SCHEMA_COMPATIBLE", schemaGateReasonCode(null));
    try std.testing.expectEqualStrings("SCHEMA_BEHIND_BINARY", schemaGateReasonCode(MigrationSchemaGateError.PendingMigrations));
    try std.testing.expectError(MigrationSchemaGateError.PendingMigrations, ensureSchemaCompatible(.{ .expected_versions = 3, .applied_versions = 2, .latest_expected_version = 3, .latest_applied_version = 2, .has_failed_migrations = false, .lock_available = true, .has_newer_schema_version = false }));
}
