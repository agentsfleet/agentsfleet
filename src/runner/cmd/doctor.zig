//! `agentsfleet-runner doctor` — preflight a host before the daemon runs: are the
//! required env vars present, and is the control plane reachable with this
//! host's token? Reports each check; exits non-zero if any fails. Auto-JSON
//! when piped. Reachability reuses the heartbeat verb (reachable + token-valid
//! in one probe).

const std = @import("std");
const protocol = @import("contract").protocol;
const Config = @import("../daemon/config.zig");
const Client = @import("../daemon/control_plane_client.zig");
const call_deadline = @import("call_deadline");
const runner_deadline = @import("../daemon/runner_deadline.zig");
const args = @import("args.zig");
const output = @import("output.zig");
const LITERAL = "\n";
const CHECK_CONTROL_PLANE = "control_plane";

const Check = struct { name: []const u8, ok: bool, detail: []const u8 };

pub fn run(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, io: std.Io, alloc: std.mem.Allocator, deadlines: *runner_deadline.Owned) u8 {
    const sched = deadlines.start(alloc);
    const a = output.audience(args.has(argv, output.FLAG_JSON));
    const api = args.flagOrEnv(env_map, argv, alloc, "--api", Config.ENV_AGENTSFLEET_API_URL) catch return output.fail(a, alloc, output.ERR_OOM);
    defer if (api) |v| alloc.free(v);
    const token = args.envOwned(env_map, alloc, Config.ENV_AGENTSFLEET_RUNNER_TOKEN) catch return output.fail(a, alloc, output.ERR_OOM);
    defer if (token) |v| alloc.free(v);

    const env = envChecks(api, token);
    const checks = [_]Check{ env[0], env[1], reachCheck(io, alloc, sched, api, token) };
    return emit(a, alloc, &checks);
}

/// Pure evaluation of the two env preconditions — testable without I/O.
fn envChecks(api: ?[]const u8, token: ?[]const u8) [2]Check {
    const api_ok = api != null;
    const token_ok = token != null and std.mem.startsWith(u8, token.?, protocol.RUNNER_TOKEN_PREFIX);
    const api_detail: []const u8 = if (api_ok) "set" else "missing — pass --api or set AGENTSFLEET_API_URL";
    const token_detail: []const u8 = if (token_ok) "present (agt_r)" else "missing or not a agt_r token";
    return .{
        .{ .name = "api_url", .ok = api_ok, .detail = api_detail },
        .{ .name = "runner_token", .ok = token_ok, .detail = token_detail },
    };
}

/// Reachability + token validity in one heartbeat probe (skipped if either
/// input is unset, so the env checks own that failure).
fn reachCheck(io: std.Io, alloc: std.mem.Allocator, sched: *call_deadline.ProcessScheduler, api: ?[]const u8, token: ?[]const u8) Check {
    if (api == null or token == null) return .{ .name = CHECK_CONTROL_PLANE, .ok = false, .detail = "skipped — api/token unset" };
    var client = Client.init(alloc, io, sched, api.?);
    defer client.deinit();
    _ = client.heartbeat(alloc, token.?, call_deadline.DEFAULT_DEADLINE_MS) catch |err|
        return .{
            .name = CHECK_CONTROL_PLANE,
            .ok = false,
            .detail = switch (err) {
                // 401/403: reached a control plane, but this token is rejected.
                error.Unauthorized => "reachable; token REJECTED (401/403) — mint a fresh agt_r",
                // Non-2xx/non-401 (a 3xx/404/5xx): the TLS host answered but it isn't
                // an agentsfleet control-plane heartbeat endpoint. Distinct from a
                // dial failure so "slack.com answered 302" doesn't read as "down".
                error.BadStatus => "reachable (TLS ok) but not an agentsfleet control plane (non-2xx/401 on /v1/runners/heartbeats)",
                // Connect/TLS/transport failed — genuinely could not reach the host.
                else => "unreachable — could not connect (dial/TLS/transport failed)",
            },
        };
    return .{ .name = CHECK_CONTROL_PLANE, .ok = true, .detail = "reachable; token valid" };
}

/// True only when every check passed — the doctor exit-code contract (any
/// failed check → non-zero). Pure so the contract is unit-testable.
fn allOk(checks: []const Check) bool {
    for (checks) |c| {
        if (!c.ok) return false;
    }
    return true;
}

fn emit(a: output.Audience, alloc: std.mem.Allocator, checks: []const Check) u8 {
    const ok = allOk(checks);
    switch (a) {
        .json => {
            const s = std.json.Stringify.valueAlloc(alloc, .{ .ok = ok, .checks = checks }, .{}) catch return 1;
            defer alloc.free(s);
            output.writeOut(s);
            output.writeOut(LITERAL);
        },
        .human => for (checks) |c| {
            var buf: [256]u8 = undefined;
            const mark = if (c.ok) "OK" else "!!";
            output.writeOut(std.fmt.bufPrint(&buf, "[{s}] {s}: {s}\n", .{ mark, c.name, c.detail }) catch LITERAL);
        },
    }
    return if (ok) 0 else 1;
}

test "envChecks flags missing api + token, passes a valid pair" {
    const missing = envChecks(null, null);
    try std.testing.expect(!missing[0].ok and !missing[1].ok);
    const bad_token = envChecks("http://x", "agt_tdeadbeef");
    try std.testing.expect(bad_token[0].ok and !bad_token[1].ok); // wrong prefix
    const good = envChecks("http://x", protocol.RUNNER_TOKEN_PREFIX ++ "a" ** 64);
    try std.testing.expect(good[0].ok and good[1].ok);
}

test "doctor verdict is non-zero iff any check failed (exit-code contract)" {
    const ok_check = Check{ .name = "a", .ok = true, .detail = "" };
    const bad_check = Check{ .name = "b", .ok = false, .detail = "" };
    try std.testing.expect(allOk(&.{ ok_check, ok_check })); // all pass → 0
    try std.testing.expect(!allOk(&.{ ok_check, bad_check })); // one fail → non-zero
    try std.testing.expect(allOk(&.{})); // vacuously true
}
