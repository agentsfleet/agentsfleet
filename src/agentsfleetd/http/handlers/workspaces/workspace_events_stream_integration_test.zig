//! Real-stack integration for the multiplexed workspace SSE stream.
//!
//! Two entry points, by what each test needs:
//!
//!  • Connection-lifecycle tests (cap, forbidden, hub-down, one-slot) drive the
//!    real HTTP handler against the shared fixture workspace. They assert on the
//!    connection, not on the fleet SET, so a workspace shared with sibling
//!    suites under the parallel runner is fine.
//!
//!  • Fan-in tests (fans-in, scoped-fan-in, isolation, changing-set, revocation)
//!    need a DETERMINISTIC fleet set, which the shared workspace cannot give
//!    (siblings seed fleets into it concurrently). They drive the `FanIn` seam
//!    against a workspace UNIQUE to each test — still the real hub, real Redis
//!    pub/sub, and real authorization query, just entered below the SSE socket
//!    so the fleet set is the test's alone. The persona tokens are all pinned to
//!    the one shared workspace, so a constructed principal is the only way to
//!    authorize an isolated one.
//!
//! Requires TEST_DATABASE_URL, TEST_REDIS_TLS_URL, REDIS_URL_API — skipped
//! gracefully otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const pg = @import("pg");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("../fleets/test_sse_client.zig");
const fx = @import("workspace_stream_test_fixtures.zig");
const FanIn = @import("events_stream_fanin.zig");
const principal_mod = @import("../../../auth/principal.zig");
const Subscription = @import("../../../events/subscription.zig");
const activity_channel = @import("../../../events/activity_channel.zig");

const ALLOC = std.testing.allocator;

/// Test-teardown deletes are best-effort — a failed cleanup is logged, never
/// swallowed silently (a bare empty catch trips zlint's suppressed-errors).
const CLEANUP_IGNORED = "cleanup ignored: {s}";

/// Long enough to straddle at least one refresh tick (harness cadence 150 ms)
/// plus the hub's SUBSCRIBE settle.
const TICK_SETTLE_NS: u64 = 500 * std.time.ns_per_ms;
/// A quiet fan-in should see NOTHING within this window — the isolation proof.
const NEGATIVE_POP_MS: u64 = 400;
const POP_WAIT_MS: u64 = 4_000;

fn boot() !*TestHarness {
    return fx.startHarnessWithWorkspace(ALLOC) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => error.SkipZigTest,
        else => err,
    };
}

/// At the FanIn seam, a consumer frame is `channel\npayload` — the originating
/// fleet is the channel it arrived on (the handler splices `fleet_id` into the
/// JSON only when it writes the SSE frame, downstream of here). So route by the
/// channel, exactly as the handler does.
fn frameFromFleet(raw_frame: []const u8, fleet_id: []const u8) bool {
    const split = Subscription.splitTagged(raw_frame) orelse return false;
    const from = activity_channel.fleetId(split.channel_name) orelse return false;
    return std.mem.eql(u8, from, fleet_id);
}

// ── FanIn-seam harness for the deterministic fleet-set tests ─────────────────

