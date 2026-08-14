// Integration tests for the runner activity verb —
// `POST /v1/runners/me/leases/{lease_id}/activity`.
//
// The handler had zero executed lines anywhere: no suite drove the live-tail
// forwarding path. Its two hard checks are authz-shaped — the lease must
// resolve AND belong to the presenting runner — and a miss on either is the
// difference between "cosmetic frames" and a runner publishing onto a fleet it
// holds no lease on. The batch walk itself is proven over all four frame
// variants so the wire→SSE vocabulary bridge compiles AND executes per arm.
//
// Requires TEST_DATABASE_URL (harness skips otherwise); the publish rides the
// harness Redis and is best-effort by contract, so assertions here are the
// verb's status/authz surface, not Redis delivery.

const std = @import("std");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const api_key = @import("../../../auth/api_key.zig");
const ec = @import("../../../errors/error_registry.zig");
const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;

const ALLOC = std.testing.allocator;

// Distinct UUIDv7 literals — no collision with sibling runner suites.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee01";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee02";
const RUNNER_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee03";
const RUNNER_OTHER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee04";
const LEASE_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee05";
const LEASE_OTHER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fee06";
const EVENT_ID = "evt-activity-fwd-1";
const NOW_MS: i64 = 1_900_000_000_000;

const TOKEN_OWNER = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "f" ** 60;
const TOKEN_OTHER = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "g" ** 60;

// One frame per ActivityFrame variant: the body that proves every arm of the
// vocabulary bridge executes, not merely compiles.
const FRAMES_ALL_VARIANTS =
    \\{"frames":[
    \\  {"tool_call_started":{"name":"bash","args_redacted":"{\"cmd\":\"ls\"}"}},
    \\  {"fleet_response_chunk":{"text":"partial reply"}},
    \\  {"tool_call_progress":{"name":"bash","elapsed_ms":40}},
    \\  {"tool_call_completed":{"name":"bash","ms":81}}
    \\]}
;

// SAFETY: populated by configureRegistry before the runner_bearer middleware
// (and thus the lookup) ever reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, raw_token: []const u8) !void {
    const hash = api_key.sha256Hex(raw_token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'activity-fwd-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, hash[0..] });
}

fn seedLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test',
        \\        'chat', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
        \\        5, $7, $8, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, runner_id, FLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_ACTIVE });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_OWNER, RUNNER_OTHER });
    base.teardownFleets(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn activityPath(lease_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}/activity", .{ protocol.PATH_RUNNER_LEASES, lease_id });
}

test "integration: activity forwards a mixed frame batch on the runner's own lease" {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_ID);
        try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, "activity-fwd", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedRunner(conn, RUNNER_OTHER, TOKEN_OTHER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER);
        try seedLease(conn, LEASE_OTHER, RUNNER_OTHER);
    }
    defer {
        const conn = h.acquireConn() catch null;
        if (conn) |c| {
            teardown(c);
            h.releaseConn(c);
        }
    }

    // (1) The owner forwards all four frame variants on its own lease → 202,
    // no ack beyond {ok:true}. This is the whole happy path: parse, resolve,
    // publish each variant.
    {
        const path = try activityPath(LEASE_OWNER);
        defer ALLOC.free(path);
        const resp = try (try (try h.post(path).bearer(TOKEN_OWNER)).json(FRAMES_ALL_VARIANTS)).send();
        defer resp.deinit();
        try resp.expectStatus(.accepted);
    }

    // (2) IDOR: the owner presents the OTHER runner's lease_id. The lease
    // exists but is not owned by the presenting runner → 404, typed. Without
    // this check a runner could publish frames onto any fleet's live tail.
    {
        const path = try activityPath(LEASE_OTHER);
        defer ALLOC.free(path);
        const resp = try (try (try h.post(path).bearer(TOKEN_OWNER)).json(FRAMES_ALL_VARIANTS)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
        try std.testing.expect(resp.bodyContains(ec.ERR_RUN_LEASE_NOT_FOUND));
    }

    // (3) A lease_id that resolves nowhere is the same typed 404 — a probe
    // cannot distinguish "foreign" from "absent".
    {
        const path = try activityPath("0195b4ba-8d3a-7f13-8abc-2b3e1e0feeff");
        defer ALLOC.free(path);
        const resp = try (try (try h.post(path).bearer(TOKEN_OWNER)).json(FRAMES_ALL_VARIANTS)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
    }

    // (4) A malformed body on a VALID lease is rejected before any publish.
    {
        const path = try activityPath(LEASE_OWNER);
        defer ALLOC.free(path);
        const resp = try (try (try h.post(path).bearer(TOKEN_OWNER)).json("{\"frames\":[{\"not_a_variant\":{}}]}")).send();
        defer resp.deinit();
        try resp.expectStatus(.bad_request);
        try std.testing.expect(resp.bodyContains(ec.ERR_INVALID_REQUEST));
    }
}
