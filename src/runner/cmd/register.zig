//! `zombie-runner register` — operator-run host registration (Option B). The
//! daemon NEVER calls this (docs/AUTH.md): an operator runs it with a
//! platform-admin Clerk JWT (ZOMBIE_TOKEN env, else `--token`), it POSTs
//! /v1/runners, and on success writes the minted `zrn_` + endpoint + host_id to
//! the env file deploy.sh installs. A tenant `zmb_t_` caller is rejected 403.
//!
//! The `zrn_` is written only to the 0600 env file — never logged, never echoed
//! to stdout (LOGGING/RULE VLT): success reports the runner_id and the path.

const std = @import("std");
const protocol = @import("contract").protocol;
const Config = @import("../daemon/config.zig");
const Client = @import("../daemon/control_plane_client.zig");
const args = @import("args.zig");
const output = @import("output.zig");

const ENV_FILE_FLAG = "--env-file";
const DEFAULT_ENV_FILE = "/etc/default/zombie-runner";
const DEFAULT_TIER = @tagName(protocol.SandboxTier.dev_none);

pub fn run(alloc: std.mem.Allocator) u8 {
    const a = output.audience(args.has(output.FLAG_JSON));
    const api = args.flagOrEnv(alloc, "--api", Config.ENV_ZOMBIE_API_URL) orelse return output.fail(a, alloc, output.ERR_API_URL_UNSET);
    defer alloc.free(api);
    const jwt = args.flagOrEnv(alloc, "--token", Config.ENV_ZOMBIE_TOKEN) orelse return output.fail(a, alloc, ERR_NO_JWT);
    defer alloc.free(jwt);
    const host_id = args.flagOrEnv(alloc, "--host-id", Config.ENV_RUNNER_HOST_ID) orelse return output.fail(a, alloc, ERR_NO_HOST);
    defer alloc.free(host_id);
    const tier = envOrDefault(alloc, Config.ENV_RUNNER_SANDBOX_TIER, DEFAULT_TIER) orelse return output.fail(a, alloc, ERR_OOM);
    defer alloc.free(tier);

    const req = protocol.RegisterRequest{
        .host_id = host_id,
        .sandbox_tier = std.meta.stringToEnum(protocol.SandboxTier, tier) orelse .dev_none,
        .labels = &.{},
    };
    const client = Client{ .base_url = api };
    const result = client.register(alloc, jwt, req) catch return output.fail(a, alloc, output.ERR_UNREACHABLE);
    switch (result) {
        .rejected => |status| return output.fail(a, alloc, rejectionError(status)),
        .created => |parsed| {
            defer parsed.deinit();
            const env_file = args.opt(ENV_FILE_FLAG) orelse DEFAULT_ENV_FILE;
            writeEnvFile(alloc, env_file, api, parsed.value.runner_token, host_id) catch
                return output.fail(a, alloc, ERR_ENV_WRITE);
            return emitSuccess(a, alloc, parsed.value.runner_id, env_file);
        },
    }
}

fn envOrDefault(alloc: std.mem.Allocator, env: []const u8, default: []const u8) ?[]const u8 {
    return std.process.getEnvVarOwned(alloc, env) catch |err| switch (err) {
        // Only an unset var falls through to the default; OOM / invalid encoding
        // must not be masked as "use dev_none" — surface them as null (→ ERR_OOM).
        error.EnvironmentVariableNotFound => alloc.dupe(u8, default) catch null,
        else => null,
    };
}

/// Write the runner env file deploy.sh consumes (0600 — it carries the `zrn_`).
/// Keys are single-sourced from Config (RULE UFS).
fn writeEnvFile(alloc: std.mem.Allocator, path: []const u8, api: []const u8, token: []const u8, host_id: []const u8) !void {
    const content = try std.fmt.allocPrint(alloc, "{s}={s}\n{s}={s}\n{s}={s}\n", .{
        Config.ENV_ZOMBIE_API_URL,    api,
        Config.ENV_ZOMBIE_RUNNER_TOKEN, token,
        Config.ENV_RUNNER_HOST_ID,    host_id,
    });
    defer alloc.free(content);
    var file = try std.fs.cwd().createFile(path, .{ .mode = 0o600 });
    defer file.close();
    // `mode` on createFile only applies when the file is newly created; a
    // pre-existing env file (e.g. 0644 from a prior tool) keeps its perms and
    // truncate would write the zrn_ world-readable. chmod the fd so 0600 holds
    // regardless of prior state (RULE VLT — the "(mode 0600)" claim must be true).
    try file.chmod(0o600);
    try file.writeAll(content);
}

fn emitSuccess(a: output.Audience, alloc: std.mem.Allocator, runner_id: []const u8, env_file: []const u8) u8 {
    var buf: [512]u8 = undefined;
    const line = switch (a) {
        .json => std.fmt.bufPrint(&buf, "{{\"ok\":true,\"data\":{{\"runner_id\":\"{s}\",\"env_file\":\"{s}\"}}}}\n", .{ runner_id, env_file }),
        .human => std.fmt.bufPrint(&buf, "registered runner {s}\n  token written to {s} (mode 0600)\n", .{ runner_id, env_file }),
    } catch {
        _ = alloc;
        return 1;
    };
    output.writeOut(line);
    return 0;
}

/// Map a server rejection status to a precise, actionable CLI error. Codes are
/// CLI-local stable strings (the server-side registry codes stay server-side).
fn rejectionError(status: u16) output.CliError {
    return switch (status) {
        403 => .{ .code = "FORBIDDEN", .message = "platform-admin privileges required to register a runner", .suggestion = "ZOMBIE_TOKEN must be a platform-admin Clerk JWT; a tenant zmb_t_ key is rejected" },
        401 => .{ .code = "UNAUTHENTICATED", .message = "the admin token was not accepted", .suggestion = "check ZOMBIE_TOKEN is a current Clerk JWT (it may have expired)" },
        else => .{ .code = "REGISTER_FAILED", .message = "the control plane refused the registration", .suggestion = "retry; if it persists check the zombied logs for the request id" },
    };
}

const ERR_NO_JWT = output.CliError{ .code = "ADMIN_TOKEN_UNSET", .message = "platform-admin token not set", .suggestion = "set ZOMBIE_TOKEN (or pass --token) to a platform-admin Clerk JWT" };
const ERR_NO_HOST = output.CliError{ .code = "HOST_ID_UNSET", .message = "host id not set", .suggestion = "pass --host-id <id> or set RUNNER_HOST_ID" };
const ERR_ENV_WRITE = output.CliError{ .code = "ENV_WRITE_FAILED", .message = "registered, but writing the env file failed", .suggestion = "re-run with --env-file <path> you can write, or run as the install user" };
const ERR_OOM = output.CliError{ .code = "OUT_OF_MEMORY", .message = "out of memory", .suggestion = "retry" };

test "rejectionError maps 403 to a platform-admin hint, 401 to a token hint" {
    try std.testing.expectEqualStrings("FORBIDDEN", rejectionError(403).code);
    try std.testing.expect(std.mem.indexOf(u8, rejectionError(403).suggestion, "zmb_t_") != null);
    try std.testing.expectEqualStrings("UNAUTHENTICATED", rejectionError(401).code);
    try std.testing.expectEqualStrings("REGISTER_FAILED", rejectionError(500).code);
}