/// A workspace owned by this test alone, plus the fan-in watching it. Seeds the
/// workspace + fleets, authorizes a constructed principal (same tenant, no
/// workspace-scope pin), and syncs once so the initial set is attached.
const IsolatedFanIn = struct {
    h: *TestHarness,
    workspace_id: []const u8,
    fanin: *FanIn,

    fn open(h: *TestHarness, workspace_id: []const u8, fleet_ids: []const []const u8) !IsolatedFanIn {
        {
            const conn = try h.acquireConn();
            defer h.releaseConn(conn);
            try seedWorkspace(conn, workspace_id, fx.TEST_TENANT_ID);
            // Each fleet's name must be unique within the workspace
            // (uq_fleets_workspace_id_name); the fleet id is unique, so name by it
            // — a shared name would ON CONFLICT DO NOTHING and drop all but one.
            for (fleet_ids) |zid| try fx.seedFleetInWorkspace(conn, workspace_id, zid, zid);
        }
        const caller = principal(fx.TEST_TENANT_ID);
        const fanin = try FanIn.create(&h.ctx, workspace_id, caller);
        errdefer fanin.destroy();
        // First tick authorizes and attaches the seeded set.
        _ = fanin.sync(clock.nowMillis());
        return .{ .h = h, .workspace_id = workspace_id, .fanin = fanin };
    }

    fn tick(self: *IsolatedFanIn) FanIn.SyncResult {
        return self.fanin.sync(clock.nowMillis());
    }

    fn close(self: *IsolatedFanIn) void {
        self.fanin.destroy();
        const conn = self.h.acquireConn() catch return;
        defer self.h.releaseConn(conn);
        _ = conn.exec("DELETE FROM core.fleets WHERE workspace_id = $1::uuid", .{self.workspace_id}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
        _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{self.workspace_id}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
    }
};

fn principal(tenant_id: []const u8) principal_mod.AuthPrincipal {
    return .{
        .mode = .jwt_oidc,
        .user_id = "user_ws_stream_test",
        .tenant_id = tenant_id,
        .workspace_scope_id = null,
        .scopes = principal_mod.ScopeSet.initEmpty(),
    };
}

fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ workspace_id, tenant_id, clock.nowMillis() });
}

fn publish(pub_client: anytype, fleet_id: []const u8, payload: []const u8) !void {
    const channel = try fx.activityChannel(ALLOC, fleet_id);
    defer ALLOC.free(channel);
    try pub_client.publish(channel, payload);
}

fn expectTaggedFrame(fanin: *FanIn, fleet_id: []const u8) !void {
    const got = fanin.sub.pop(POP_WAIT_MS);
    if (got != .message) return error.NoFrameDelivered;
    defer ALLOC.free(got.message);
    try std.testing.expect(frameFromFleet(got.message, fleet_id));
}

// ── fan-in (FanIn seam, isolated workspace) ─────────────────────────────

test "integration: workspace stream fans in every readable fleet, each frame tagged" {
    const h = try boot();
    defer h.deinit();

    const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f1101";
    const FA = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f1a01";
    const FB = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f1a02";
    const FC = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f1a03";
    var iso = try IsolatedFanIn.open(h, WS, &.{ FA, FB, FC });
    defer iso.close();
    // Three seeded fleets → three attached channels, on ONE shared consumer.
    try std.testing.expectEqual(@as(usize, 3), iso.fanin.channelCount());

    var pub_client = fx.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    common.sleepNanos(TICK_SETTLE_NS); // let the wire SUBSCRIBE settle

    try publish(&pub_client, FA, "{\"kind\":\"event_received\",\"event_id\":\"a\"}");
    try publish(&pub_client, FB, "{\"kind\":\"event_received\",\"event_id\":\"b\"}");
    try publish(&pub_client, FC, "{\"kind\":\"event_received\",\"event_id\":\"c\"}");

    // All three arrive on the ONE consumer queue, each tagged with its fleet.
    var seen_a = false;
    var seen_b = false;
    var seen_c = false;
    var i: usize = 0;
    while (i < 3) : (i += 1) {
        const got = iso.fanin.sub.pop(POP_WAIT_MS);
        try std.testing.expect(got == .message);
        defer ALLOC.free(got.message);
        if (frameFromFleet(got.message, FA)) seen_a = true;
        if (frameFromFleet(got.message, FB)) seen_b = true;
        if (frameFromFleet(got.message, FC)) seen_c = true;
    }
    try std.testing.expect(seen_a and seen_b and seen_c);
}

// ── scoped fan-in, not a pattern subscribe ─────────────────────────────

