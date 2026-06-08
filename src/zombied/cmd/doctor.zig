const std = @import("std");
const constants = @import("common");

const db = @import("../db/pool.zig");
const oidc_auth = @import("../auth/oidc.zig");
const env_vars = @import("../config/env_vars.zig");
const queue_redis = @import("../queue/redis.zig");
const common = @import("common.zig");
const doctor_args = @import("doctor_args.zig");
const doctor_render = @import("doctor_render.zig");
const logging = @import("log");

const log = logging.scoped(.zombied);

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

fn redisUsernameFromUrl(url: []const u8) ?[]const u8 {
    const rest = if (std.mem.startsWith(u8, url, "redis://"))
        url["redis://".len..]
    else if (std.mem.startsWith(u8, url, "rediss://"))
        url["rediss://".len..]
    else
        return null;
    const at = std.mem.lastIndexOfScalar(u8, rest, '@') orelse return null;
    const userpass = rest[0..at];
    const colon = std.mem.indexOfScalar(u8, userpass, ':') orelse return null;
    if (colon == 0) return null;
    return userpass[0..colon];
}

fn ensureSchemaCompatible(state: db.MigrationState) MigrationSchemaGateError!void {
    if (state.has_failed_migrations) return MigrationSchemaGateError.FailedMigrations;
    if (state.has_newer_schema_version) return MigrationSchemaGateError.SchemaAhead;
    if (state.applied_versions < state.expected_versions) return MigrationSchemaGateError.PendingMigrations;
}

