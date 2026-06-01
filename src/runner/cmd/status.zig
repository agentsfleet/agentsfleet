//! `zombie-runner status` — report this host's registration + the control
//! plane's current fleet directive. Uses the heartbeat verb (the lightest
//! authenticated runner call; there is no read-only GET yet — a dedicated
//! status read lands with the later fleet-inventory work). Auto-JSON when piped.

const std = @import("std");
const protocol = @import("contract").protocol;
const Config = @import("../daemon/config.zig");
const Client = @import("../daemon/control_plane_client.zig");
const args = @import("args.zig");
const output = @import("output.zig");

pub fn run(alloc: std.mem.Allocator) u8 {
    const a = output.audience(args.has(output.FLAG_JSON));
    const api = args.flagOrEnv(alloc, "--api", Config.ENV_ZOMBIE_API_URL) orelse return output.fail(a, alloc, output.ERR_API_URL_UNSET);
    defer alloc.free(api);
    const token = args.envOwned(alloc, Config.ENV_ZOMBIE_RUNNER_TOKEN) orelse return output.fail(a, alloc, ERR_NO_TOKEN);
    defer alloc.free(token);

    const client = Client{ .base_url = api };
    const hb = client.heartbeat(alloc, token) catch return output.fail(a, alloc, output.ERR_UNREACHABLE);
    var buf: [256]u8 = undefined;
    output.writeOut(renderStatus(&buf, a, hb.status));
    return 0;
}

/// Render the status line for a heartbeat directive. Pure (no I/O) so the
/// human/JSON contract is unit-testable.
fn renderStatus(buf: []u8, a: output.Audience, st: protocol.HeartbeatStatus) []const u8 {
    return switch (a) {
        .json => std.fmt.bufPrint(buf, "{{\"ok\":true,\"data\":{{\"registered\":true,\"fleet\":\"{s}\"}}}}\n", .{@tagName(st)}),
        .human => std.fmt.bufPrint(buf, "registered: yes\nfleet:      {s}\n", .{@tagName(st)}),
    } catch "\n";
}

const ERR_NO_TOKEN = output.CliError{ .code = "RUNNER_TOKEN_UNSET", .message = "this host has no runner token", .suggestion = "set ZOMBIE_RUNNER_TOKEN — have an operator run `zombie-runner register` first" };

test "renderStatus emits the fleet directive in both audiences" {
    var buf: [256]u8 = undefined;
    try std.testing.expect(std.mem.indexOf(u8, renderStatus(&buf, .json, .ok), "\"fleet\":\"ok\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, renderStatus(&buf, .json, .ok), "\"registered\":true") != null);
    try std.testing.expect(std.mem.indexOf(u8, renderStatus(&buf, .human, .drain), "drain") != null);
}