test "integration: fan-in subscribes one wire channel per fleet (scoped, not a pattern)" {
    const h = try boot();
    defer h.deinit();

    const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f1201";
    const FA = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f2a01";
    const FB = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f2a02";
    const before = h.hub.channelCount();
    var iso = try IsolatedFanIn.open(h, WS, &.{ FA, FB });
    defer iso.close();

    // The fan-in is a per-fleet SUBSCRIBE set (rising by exactly N), never a
    // PSUBSCRIBE over every tenant's channels — the subscriber client exposes no
    // PSUBSCRIBE send at all, so a pattern subscribe is impossible by
    // construction; this proves the scoped shape mechanically.
    try std.testing.expectEqual(before + 2, h.hub.channelCount());
    try std.testing.expectEqual(@as(usize, 2), iso.fanin.channelCount());
}

// ── isolation ──────────────────────────────────────────────────────────

test "integration: fan-in never delivers a frame from a fleet outside the workspace" {
    const h = try boot();
    defer h.deinit();

    const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f2101";
    const MINE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f3a01";
    // A fleet that is NOT in this workspace — its channel is never subscribed.
    const OUTSIDER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f3b01";
    var iso = try IsolatedFanIn.open(h, WS, &.{MINE});
    defer iso.close();

    var pub_client = fx.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    common.sleepNanos(TICK_SETTLE_NS);

    // Publish on the outsider first. If its channel were subscribed (a pattern
    // firehose would subscribe it), it would arrive before ours.
    try publish(&pub_client, OUTSIDER, "{\"kind\":\"event_received\",\"event_id\":\"leak\"}");
    // Nothing must arrive from the outsider within the negative window.
    try std.testing.expect(iso.fanin.sub.pop(NEGATIVE_POP_MS) == .timeout);

    // Our own fleet's frame does arrive — proving the stream is live, not just quiet.
    try publish(&pub_client, MINE, "{\"kind\":\"event_received\",\"event_id\":\"mine\"}");
    try expectTaggedFrame(iso.fanin, MINE);
}

// ── revocation ─────────────────────────────────────────────────────────

test "integration: membership revoked mid-stream unsubscribes the caller on the next tick" {
    const h = try boot();
    defer h.deinit();

    const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f2301";
    const FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f4a01";
    // A second tenant to reassign the workspace to — revocation is the workspace
    // leaving the caller's tenant, which is FK-safe (unlike deleting a workspace
    // a fleet still references).
    const OTHER_TENANT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f00b2";
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec(
            \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
            \\VALUES ($1, 'RevOtherTenant', $2, $2) ON CONFLICT (tenant_id) DO NOTHING
        , .{ OTHER_TENANT, clock.nowMillis() });
    }
    var iso = try IsolatedFanIn.open(h, WS, &.{FLEET});
    defer iso.close();
    defer cleanupTenant(h, OTHER_TENANT);
    try std.testing.expectEqual(@as(usize, 1), iso.fanin.channelCount());

    // Revoke: the workspace moves to another tenant, so the caller no longer
    // owns it — while its fleet set is otherwise unchanged.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec("UPDATE workspaces SET tenant_id = $2 WHERE workspace_id = $1", .{ WS, OTHER_TENANT });
    }

    // The next tick re-authorizes THIS caller (never cached), finds it can no
    // longer read the workspace, and unsubscribes every channel.
    try std.testing.expect(iso.tick() == .revoked);
    try std.testing.expectEqual(@as(usize, 0), iso.fanin.channelCount());
}

// ── a changing fleet set ────────────────────────────────────────────────

