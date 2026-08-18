//! Integration tier for the control-plane half of the bind check:
//! `PATCH /v1/fleets/runners/{id}` with `assigned_policy.extra_binds`.
//!
//! The runner validates this same list before `buildArgv`, and the control
//! plane validates it again here — neither side trusts the other's check. That
//! second check had no daemon-side test at all: every proof of `extraBindsValid`
//! lived at the unit tier, against the function rather than against the route
//! that stores what a host will mount. A regression in the handler's call to it
//! would have persisted an unsafe bind set and been caught, at best, one
//! heartbeat later on a degraded runner.
//!
//! What must hold at this boundary: a refused list is never STORED. Storing
//! first and validating on the runner would leave a host mounting whatever the
//! row says the moment someone ships a runner that skips its own check.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const ec = @import("../../../errors/error_registry.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");

const TestHarness = harness_mod.TestHarness;

/// The bind set is assigned through the policy, which is gated on runner:write.
const TOKEN_OPERATOR = scope_fixtures.PLATFORM_ADMIN;

// Fixed v7 ids (the `7` nibble opens the third group) — a non-v7 runner id is
// refused before the body is ever looked at.
const RUNNER_BINDS = "0195b4ba-8d3a-7f13-8abc-00000000eb01";
const HOST_PREFIX = "extra-binds-patch-";

fn policyBody(comptime binds: []const u8) []const u8 {
    return "{\"assigned_policy\":{\"sandbox_tier\":\"container_nested\"," ++
        "\"network_policy\":\"deny_all_egress\",\"registry_allowlist\":[]," ++
        "\"worker_count\":1,\"extra_binds\":" ++ binds ++ "}}";
}

const BINDS_OK = policyBody(
    \\[{"path":"/srv/models","mode":"read_only","note":"shared model cache"}]
);
/// Names a path the daemon already binds. Appended AFTER the baseline, and
/// bubblewrap's last write to a target wins, so this would re-mode `/etc`.
const BINDS_BASELINE = policyBody(
    \\[{"path":"/etc","mode":"read_write","note":"why"}]
);
/// The same mount wearing a second spelling. `bindPathValid` admitted `.`
/// segments until this milestone while the overlap check compared raw strings,
/// so this resolved onto the baseline's `/etc/ssl` having matched neither
/// `/etc` nor `/etc/ssl` textually.
const BINDS_NON_CANONICAL = policyBody(
    \\[{"path":"/etc/./ssl","mode":"read_write","note":"looks new, is not"}]
);
/// Contains `/var/lib/agentsfleet` — the daemon's own state directory, which
/// holds the runner token and the container socket.
const BINDS_CONTAINS_SENSITIVE = policyBody(
    \\[{"path":"/var","mode":"read_write","note":"the whole tree"}]
);

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedRunner(conn: *pg.Conn, id: []const u8) !void {
    const now = clock.nowMillis();
    var host_buf: [64]u8 = undefined;
    const host = try std.fmt.bufPrint(&host_buf, "{s}{s}", .{ HOST_PREFIX, id[24..] });
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $2, 'standard', 'active', '[]'::jsonb, $3, $3, $3)
        \\ON CONFLICT (id) DO UPDATE
        \\  SET admin_state = 'active', extra_binds = NULL
    , .{ id, host, now });
}

/// Best-effort teardown: a failed delete must not mask the assertion the test
/// just made, and the next seed upserts over any survivor.
fn deleteRunner(conn: *pg.Conn, id: []const u8) void {
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{id}) catch |err|
        std.log.warn("extra-binds patch fixture cleanup ignored: {s}", .{@errorName(err)});
}

/// The stored bind set as text, or null when the row carries none. Read back
/// from the ROW rather than from the reply: what a lease mounts comes from the
/// row, so that is what a refusal has to leave untouched.
fn storedBinds(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !?[]u8 {
    var row = (try conn.row("SELECT extra_binds::text FROM fleet.runners WHERE id = $1::uuid", .{id})) orelse
        return error.RunnerMissing;
    defer row.deinit() catch |err|
        std.log.warn("extra-binds row deinit ignored: {s}", .{@errorName(err)});
    const raw = (try row.get(?[]const u8, 0)) orelse return null;
    return try alloc.dupe(u8, raw);
}

fn patchUrl(alloc: std.mem.Allocator, id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/fleets/runners/{s}", .{id});
}

/// Assert one body is refused at the boundary AND changes nothing on the row.
fn expectRefusedAndUnstored(alloc: std.mem.Allocator, comptime body: []const u8) !void {
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_BINDS);
    try seedRunner(conn, RUNNER_BINDS);

    const url = try patchUrl(alloc, RUNNER_BINDS);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();

    const invalid = ec.lookup(ec.ERR_INVALID_REQUEST);
    try std.testing.expectEqual(@intFromEnum(invalid.http_status), @as(u16, r.status));
    try std.testing.expect(std.mem.indexOf(u8, r.body, invalid.code) != null);

    // The load-bearing half: refused means never written. A stored-then-rejected
    // list would still be handed to whatever reads the row next.
    const stored = try storedBinds(alloc, conn, RUNNER_BINDS);
    defer if (stored) |s| alloc.free(s);
    try std.testing.expectEqual(@as(?[]u8, null), stored);
}

test "integration: an operator's valid extra bind is stored and echoed back" {
    const alloc = std.testing.allocator;
    const h = try startHarness(alloc);
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer deleteRunner(conn, RUNNER_BINDS);
    try seedRunner(conn, RUNNER_BINDS);

    const url = try patchUrl(alloc, RUNNER_BINDS);
    defer alloc.free(url);

    const r = try (try (try h.request(.PATCH, url).bearer(TOKEN_OPERATOR)).json(BINDS_OK)).send();
    defer r.deinit();
    try std.testing.expectEqual(@as(u16, 200), r.status);

    // The mount, its mode, and the operator's reason all survive the round trip
    // — the note is what keeps a bind from outliving the reason it was added.
    const stored = (try storedBinds(alloc, conn, RUNNER_BINDS)) orelse return error.TestUnexpectedResult;
    defer alloc.free(stored);
    try std.testing.expect(std.mem.indexOf(u8, stored, "/srv/models") != null);
    try std.testing.expect(std.mem.indexOf(u8, stored, "read_only") != null);
    try std.testing.expect(std.mem.indexOf(u8, stored, "shared model cache") != null);
}

test "integration: a bind naming a daemon-owned path is refused before it is stored" {
    try expectRefusedAndUnstored(std.testing.allocator, BINDS_BASELINE);
}

test "integration: a non-canonical spelling cannot smuggle a bind past the boundary" {
    // The control-plane half of the fix for the reported bypass. The unit tier
    // proves `extraBindsValid` refuses `/etc/./ssl`; this proves the ROUTE does,
    // which is what decides whether an unsafe row can exist at all.
    try expectRefusedAndUnstored(std.testing.allocator, BINDS_NON_CANONICAL);
}

test "integration: a bind CONTAINING a protected path is refused, not just one naming it" {
    // Overlap runs both directions: `/var` contains the daemon's state dir, so
    // binding it read-write would shadow the runner token wholesale. Plain
    // equality would admit this.
    try expectRefusedAndUnstored(std.testing.allocator, BINDS_CONTAINS_SENSITIVE);
}