pub fn run(io: std.Io, env_map: *const EnvMap, argv: []const [:0]const u8, alloc: std.mem.Allocator) !void {
    // Allocator contract: callers must provide an arena-style allocator; CheckResult.detail slices are retained until renderText/renderJson completes.
    log.info("doctor.start", .{});
    var ok = true;
    var stdout_buf: [8192]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &stdout_buf);
    const stdout = &stdout_w.interface;
    var results: std.ArrayList(CheckResult) = .empty;
    defer results.deinit(alloc);

    // argv[0]=binary, argv[1]=subcommand; the rest are doctor flags.
    var extra_args: std.ArrayList([]const u8) = .empty;
    defer extra_args.deinit(alloc);
    if (argv.len > 2) for (argv[2..]) |arg| {
        try extra_args.append(alloc, arg);
    };
    const options = parseDoctorArgs(extra_args.items) catch |err| {
        switch (err) {
            DoctorArgError.InvalidDoctorArgument => try stdout.print("fatal: invalid doctor argument\n", .{}),
            DoctorArgError.MissingFormatValue => try stdout.print("fatal: --format requires a value (text|json)\n", .{}),
            DoctorArgError.InvalidFormatValue => try stdout.print("fatal: invalid --format value (use text|json)\n", .{}),
        }
        try stdout.flush();
        std.process.exit(2);
    };

    var role_urls = try env_vars.loadFromEnv(env_map, alloc);
    defer role_urls.deinit();
    const redis_api_url = role_urls.redis_api;

    role_env_check: {
        env_vars.validateLoaded(role_urls) catch |err| {
            switch (err) {
                env_vars.EnvVarsErrors.MissingDatabaseUrlApi => try appendCheck(alloc, &results, "role_env_required", false, "DATABASE_URL_API required", &ok),
                env_vars.EnvVarsErrors.MissingRedisUrlApi => try appendCheck(alloc, &results, "role_env_redis_required", false, "REDIS_URL_API required", &ok),
                env_vars.EnvVarsErrors.RedisApiTlsRequired => try appendCheck(alloc, &results, "redis_api_tls", false, "REDIS_URL_API must use rediss://", &ok),
            }
            break :role_env_check;
        };
        try appendCheck(alloc, &results, "env_vars_contract", true, "API DB/Redis URLs configured with Redis TLS", &ok);
    }

    db_check: {
        log.info(S_DOCTOR_DB_CONNECT_START, .{ .role = S_API });
        const pool = db.initFromEnvForRole(io, env_map, alloc, .api) catch |err| {
            log.err(S_DOCTOR_DB_CONNECT_FAILED, .{ .role = S_API, .err = @errorName(err) });
            try appendCheck(alloc, &results, S_DB_API_CONFIG, false, "DATABASE_URL_API not set/invalid", &ok);
            break :db_check;
        };
        pool.deinit();
        log.info(S_DOCTOR_DB_CONNECT_OK, .{ .role = S_API });
        try appendCheck(alloc, &results, S_DB_API_CONFIG, true, "API database config", &ok);
    }

    if (options.schema_gate) schema_gate_check: {
        log.info("doctor.schema_gate_start", .{});
        const pool = db.initFromEnvForRole(io, env_map, alloc, .migrator) catch |err| {
            log.err(S_DOCTOR_SCHEMA_GATE_FAILED, .{ .stage = "connect", .err = @errorName(err) });
            try appendCheck(alloc, &results, "schema_gate_config", false, "DATABASE_URL_MIGRATOR not set/invalid", &ok);
            break :schema_gate_check;
        };
        defer pool.deinit();

        const migrations = common.canonicalMigrations();
        const state = db.inspectMigrationState(pool, &migrations) catch |err| {
            log.err(S_DOCTOR_SCHEMA_GATE_FAILED, .{ .stage = "inspect", .err = @errorName(err) });
            try appendCheck(alloc, &results, "schema_gate_state", false, "Unable to inspect migration state", &ok);
            break :schema_gate_check;
        };

        ensureSchemaCompatible(state) catch |err| {
            const reason = schemaGateReasonCode(err);
            try appendFmtCheck(
                alloc,
                &results,
                S_SCHEMA_GATE_COMPAT,
                false,
                &ok,
                "schema_gate status=fail expected_versions={d} applied_versions={d} reason_code={s}",
                .{ state.expected_versions, state.applied_versions, reason },
            );
            break :schema_gate_check;
        };

        log.info("doctor.schema_gate_ok", .{ .expected = state.expected_versions, .applied = state.applied_versions });
        try appendFmtCheck(
            alloc,
            &results,
            S_SCHEMA_GATE_COMPAT,
            true,
            &ok,
            "schema_gate status=ok expected_versions={d} applied_versions={d} reason_code={s}",
            .{ state.expected_versions, state.applied_versions, schemaGateReasonCode(null) },
        );
    }

    redis_api_check: {
        log.info(S_DOCTOR_REDIS_CONNECT_START, .{ .role = S_API });
        var client = queue_redis.Client.connectFromEnv(io, env_map, alloc, .api) catch |err| {
            log.err(S_DOCTOR_REDIS_CONNECT_FAILED, .{ .role = S_API, .err = @errorName(err) });
            try appendCheck(alloc, &results, "redis_api_config", false, "REDIS_URL_API not set/invalid", &ok);
            break :redis_api_check;
        };
        defer client.deinit();
        client.readyCheck() catch {
            try appendCheck(alloc, &results, "redis_api_ready", false, "Redis API readiness (PING + XGROUP)", &ok);
            break :redis_api_check;
        };
        const expected = if (redis_api_url) |u| redisUsernameFromUrl(u) else null;
        if (expected) |user| {
            const actual = client.aclWhoAmI() catch {
                try appendCheck(alloc, &results, "redis_api_acl_probe", false, "Redis API ACL identity probe failed (ACL WHOAMI)", &ok);
                break :redis_api_check;
            };
            defer alloc.free(actual);
            if (!std.mem.eql(u8, actual, user)) {
                try appendCheck(alloc, &results, "redis_api_acl_mismatch", false, "Redis API ACL user mismatch expected URL user", &ok);
                break :redis_api_check;
            }
        }
        log.info(S_DOCTOR_REDIS_CONNECT_OK, .{ .role = S_API });
        try appendCheck(alloc, &results, "redis_api_ready_acl", true, "Redis API readiness + ACL identity", &ok);
    }

    {
        const key: ?[]const u8 = constants.env.owned(env_map, alloc, "ENCRYPTION_MASTER_KEY") catch null;
        if (key) |k| {
            defer alloc.free(k);
            if (k.len == 64) {
                try appendCheck(alloc, &results, S_ENCRYPTION_MASTER_KEY, true, "ENCRYPTION_MASTER_KEY set", &ok);
            } else {
                try appendCheck(alloc, &results, S_ENCRYPTION_MASTER_KEY, false, "ENCRYPTION_MASTER_KEY must be 64 hex chars", &ok);
            }
        } else {
            try appendCheck(alloc, &results, S_ENCRYPTION_MASTER_KEY, false, "ENCRYPTION_MASTER_KEY not set", &ok);
        }
    }

    {
        const key: ?[]const u8 = constants.env.owned(env_map, alloc, "AUTH_SESSION_CODE_PEPPER") catch null;
        if (key) |k| {
            defer alloc.free(k);
            if (k.len == 64) {
                try appendCheck(alloc, &results, S_AUTH_SESSION_CODE_PEPPER, true, "AUTH_SESSION_CODE_PEPPER set", &ok);
            } else {
                try appendCheck(alloc, &results, S_AUTH_SESSION_CODE_PEPPER, false, "AUTH_SESSION_CODE_PEPPER must be 64 hex chars", &ok);
            }
        } else {
            try appendCheck(alloc, &results, S_AUTH_SESSION_CODE_PEPPER, false, "AUTH_SESSION_CODE_PEPPER not set", &ok);
        }
    }

    {
        const key: ?[]const u8 = constants.env.owned(env_map, alloc, "AUDIT_LOG_PEPPER") catch null;
        if (key) |k| {
            defer alloc.free(k);
            if (k.len == 64) {
                try appendCheck(alloc, &results, S_AUDIT_LOG_PEPPER, true, "AUDIT_LOG_PEPPER set", &ok);
            } else {
                try appendCheck(alloc, &results, S_AUDIT_LOG_PEPPER, false, "AUDIT_LOG_PEPPER must be 64 hex chars", &ok);
            }
        } else {
            try appendCheck(alloc, &results, S_AUDIT_LOG_PEPPER, false, "AUDIT_LOG_PEPPER not set", &ok);
        }
    }

    {
        const oidc_provider_raw: ?[]const u8 = constants.env.owned(env_map, alloc, "OIDC_PROVIDER") catch null;
        defer if (oidc_provider_raw) |v| alloc.free(v);
        const oidc_provider = blk: {
            const raw = oidc_provider_raw orelse break :blk oidc_auth.Provider.clerk;
            break :blk oidc_auth.parseProvider(std.mem.trim(u8, raw, S_T_R_N)) catch {
                try appendCheck(alloc, &results, S_OIDC_PROVIDER, false, "OIDC_PROVIDER is invalid", &ok);
                break :blk null;
            };
        };

        var any_auth_configured = false;

        const jwks_url: ?[]const u8 = constants.env.owned(env_map, alloc, "OIDC_JWKS_URL") catch null;
        if (jwks_url) |url| {
            defer alloc.free(url);
            any_auth_configured = true;
            if (std.mem.trim(u8, url, S_T_R_N).len == 0) {
                try appendCheck(alloc, &results, "oidc_jwks_url", false, "OIDC_JWKS_URL is empty", &ok);
            } else {
                if (oidc_provider) |provider| {
                    const provider_name = @tagName(provider);
                    try appendFmtCheck(alloc, &results, S_OIDC_PROVIDER, true, &ok, "OIDC_PROVIDER={s}", .{provider_name});
                }

                var verifier = oidc_auth.Verifier.init(alloc, .{
                    .provider = oidc_provider orelse .clerk,
                    .jwks_url = url,
                });
                defer verifier.deinit();
                var jwks_ok = true;
                verifier.checkJwksConnectivity() catch {
                    try appendCheck(alloc, &results, S_OIDC_JWKS_REACHABILITY, false, "OIDC JWKS fetch failed", &ok);
                    jwks_ok = false;
                };
                if (jwks_ok) {
                    try appendCheck(alloc, &results, S_OIDC_JWKS_REACHABILITY, true, "OIDC JWKS reachable", &ok);
                }
            }
        }

        if (!any_auth_configured) {
            try appendCheck(alloc, &results, "auth_config", false, "Set OIDC_JWKS_URL — OIDC is required (M11_006 removed the API_KEY bootstrap)", &ok);
        }
    }

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

test "redisUsernameFromUrl parses user for redis and rediss" {
    try std.testing.expectEqualStrings("api_user", redisUsernameFromUrl("redis://api_user:pw@cache.local:6379").?);
    try std.testing.expectEqualStrings("worker_user", redisUsernameFromUrl("rediss://worker_user:pw@cache.local:6379").?);
    try std.testing.expect(redisUsernameFromUrl("rediss://cache.local:6379") == null);
}

test "schema gate reason and compatibility mapping are deterministic" {
    try std.testing.expectEqualStrings("SCHEMA_COMPATIBLE", schemaGateReasonCode(null));
    try std.testing.expectEqualStrings("SCHEMA_BEHIND_BINARY", schemaGateReasonCode(MigrationSchemaGateError.PendingMigrations));
    try std.testing.expectError(MigrationSchemaGateError.PendingMigrations, ensureSchemaCompatible(.{ .expected_versions = 3, .applied_versions = 2, .latest_expected_version = 3, .latest_applied_version = 2, .has_failed_migrations = false, .lock_available = true, .has_newer_schema_version = false }));
}