test "integration: a fleet created mid-stream is picked up; a deleted one is dropped" {
    const h = try boot();
    defer h.deinit();

    const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f3301";
    const FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f5a01";
    const LATE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f5a02";
    var iso = try IsolatedFanIn.open(h, WS, &.{FLEET});
    defer iso.close();
    try std.testing.expectEqual(@as(usize, 1), iso.fanin.channelCount());

    // Create a fleet AFTER connect; the next tick must pick it up.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fx.seedFleetInWorkspace(conn, WS, LATE, "late");
    }
    // Force staleness so the cache re-enumerates on this tick.
    common.sleepNanos(TICK_SETTLE_NS);
    try std.testing.expect(iso.tick() == .changed);
    try std.testing.expectEqual(@as(usize, 2), iso.fanin.channelCount());

    // Its frames now flow on the same consumer.
    var pub_client = fx.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    common.sleepNanos(TICK_SETTLE_NS);
    try publish(&pub_client, LATE, "{\"kind\":\"event_received\",\"event_id\":\"late\"}");
    try expectTaggedFrame(iso.fanin, LATE);

    // Delete it; the next tick detaches it with no error.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{LATE});
    }
    common.sleepNanos(TICK_SETTLE_NS);
    try std.testing.expect(iso.tick() == .changed);
    try std.testing.expectEqual(@as(usize, 1), iso.fanin.channelCount());
}

// ── one registry slot (HTTP — connection-level, fleet-set-independent) ──

test "integration: a workspace stream claims exactly one registry slot" {
    const SLOT_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f9a01";
    const h = try boot();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fx.seedFleet(conn, SLOT_FLEET, "one-slot");
    }
    defer httpCleanup(h, SLOT_FLEET);

    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
    var sc = try openHttpStream(h);
    defer sc.deinit();
    // However many fleets fan in, the connection is ONE slot.
    try std.testing.expectEqual(@as(usize, 1), h.streams.count());

    var pub_client = fx.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    drainHttp(&sc, &pub_client, SLOT_FLEET);
}

// ── forbidden (HTTP) ────────────────────────────────────────────────────

test "integration: workspace stream forbidden for a workspace the caller cannot read" {
    const h = try boot();
    defer h.deinit();
    const path = try fx.workspaceStreamPathFor(ALLOC, fx.UNKNOWN_WORKSPACE_ID);
    defer ALLOC.free(path);
    const denied = SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR });
    try std.testing.expectError(error.SseUnexpectedStatus, denied);
    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
}

// ── cap (HTTP, drain path) ──────────────────────────────────────────────

test "integration: workspace stream refused at the SSE cap" {
    const h = try boot();
    defer h.deinit();
    h.streams.drain();
    const path = try fx.workspaceStreamPath(ALLOC);
    defer ALLOC.free(path);
    const denied = SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR });
    try std.testing.expectError(error.SseUnexpectedStatus, denied);
    try std.testing.expectEqual(@as(usize, 0), h.streams.count());
}

// ── reconnect resets the sequence (HTTP) ────────────────────────────────

test "integration: reconnect restarts the per-connection id at 0 and ignores Last-Event-ID" {
    const SEQ_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f9b01";
    const h = try boot();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fx.seedFleet(conn, SEQ_FLEET, "seq");
    }
    defer httpCleanup(h, SEQ_FLEET);

    var pub_client = fx.connectPublisher(ALLOC) catch return error.SkipZigTest;
    defer pub_client.deinit();
    const path = try fx.workspaceStreamPath(ALLOC);
    defer ALLOC.free(path);
    const seq_channel = try fx.activityChannel(ALLOC, SEQ_FLEET);
    defer ALLOC.free(seq_channel);

    // The per-connection sequence starts at 0 regardless of which fleet emits
    // the frame, so the assertion is robust to sibling publishes on the shared
    // workspace: the FIRST frame this connection sends carries id 0.
    var sc1 = try connectSeqStream(h, path);
    var sc1_live = true;
    defer if (sc1_live) sc1.deinit();
    var f0 = try awaitFrame(&sc1, &pub_client, SEQ_FLEET);
    try std.testing.expectEqualStrings("0", f0.id);
    f0.deinit(ALLOC);
    fx.closeAndWakeSubscriber(&sc1, &pub_client, seq_channel);
    sc1.deinit();
    sc1_live = false;

    // Reconnect with a forged Last-Event-ID: the server ignores it and the id
    // restarts at 0.
    var sc2 = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR, .last_event_id = "99", .deadline_ms = SEQ_READ_DEADLINE_MS });
    defer sc2.deinit();
    var first = try awaitFrame(&sc2, &pub_client, SEQ_FLEET);
    defer first.deinit(ALLOC);
    try std.testing.expectEqualStrings("0", first.id);

    drainHttp(&sc2, &pub_client, SEQ_FLEET);
}

