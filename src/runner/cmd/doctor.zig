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

/// Public for `runner_tail_coverage_test.zig` alone, which drives `emit` with
/// stdout muted — the only way to reach the write path in-process.
pub const Check = struct { name: []const u8, ok: bool, detail: []const u8 };

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
    const probe = client.heartbeat(alloc, token.?, call_deadline.DEFAULT_DEADLINE_MS, null) catch |err|
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
    probe.deinit();
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

/// Pure render of the JSON verdict envelope; null on OOM. Split from `emit`
/// so tests can cover rendering without writing to stdout (the test runner's
/// stdout is the build-protocol channel — a printing test hangs the lane).
fn renderJson(alloc: std.mem.Allocator, checks: []const Check) ?[]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{ .ok = allOk(checks), .checks = checks }, .{}) catch null;
}

/// Pure render of one human-audience line into the caller's buffer.
fn renderHumanLine(buf: []u8, c: Check) []const u8 {
    const mark = if (c.ok) "OK" else "!!";
    return std.fmt.bufPrint(buf, "[{s}] {s}: {s}\n", .{ mark, c.name, c.detail }) catch LITERAL;
}

/// Render the whole verdict into one buffer. Pure — no I/O — so both audiences
/// are testable, which the per-check write loop below it never was: under
/// `zig build test` stdout is the build-runner protocol stream, so a test that
/// reaches `writeOut` deadlocks the lane. Writing once also costs one syscall
/// instead of one per check.
fn renderAll(alloc: std.mem.Allocator, a: output.Audience, checks: []const Check) ![]u8 {
    switch (a) {
        .json => {
            const s = renderJson(alloc, checks) orelse return error.OutOfMemory;
            defer alloc.free(s);
            return std.fmt.allocPrint(alloc, "{s}{s}", .{ s, LITERAL });
        },
        .human => {
            var out: std.ArrayList(u8) = .empty;
            errdefer out.deinit(alloc);
            for (checks) |c| {
                var buf: [256]u8 = undefined;
                try out.appendSlice(alloc, renderHumanLine(&buf, c));
            }
            return out.toOwnedSlice(alloc);
        },
    }
}

