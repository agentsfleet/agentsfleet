//! The approval gate's positive control: approval is what RELEASES a run.
//!
//! Every other gate assertion in this suite is a negative — no lease on a
//! pending gate, terminal rows on denial and expiry
//! (`event_lifecycle_integration_test.zig`,
//! `event_lifecycle_reclaim_integration_test.zig`). A fleet that simply could
//! not run would satisfy all of them just as well as a gate that held. These
//! tests prove the same poll that refused a moment ago issues the lease once a
//! human answers, which is what makes the negatives mean "the gate held"
//! rather than "nothing happened".
//!
//! Two variants, deliberately: the credential-free fleet isolates the human's
//! answer as the only variable, and the credentialed fleet proves the answer
//! still releases the run when secret resolution and the integration-grant
//! check sit in front of the gate — the path every user bundle declaring a
//! credential actually takes, where a grant-parked refusal is byte-for-byte
//! indistinguishable from an unanswered gate at the event row.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const life = @import("event_lifecycle_integration_test.zig");
const event_rows = @import("event_rows.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const vault = @import("../state/vault.zig");

const ALLOC = std.testing.allocator;
const DECISION_TTL_S: i64 = 60;

/// Owned by this file alone — `life.Env.deinit` purges a fixed fleet list that
/// does not include this one, so its Redis footprint is dropped here by hand.
const FLEET_GATED_CRED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d11";
const GRANT_ID = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d12";
const GRANT_STATUS_APPROVED = "approved";
const CREDENTIAL_GITHUB = "github";
const HANDLE_GITHUB = "{\"integration\":\"github\",\"installation_id\":\"42\"}";

/// The fixture's gated shape plus a declared credential. Secrets resolve and
/// the standing grant is checked BEFORE the gate is consulted, so this config
/// only reaches the gate at all once both seeds below are in place.
const CONFIG_GATED_CRED =
    \\{"name":"gate-release-cred","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["github"],"budget":{"daily_dollars":5.0},"gates":{"rules":[{"tool":"*","action":"*","behavior":"approve"}],"timeout_ms":1800000}}}
;

/// The action id the daemon parked this event under.
fn gateRefFor(h: anytype, fleet_id: []const u8, event_id: []const u8) !approval_gate_async.EventGateRef {
    const maybe_ref = try approval_gate_async.lookupEventGateRef(&h.queue, fleet_id, event_id);
    return maybe_ref orelse error.GateRefMissing;
}

/// Write the decision the Slack approval webhook would write.
fn resolveGate(h: anytype, ref: *const approval_gate_async.EventGateRef, decision: []const u8) !void {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "{s}{s}", .{ gate_constants.GATE_RESPONSE_KEY_PREFIX, ref.actionId() });
    try h.queue.setEx(key, decision, DECISION_TTL_S);
}

/// The released lease must belong to THIS fleet and THIS event — `pollLease`
/// alone proves only that some lease was issued somewhere.
fn expectLeaseFor(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !void {
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*) FROM fleet.runner_leases
        \\ WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ fleet_id, event_id }));
    defer q.deinit();
    const row = try q.next() orelse return error.LeaseCountMissing;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

fn runParkApproveRelease(env: *life.Env, fleet_id: []const u8) !void {
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const event_id = try life.publishEvent(h, fleet_id);
    defer h.queue.alloc.free(event_id);

    // Parked: the gate holds, no lease is issued, and the daemon recorded the
    // question it raised.
    try std.testing.expect(!try life.pollLease(h));
    const ref = try gateRefFor(h, fleet_id, event_id);
    try resolveGate(h, &ref, gate_constants.GATE_DECISION_APPROVE);

    // Approval is what releases the run: the same poll that refused a moment
    // ago now issues the lease — and the lease is THIS fleet's, for THIS event.
    try std.testing.expect(try life.pollLease(h));
    try expectLeaseFor(conn, fleet_id, event_id);

    // Still claimed, and that is the point: a lease is not a completion. The
    // entry stays in the Pending Entries List until the runner reports, which
    // is what distinguishes "the run started" from the terminal refusals in the
    // sibling suites, where the XACK lands with the `gate_blocked` row.
    try std.testing.expectEqual(@as(i64, 1), try life.pendingCount(h, fleet_id));
    try life.expectRow(conn, fleet_id, event_id, event_rows.STATUS_RECEIVED, "");
}

test "test_approved_event_runs" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();

    {
        const conn = try env.h.acquireConn();
        defer env.h.releaseConn(conn);
        try life.seedFleetWithConfig(conn, life.AGENTSFLEET_GATED, "lifecycle-gated", life.CONFIG_GATED_ALL);
    }
    try runParkApproveRelease(&env, life.AGENTSFLEET_GATED);
}

test "test_approved_event_runs_with_declared_credential" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_GATED_CRED) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});

    {
        const conn = try env.h.acquireConn();
        defer env.h.releaseConn(conn);
        try life.seedFleetWithConfig(conn, FLEET_GATED_CRED, "gate-release-cred", CONFIG_GATED_CRED);
        // Both halves of the credential's authorization, so the only variable
        // left between the refusing poll and the releasing one is the answer.
        try vault.storeJsonPlaintext(ALLOC, conn, life.WORKSPACE_ID, CREDENTIAL_GITHUB, HANDLE_GITHUB);
        _ = try conn.exec(
            \\INSERT INTO core.integration_grants
            \\  (id, fleet_id, service, status, created_at, requested_reason)
            \\VALUES ($1::uuid, $2::uuid, $3, $4, 0, 'gate release credentialed test')
            \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status
        , .{ GRANT_ID, FLEET_GATED_CRED, CREDENTIAL_GITHUB, GRANT_STATUS_APPROVED });
    }
    try runParkApproveRelease(&env, FLEET_GATED_CRED);
}