// A short per-read deadline so a republish attempt fails fast when its frame
// was dropped pre-subscribe.
const SEQ_READ_DEADLINE_MS: u32 = 700;

fn connectSeqStream(h: *TestHarness, path: []const u8) !SseClient {
    return SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR, .deadline_ms = SEQ_READ_DEADLINE_MS });
}

/// Pub/sub has no replay: a frame published before the fan-in's wire SUBSCRIBE
/// settles is dropped, and the workspace stream subscribes to every fleet in
/// the (shared) workspace, so the settle is not a fixed interval. Republish on
/// each short read timeout until one lands — the first frame that arrives is
/// the connection's first emitted frame, so its id is 0 regardless of how many
/// pre-subscribe publishes were dropped.
fn awaitFrame(sc: *SseClient, pub_client: anytype, fleet_id: []const u8) !SseClient.Frame {
    var attempt: usize = 0;
    while (attempt < 30) : (attempt += 1) {
        try publish(pub_client, fleet_id, "{\"kind\":\"event_received\",\"event_id\":\"seq\"}");
        if (sc.nextFrame()) |f| {
            return f;
        } else |err| switch (err) {
            error.SseFrameTimeout => continue,
            else => return err,
        }
    }
    return error.SseFrameTimeout;
}

// ── hub unavailable (HTTP) ──────────────────────────────────────────────

test "integration: workspace stream refused with a transient error when the hub is stopped" {
    const HUBDOWN_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f9c01";
    const h = try boot();
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fx.seedFleet(conn, HUBDOWN_FLEET, "hub-down");
    }
    defer httpCleanup(h, HUBDOWN_FLEET);

    // Stop the hub out from under the handler: creating the shared consumer must
    // fail, so the connect is refused rather than 200-ing into a dead stream.
    h.hub.stop();
    const path = try fx.workspaceStreamPath(ALLOC);
    defer ALLOC.free(path);
    const denied = SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR });
    try std.testing.expectError(error.SseUnexpectedStatus, denied);
}

// ── HTTP helpers (shared fixture workspace) ──────────────────────────────────
//
// Each HTTP test seeds its OWN fleet id so a sibling test's cleanup cannot
// delete the fleet this one is streaming (the workspace is shared under the
// parallel runner; the per-test fleet id is what keeps them from colliding).

fn openHttpStream(h: *TestHarness) !SseClient {
    const path = try fx.workspaceStreamPath(ALLOC);
    defer ALLOC.free(path);
    const sc = try SseClient.connect(ALLOC, h.port, path, .{ .bearer = fx.TOKEN_OPERATOR });
    common.sleepNanos(TICK_SETTLE_NS);
    return sc;
}

fn drainHttp(sc: *SseClient, pub_client: anytype, fleet_id: []const u8) void {
    const channel = fx.activityChannel(ALLOC, fleet_id) catch return;
    defer ALLOC.free(channel);
    fx.closeAndWakeSubscriber(sc, pub_client, channel);
}

fn httpCleanup(h: *TestHarness, fleet_id: []const u8) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{fleet_id}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_id}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
}

fn cleanupTenant(h: *TestHarness, tenant_id: []const u8) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1", .{tenant_id}) catch |err| std.log.warn(CLEANUP_IGNORED, .{@errorName(err)});
}
