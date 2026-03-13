const std = @import("std");

const db = @import("../db/pool.zig");
const oidc_auth = @import("../auth/oidc.zig");
const env_vars = @import("../config/env_vars.zig");
const queue_redis = @import("../queue/redis.zig");

const OutputFormat = enum {
    text,
    json,
};

const DoctorArgError = error{
    InvalidDoctorArgument,
    MissingFormatValue,
    InvalidFormatValue,
};

const CheckResult = struct {
    id: []const u8,
    ok: bool,
    detail: []const u8,
};

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

fn parseFormatValue(raw: []const u8) DoctorArgError!OutputFormat {
    if (std.mem.eql(u8, raw, "text")) return .text;
    if (std.mem.eql(u8, raw, "json")) return .json;
    return DoctorArgError.InvalidFormatValue;
}

fn parseOutputFormatArgs(args: []const []const u8) DoctorArgError!OutputFormat {
    var format: OutputFormat = .text;
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--json")) {
            format = .json;
            continue;
        }
        if (std.mem.eql(u8, arg, "--format")) {
            if (i + 1 >= args.len) return DoctorArgError.MissingFormatValue;
            i += 1;
            format = try parseFormatValue(args[i]);
            continue;
        }
        if (std.mem.startsWith(u8, arg, "--format=")) {
            format = try parseFormatValue(arg["--format=".len..]);
            continue;
        }
        return DoctorArgError.InvalidDoctorArgument;
    }
    return format;
}

fn appendCheck(alloc: std.mem.Allocator, results: *std.ArrayList(CheckResult), id: []const u8, ok: bool, detail: []const u8, overall_ok: *bool) !void {
    try results.append(alloc, .{
        .id = id,
        .ok = ok,
        .detail = detail,
    });
    if (!ok) overall_ok.* = false;
}

fn renderText(stdout: *std.Io.Writer, results: []const CheckResult, overall_ok: bool) !void {
    try stdout.print("zombied doctor\n\n", .{});
    for (results) |c| {
        try stdout.print("  [{s}] {s}\n", .{
            if (c.ok) "OK" else "FAIL",
            c.detail,
        });
    }
    try stdout.print("\n{s}\n", .{
        if (overall_ok) "All checks passed." else "Some checks failed — fix before running serve.",
    });
}

fn renderJson(stdout: *std.Io.Writer, results: []const CheckResult, overall_ok: bool) !void {
    var pass_count: usize = 0;
    for (results) |c| {
        if (c.ok) pass_count += 1;
    }
    const fail_count = results.len - pass_count;

    try stdout.print("{{\"ok\":{s},\"summary\":{{\"total\":{d},\"passed\":{d},\"failed\":{d}}},\"checks\":[", .{
        if (overall_ok) "true" else "false",
        results.len,
        pass_count,
        fail_count,
    });

    for (results, 0..) |c, idx| {
        if (idx > 0) try stdout.print(",", .{});
        try stdout.print("{{\"id\":{f},\"status\":{f},\"detail\":{f}}}", .{
            std.json.fmt(c.id, .{}),
            std.json.fmt(if (c.ok) "ok" else "fail", .{}),
            std.json.fmt(c.detail, .{}),
        });
    }
    try stdout.print("]}}\n", .{});
}