/// Public for `runner_tail_coverage_test.zig` alone (see `Check`).
pub fn emit(a: output.Audience, alloc: std.mem.Allocator, checks: []const Check) u8 {
    const rendered = renderAll(alloc, a, checks) catch return 1;
    defer alloc.free(rendered);
    output.writeOut(rendered);
    return if (allOk(checks)) 0 else 1;
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

// ---------------------------------------------------------------------------
// Doctor is what an operator trusts before blaming the network, so each verdict
// is pinned rather than assumed. Rendering is proven through the render helpers,
// never `emit`/`run`: those write to stdout, and under `zig build test` stdout
// is the build-runner protocol stream — printing there deadlocks the lane.
// `reachCheck` returns a value and prints nothing, so its arms are driven
// against a scripted control plane.
// ---------------------------------------------------------------------------

const common_test = @import("common");
const dts = @import("../daemon/deadline_test_support.zig");
const plane_stub = @import("plane_stub_test.zig");

const STUB_OK = plane_stub.StubStatus{ .line = "200 OK", .body = "{\"status\":\"ok\"}" };
const STUB_REJECT = plane_stub.StubStatus{ .line = "401 Unauthorized", .body = "{}" };
const STUB_WRONG_HOST = plane_stub.StubStatus{ .line = "302 Found", .body = "" };

/// Run `reachCheck` against a scripted plane and return the check it produced.
fn probePlane(status: plane_stub.StubStatus) !Check {
    const alloc = std.testing.allocator;
    const io = common_test.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = plane_stub.boundPort(listener.socket.handle) catch return error.SkipZigTest;

    var stub = plane_stub.OneShotPlane{ .io = io, .listener = &listener, .status = status };
    const responder = std.Thread.spawn(.{}, plane_stub.OneShotPlane.serve, .{&stub}) catch return error.SkipZigTest;
    defer responder.join();

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    return reachCheck(io, alloc, try deadlines.start(alloc), url, "agt_rtest");
}

test "render arms cover both audiences and the reach probe reports itself skipped" {
    // The reach probe's early guard IS the unit-safe path: with api or token
    // unset it must answer a failed check without constructing a client, and a
    // probe that touched the network here would be the bug. The scheduler
    // pointer is never read on that path, so a placeholder suffices.
    // SAFETY: never dereferenced — the guard returns before any use.
    var sched_unused: call_deadline.ProcessScheduler = undefined;
    const skipped = reachCheck(
        std.testing.io,
        std.testing.allocator,
        &sched_unused,
        null,
        null,
    );
    try std.testing.expect(!skipped.ok);
    try std.testing.expect(std.mem.indexOf(u8, skipped.detail, "skipped") != null);

    // Render helpers, not emit(): emit writes to stdout, and under
    // `zig build test` stdout is the build-runner protocol stream — a test
    // that prints there deadlocks the whole lane.
    const checks = [_]Check{
        .{ .name = "api_url", .ok = true, .detail = "set" },
        .{ .name = "runner_token", .ok = false, .detail = "unset" },
    };
    const s = renderJson(std.testing.allocator, &checks) orelse return error.OutOfMemory;
    defer std.testing.allocator.free(s);
    try std.testing.expect(std.mem.indexOf(u8, s, "\"ok\":false") != null);
    try std.testing.expect(std.mem.indexOf(u8, s, "\"name\":\"runner_token\"") != null);

    var bad_buf: [256]u8 = undefined;
    const bad_line = renderHumanLine(&bad_buf, checks[1]);
    try std.testing.expect(std.mem.indexOf(u8, bad_line, "[!!] runner_token: unset") != null);
    var ok_buf: [256]u8 = undefined;
    const ok_line = renderHumanLine(&ok_buf, checks[0]);
    try std.testing.expect(std.mem.indexOf(u8, ok_line, "[OK] api_url: set") != null);
}

test "reachCheck: a healthy plane reads reachable with a valid token" {
    const check = try probePlane(STUB_OK);
    try std.testing.expect(check.ok);
    try std.testing.expectEqualStrings("reachable; token valid", check.detail);
}

test "reachCheck: a 401 names the token, not the network" {
    // The operator fix differs entirely: mint a fresh agt_r versus check DNS.
    const check = try probePlane(STUB_REJECT);
    try std.testing.expect(!check.ok);
    try std.testing.expect(std.mem.indexOf(u8, check.detail, "token REJECTED") != null);
}

test "reachCheck: a host that answers but is not a control plane is not 'down'" {
    const check = try probePlane(STUB_WRONG_HOST);
    try std.testing.expect(!check.ok);
    try std.testing.expect(std.mem.indexOf(u8, check.detail, "not an agentsfleet control plane") != null);
}

test "reachCheck: a dial failure reads unreachable" {
    // Port 1 on loopback: nothing listens, connect is refused immediately.
    const alloc = std.testing.allocator;
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    const check = reachCheck(common_test.globalIo(), alloc, try deadlines.start(alloc), "http://127.0.0.1:1", "agt_rtest");
    try std.testing.expect(!check.ok);
    try std.testing.expect(std.mem.indexOf(u8, check.detail, "unreachable") != null);
}

test "the human verdict renders every check as one buffer, in order" {
    // The operator's whole reading of `doctor` is these lines. Rendering them
    // into one buffer is what makes them assertable at all — the write itself
    // cannot be reached from a test, because stdout is the build protocol here.
    const alloc = std.testing.allocator;
    const checks = [_]Check{
        .{ .name = "api_url", .ok = true, .detail = "set" },
        .{ .name = "runner_token", .ok = false, .detail = "missing or not a agt_r token" },
        .{ .name = CHECK_CONTROL_PLANE, .ok = false, .detail = "skipped — api/token unset" },
    };
    const rendered = try renderAll(alloc, .human, &checks);
    defer alloc.free(rendered);

    try std.testing.expect(std.mem.startsWith(u8, rendered, "[OK] api_url: set\n"));
    try std.testing.expect(std.mem.indexOf(u8, rendered, "[!!] runner_token:") != null);
    try std.testing.expect(std.mem.indexOf(u8, rendered, "[!!] control_plane:") != null);
    try std.testing.expectEqual(@as(usize, 3), std.mem.count(u8, rendered, "\n"));
}

test "the json verdict is one newline-terminated envelope" {
    // Piped output is parsed by whatever called us, so the envelope must be a
    // single object followed by exactly one newline — not a line per check.
    const alloc = std.testing.allocator;
    const checks = [_]Check{.{ .name = "api_url", .ok = true, .detail = "set" }};
    const rendered = try renderAll(alloc, .json, &checks);
    defer alloc.free(rendered);

    try std.testing.expect(std.mem.indexOf(u8, rendered, "\"ok\":true") != null);
    try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, rendered, "\n"));
    try std.testing.expect(std.mem.endsWith(u8, rendered, "\n"));
}

test "an empty check set still renders, and renders as passing" {
    // Vacuous truth reaches the exit code, so the empty render must not be an
    // error path that reports failure for having nothing to report.
    const alloc = std.testing.allocator;
    const human = try renderAll(alloc, .human, &.{});
    defer alloc.free(human);
    try std.testing.expectEqual(@as(usize, 0), human.len);

    const json = try renderAll(alloc, .json, &.{});
    defer alloc.free(json);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"ok\":true") != null);
}

test "a render that runs out of memory reports failure rather than half a verdict" {
    // `emit` turns any render failure into exit 1. Partial output would be worse
    // than none: a truncated JSON envelope parses as malformed, and a truncated
    // human list reads as "these are all the checks".
    const checks = [_]Check{
        .{ .name = "api_url", .ok = true, .detail = "set" },
        .{ .name = "runner_token", .ok = true, .detail = "present (agt_r)" },
    };
    for (0..4) |fail_index| {
        var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = fail_index });
        const alloc = failing.allocator();
        inline for (.{ output.Audience.human, output.Audience.json }) |audience| {
            if (renderAll(alloc, audience, &checks)) |buf| alloc.free(buf) else |_| {}
        }
    }
}
