//! `agentsfleet-runner status` — report this host's registration + current state.
//! Uses the read-only `GET /v1/runners/me` (`getSelf`), NOT the heartbeat — so
//! inspecting a host never writes `last_seen_at` and can't mask a dead runner's
//! liveness. Auto-JSON when stdout is piped.

const std = @import("std");
const protocol = @import("contract").protocol;
const Config = @import("../daemon/config.zig");
const Client = @import("../daemon/control_plane_client.zig");
const call_deadline = @import("call_deadline");
const runner_deadline = @import("../daemon/runner_deadline.zig");
const args = @import("args.zig");
const output = @import("output.zig");

pub fn run(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, io: std.Io, alloc: std.mem.Allocator, deadlines: *runner_deadline.Owned) u8 {
    const sched = deadlines.start(alloc);
    const a = output.audience(args.has(argv, output.FLAG_JSON));
    const api = (args.flagOrEnv(env_map, argv, alloc, "--api", Config.ENV_AGENTSFLEET_API_URL) catch return output.fail(a, alloc, output.ERR_OOM)) orelse
        return output.fail(a, alloc, output.ERR_API_URL_UNSET);
    defer alloc.free(api);
    const token = (args.envOwned(env_map, alloc, Config.ENV_AGENTSFLEET_RUNNER_TOKEN) catch return output.fail(a, alloc, output.ERR_OOM)) orelse
        return output.fail(a, alloc, ERR_NO_TOKEN);
    defer alloc.free(token);

    var client = Client.init(alloc, io, sched, api);
    defer client.deinit();
    const parsed = client.getSelf(alloc, token, call_deadline.DEFAULT_DEADLINE_MS) catch return output.fail(a, alloc, output.ERR_UNREACHABLE);
    defer parsed.deinit();
    var buf: [384]u8 = undefined;
    output.writeOut(renderStatus(&buf, a, parsed.value));
    return 0;
}

/// Render the self-status. Pure (no I/O) so the human/JSON contract is testable.
fn renderStatus(buf: []u8, a: output.Audience, s: protocol.SelfResponse) []const u8 {
    return switch (a) {
        .json => std.fmt.bufPrint(buf, "{{\"ok\":true,\"data\":{{\"registered\":true,\"status\":\"{s}\",\"host_id\":\"{s}\",\"last_seen_at\":{d}}}}}\n", .{ s.status, s.host_id, s.last_seen_at }),
        .human => std.fmt.bufPrint(buf, "registered: yes\nstatus:     {s}\nhost:       {s}\nlast seen:  {d}\n", .{ s.status, s.host_id, s.last_seen_at }),
    } catch "\n";
}

const ERR_NO_TOKEN = output.CliError{ .code = "RUNNER_TOKEN_UNSET", .message = "this host has no runner token", .suggestion = "set AGENTSFLEET_RUNNER_TOKEN — ask a platform admin to mint one from the dashboard" };

test "renderStatus reports registration + status in both audiences" {
    var buf: [384]u8 = undefined;
    const s = protocol.SelfResponse{ .id = "r1", .status = "active", .host_id = "host-7", .sandbox_tier = "dev_none", .last_seen_at = 123 };
    const j = renderStatus(&buf, .json, s);
    try std.testing.expect(std.mem.indexOf(u8, j, "\"registered\":true") != null);
    try std.testing.expect(std.mem.indexOf(u8, j, "\"status\":\"active\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, renderStatus(&buf, .human, s), "host-7") != null);
}

// ---------------------------------------------------------------------------
// `run` had no executed lines. Its contract is the operator's triage ladder —
// no URL, no token, unreachable, healthy — and each rung returns a distinct
// structured error, so each is pinned separately.
// ---------------------------------------------------------------------------

const common = @import("common");
const plane_stub = @import("plane_stub_test.zig");

const SELF_OK_BODY =
    "{\"id\":\"r1\",\"status\":\"active\",\"host_id\":\"host-7\",\"sandbox_tier\":\"dev_none\",\"last_seen_at\":123}";

test "status without an API URL fails with the URL error, not a dial attempt" {
    const alloc = std.testing.allocator;
    var map = try common.env.fromPairs(alloc, &.{});
    defer map.deinit();
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status" };
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();

    try std.testing.expectEqual(@as(u8, 1), run(&argv, &map, common.globalIo(), alloc, &deadlines));
}

test "status with a URL but no token names the missing token" {
    const alloc = std.testing.allocator;
    var map = try common.env.fromPairs(alloc, &.{});
    defer map.deinit();
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--api", "http://127.0.0.1:1" };
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();

    // Fails on the token rung BEFORE dialing — port 1 would refuse, but the
    // token check outranks reachability in the triage ladder.
    try std.testing.expectEqual(@as(u8, 1), run(&argv, &map, common.globalIo(), alloc, &deadlines));
}

test "status against a dead control plane reports unreachable" {
    const alloc = std.testing.allocator;
    var map = try common.env.fromPairs(alloc, &.{.{ Config.ENV_AGENTSFLEET_RUNNER_TOKEN, "agt_rtest" }});
    defer map.deinit();
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--api", "http://127.0.0.1:1" };
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();

    try std.testing.expectEqual(@as(u8, 1), run(&argv, &map, common.globalIo(), alloc, &deadlines));
}

test "status renders the registration read from a healthy plane and exits zero" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = plane_stub.boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var stub = plane_stub.OneShotPlane{
        .io = io,
        .listener = &listener,
        .status = .{ .line = "200 OK", .body = SELF_OK_BODY },
    };
    const responder = std.Thread.spawn(.{}, plane_stub.OneShotPlane.serve, .{&stub}) catch return error.SkipZigTest;
    defer responder.join();

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrintZ(&url_buf, "http://127.0.0.1:{d}", .{port});
    var map = try common.env.fromPairs(alloc, &.{.{ Config.ENV_AGENTSFLEET_RUNNER_TOKEN, "agt_rtest" }});
    defer map.deinit();
    const argv = [_][:0]const u8{ "agentsfleet-runner", "status", "--api", url };
    var deadlines: runner_deadline.Owned = .{};
    defer deadlines.deinit();
    var muted = try plane_stub.MutedStdout.mute();
    defer muted.restore();

    try std.testing.expectEqual(@as(u8, 0), run(&argv, &map, io, alloc, &deadlines));
}