pub fn run(alloc: std.mem.Allocator) !void {
    var ok = true;
    var stdout_buf: [8192]u8 = undefined;
    var stdout_w = std.fs.File.stdout().writer(&stdout_buf);
    const stdout = &stdout_w.interface;
    var results: std.ArrayList(CheckResult) = .{};
    defer results.deinit(alloc);

    var args_it = std.process.args();
    _ = args_it.next();
    _ = args_it.next();
    var extra_args: std.ArrayList([]const u8) = .{};
    defer extra_args.deinit(alloc);
    while (args_it.next()) |arg| {
        try extra_args.append(alloc, arg);
    }
    const output_format = parseOutputFormatArgs(extra_args.items) catch |err| {
        switch (err) {
            DoctorArgError.InvalidDoctorArgument => try stdout.print("fatal: invalid doctor argument\n", .{}),
            DoctorArgError.MissingFormatValue => try stdout.print("fatal: --format requires a value (text|json)\n", .{}),
            DoctorArgError.InvalidFormatValue => try stdout.print("fatal: invalid --format value (use text|json)\n", .{}),
        }
        try stdout.flush();
        std.process.exit(2);
    };

    var role_urls = env_vars.loadFromEnv(alloc);
    defer role_urls.deinit();
    const redis_api_url = role_urls.redis_api;
    const redis_worker_url = role_urls.redis_worker;

    role_env_check: {
        env_vars.validateLoaded(role_urls) catch |err| {
            switch (err) {
                env_vars.EnvVarsErrors.MissingDatabaseUrlApi,
                env_vars.EnvVarsErrors.MissingDatabaseUrlWorker,
                => try appendCheck(alloc, &results, "role_env_required", false, "DATABASE_URL_API and DATABASE_URL_WORKER required (no shared fallback)", &ok),
                env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => try appendCheck(alloc, &results, "role_env_db_separation", false, "DATABASE_URL_API and DATABASE_URL_WORKER must differ", &ok),
                env_vars.EnvVarsErrors.MissingRedisUrlApi,
                env_vars.EnvVarsErrors.MissingRedisUrlWorker,
                => try appendCheck(alloc, &results, "role_env_redis_required", false, "REDIS_URL_API and REDIS_URL_WORKER required (no shared fallback)", &ok),
                env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => try appendCheck(alloc, &results, "role_env_redis_separation", false, "REDIS_URL_API and REDIS_URL_WORKER must differ", &ok),
                env_vars.EnvVarsErrors.RedisApiTlsRequired => try appendCheck(alloc, &results, "redis_api_tls", false, "REDIS_URL_API must use rediss://", &ok),
                env_vars.EnvVarsErrors.RedisWorkerTlsRequired => try appendCheck(alloc, &results, "redis_worker_tls", false, "REDIS_URL_WORKER must use rediss://", &ok),
            }
            break :role_env_check;
        };
        try appendCheck(alloc, &results, "env_vars_contract", true, "Role-separated DB/Redis URLs configured with Redis TLS", &ok);
    }

    db_check: {
        const pool = db.initFromEnvForRole(alloc, .api) catch {
            try appendCheck(alloc, &results, "db_api_config", false, "DATABASE_URL_API not set/invalid", &ok);
            break :db_check;
        };
        pool.deinit();
        try appendCheck(alloc, &results, "db_api_config", true, "API database config", &ok);
    }

    worker_db_check: {
        const pool = db.initFromEnvForRole(alloc, .worker) catch {
            try appendCheck(alloc, &results, "db_worker_config", false, "DATABASE_URL_WORKER not set/invalid", &ok);
            break :worker_db_check;
        };
        pool.deinit();
        try appendCheck(alloc, &results, "db_worker_config", true, "Worker database config", &ok);
    }

    redis_api_check: {
        var client = queue_redis.Client.connectFromEnv(alloc, .api) catch {
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
        try appendCheck(alloc, &results, "redis_api_ready_acl", true, "Redis API readiness + ACL identity", &ok);
    }

    redis_worker_check: {
        var client = queue_redis.Client.connectFromEnv(alloc, .worker) catch {
            try appendCheck(alloc, &results, "redis_worker_config", false, "REDIS_URL_WORKER not set/invalid", &ok);
            break :redis_worker_check;
        };
        defer client.deinit();
        client.readyCheck() catch {
            try appendCheck(alloc, &results, "redis_worker_ready", false, "Redis worker readiness (PING + XGROUP)", &ok);
            break :redis_worker_check;
        };
        const expected = if (redis_worker_url) |u| redisUsernameFromUrl(u) else null;
        if (expected) |user| {
            const actual = client.aclWhoAmI() catch {
                try appendCheck(alloc, &results, "redis_worker_acl_probe", false, "Redis worker ACL identity probe failed (ACL WHOAMI)", &ok);
                break :redis_worker_check;
            };
            defer alloc.free(actual);
            if (!std.mem.eql(u8, actual, user)) {
                try appendCheck(alloc, &results, "redis_worker_acl_mismatch", false, "Redis worker ACL user mismatch expected URL user", &ok);
                break :redis_worker_check;
            }
        }
        try appendCheck(alloc, &results, "redis_worker_ready_acl", true, "Redis worker readiness + ACL identity", &ok);
    }

    {
        const key = std.process.getEnvVarOwned(alloc, "ENCRYPTION_MASTER_KEY") catch null;
        if (key) |k| {
            defer alloc.free(k);
            if (k.len == 64) {
                try appendCheck(alloc, &results, "encryption_master_key", true, "ENCRYPTION_MASTER_KEY set", &ok);
            } else {
                try appendCheck(alloc, &results, "encryption_master_key", false, "ENCRYPTION_MASTER_KEY must be 64 hex chars", &ok);
            }
        } else {
            try appendCheck(alloc, &results, "encryption_master_key", false, "ENCRYPTION_MASTER_KEY not set", &ok);
        }
    }

    {
        const app_id = std.process.getEnvVarOwned(alloc, "GITHUB_APP_ID") catch null;
        if (app_id) |id| {
            defer alloc.free(id);
            if (id.len > 0) {
                try appendCheck(alloc, &results, "github_app_id", true, "GITHUB_APP_ID set", &ok);
            } else {
                try appendCheck(alloc, &results, "github_app_id", false, "GITHUB_APP_ID is empty", &ok);
            }
        } else {
            try appendCheck(alloc, &results, "github_app_id", false, "GITHUB_APP_ID not set", &ok);
        }
    }

    {
        const key = std.process.getEnvVarOwned(alloc, "GITHUB_APP_PRIVATE_KEY") catch null;
        if (key) |k| {
            defer alloc.free(k);
            if (k.len > 0) {
                try appendCheck(alloc, &results, "github_app_private_key", true, "GITHUB_APP_PRIVATE_KEY set", &ok);
            } else {
                try appendCheck(alloc, &results, "github_app_private_key", false, "GITHUB_APP_PRIVATE_KEY is empty", &ok);
            }
        } else {
            try appendCheck(alloc, &results, "github_app_private_key", false, "GITHUB_APP_PRIVATE_KEY not set", &ok);
        }
    }

    {
        const config_dir = std.process.getEnvVarOwned(alloc, "AGENT_CONFIG_DIR") catch
            try alloc.dupe(u8, "./config");
        defer alloc.free(config_dir);

        const required_files = [_][]const u8{
            "echo-prompt.md",        "scout-prompt.md", "warden-prompt.md",
            "echo.json",             "scout.json",      "warden.json",
            "pipeline-default.json",
        };
        for (required_files) |fname| {
            const path = try std.fmt.allocPrint(alloc, "{s}/{s}", .{ config_dir, fname });
            defer alloc.free(path);
            if (std.fs.accessAbsolute(path, .{})) |_| {
                const detail = try std.fmt.allocPrint(alloc, "config/{s}", .{fname});
                defer alloc.free(detail);
                try appendCheck(alloc, &results, "agent_config_file", true, detail, &ok);
            } else |_| {
                const detail = try std.fmt.allocPrint(alloc, "config/{s} missing", .{fname});
                defer alloc.free(detail);
                try appendCheck(alloc, &results, "agent_config_file", false, detail, &ok);
            }
        }
    }

    {
        const oidc_provider_raw = std.process.getEnvVarOwned(alloc, "OIDC_PROVIDER") catch null;
        defer if (oidc_provider_raw) |v| alloc.free(v);
        const oidc_provider = blk: {
            const raw = oidc_provider_raw orelse break :blk oidc_auth.Provider.clerk;
            break :blk oidc_auth.parseProvider(std.mem.trim(u8, raw, " \t\r\n")) catch {
                try appendCheck(alloc, &results, "oidc_provider", false, "OIDC_PROVIDER is invalid", &ok);
                break :blk null;
            };
        };

        var any_auth_configured = false;

        const jwks_url = std.process.getEnvVarOwned(alloc, "OIDC_JWKS_URL") catch null;
        if (jwks_url) |url| {
            defer alloc.free(url);
            any_auth_configured = true;
            if (std.mem.trim(u8, url, " \t\r\n").len == 0) {
                try appendCheck(alloc, &results, "oidc_jwks_url", false, "OIDC_JWKS_URL is empty", &ok);
            } else {
                if (oidc_provider) |provider| {
                    const provider_name = @tagName(provider);
                    const detail = try std.fmt.allocPrint(alloc, "OIDC_PROVIDER={s}", .{provider_name});
                    defer alloc.free(detail);
                    try appendCheck(alloc, &results, "oidc_provider", true, detail, &ok);
                }

                var verifier = oidc_auth.Verifier.init(alloc, .{
                    .provider = oidc_provider orelse .clerk,
                    .jwks_url = url,
                });
                defer verifier.deinit();
                var jwks_ok = true;
                verifier.checkJwksConnectivity() catch {
                    try appendCheck(alloc, &results, "oidc_jwks_reachability", false, "OIDC JWKS fetch failed", &ok);
                    jwks_ok = false;
                };
                if (jwks_ok) {
                    try appendCheck(alloc, &results, "oidc_jwks_reachability", true, "OIDC JWKS reachable", &ok);
                }
            }
        }

        const key = std.process.getEnvVarOwned(alloc, "API_KEY") catch null;
        if (key) |k| {
            defer alloc.free(k);
            any_auth_configured = true;
            if (std.mem.trim(u8, k, " \t\r\n").len == 0) {
                try appendCheck(alloc, &results, "api_key", false, "API_KEY is empty", &ok);
            } else {
                try appendCheck(alloc, &results, "api_key", true, "API_KEY configured", &ok);
            }
        }

        if (!any_auth_configured) {
            try appendCheck(alloc, &results, "auth_config", false, "Set OIDC_JWKS_URL or API_KEY", &ok);
        }
    }

    switch (output_format) {
        .text => try renderText(stdout, results.items, ok),
        .json => try renderJson(stdout, results.items, ok),
    }
    try stdout.flush();
    if (!ok) std.process.exit(1);
}

test "redisUsernameFromUrl parses user for redis and rediss" {
    try std.testing.expectEqualStrings("api_user", redisUsernameFromUrl("redis://api_user:pw@cache.local:6379").?);
    try std.testing.expectEqualStrings("worker_user", redisUsernameFromUrl("rediss://worker_user:pw@cache.local:6379").?);
    try std.testing.expect(redisUsernameFromUrl("rediss://cache.local:6379") == null);
}

test "parseOutputFormatArgs supports --json and --format" {
    const args1 = [_][]const u8{"--json"};
    try std.testing.expectEqual(OutputFormat.json, try parseOutputFormatArgs(&args1));

    const args2 = [_][]const u8{ "--format", "json" };
    try std.testing.expectEqual(OutputFormat.json, try parseOutputFormatArgs(&args2));

    const args3 = [_][]const u8{"--format=text"};
    try std.testing.expectEqual(OutputFormat.text, try parseOutputFormatArgs(&args3));
}

test "parseOutputFormatArgs validates invalid values" {
    const args1 = [_][]const u8{ "--format", "yaml" };
    try std.testing.expectError(DoctorArgError.InvalidFormatValue, parseOutputFormatArgs(&args1));

    const args2 = [_][]const u8{"--format"};
    try std.testing.expectError(DoctorArgError.MissingFormatValue, parseOutputFormatArgs(&args2));

    const args3 = [_][]const u8{"--unknown"};
    try std.testing.expectError(DoctorArgError.InvalidDoctorArgument, parseOutputFormatArgs(&args3));
}
