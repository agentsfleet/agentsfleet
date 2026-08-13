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
const protocol = @import("contract").protocol;

const life = @import("event_lifecycle_integration_test.zig");
const event_rows = @import("event_rows.zig");
const approval_gate = @import("../fleet_runtime/approval_gate.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const binding_json = @import("../fleet_runtime/repository_binding_json.zig");
const gate_constants = @import("../fleet_runtime/approval_gate_constants.zig");
const repair_branch = @import("../git/repair_branch.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const vault = @import("../state/vault.zig");

const ALLOC = std.testing.allocator;
const TEST_RESOLVER = "user:gate-release-test";
const RUNNER_TOKEN = @import("../auth/middleware/mod.zig").runner_bearer.RUNNER_TOKEN_PREFIX ++ "e" ** 64;

/// Owned by this file alone — `life.Env.deinit` purges a fixed fleet list that
/// does not include this one, so its Redis footprint is dropped here by hand.
const FLEET_GATED_CRED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d11";
/// Write-kind fixtures, also purged by hand.
const FLEET_WRITE_NO_GATES = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d21";
const FLEET_WRITE_RULE_FALLTHROUGH = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d22";
const FLEET_WRITE_ROW = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d23";
const FLEET_WRITE_LEGACY = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d24";
const FLEET_NETWORK_READ_ONLY = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d25";
const FLEET_NETWORK_READ_POST = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d26";
const FLEET_READ_BINDING = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d27";
/// The repository the write-kind configs below bind, as the mint compares it.
const REPOS_BOUND = [_][]const u8{"acme/payments"};
const REPOSITORY_BASE = "main";
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

/// A WRITE-access binding and NO gates block at all: the hole the kind-park
/// closes — without it this config auto-passed at the `gates orelse`
/// early return and a write-capable run leased with no human in the loop.
const CONFIG_WRITE_NO_GATES =
    \\{"name":"write-kind-ungated","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"repositories":["acme/payments"],"repository_access":"write","repository_base":"main"}}
;

/// A WRITE-access binding whose gate rules match NOTHING — the
/// `.auto_approve` no-match fallthrough. Rule-parking is what this fixture
/// proves unsafe: the kind must park even though every rule misses.
const CONFIG_WRITE_RULE_FALLTHROUGH =
    \\{"name":"write-kind-fallthrough","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"repositories":["acme/payments"],"repository_access":"write","repository_base":"main","gates":{"rules":[{"tool":"never-matches","action":"never","behavior":"approve"}],"timeout_ms":1800000}}}
;

/// Persisted by the earlier daemon before write bindings named a trusted base.
/// New authoring rejects this shape; only the stored parser may read it.
const CONFIG_WRITE_LEGACY =
    \\{"name":"write-kind-legacy","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"repositories":["acme/payments"],"repository_access":"write"}}
;
const CONFIG_NETWORK_READ_ONLY =
    \\{"name":"network-read-only","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"network":{"allow":["api.example.com"],"read_only":true}}}
;
const CONFIG_NETWORK_READ_POST =
    \\{"name":"network-read-post","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"network":{"allow":["api.example.com"],"read_post_paths":["https://api.example.com/query"]}}}
;
const CONFIG_READ_BINDING =
    \\{"name":"read-binding","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"repositories":["acme/payments"],"repository_access":"read"}}
;

/// The action id the daemon parked this event under.
fn gateRefFor(h: anytype, fleet_id: []const u8, event_id: []const u8) !approval_gate_async.EventGateRef {
    const maybe_ref = try approval_gate_async.lookupEventGateRef(&h.queue, fleet_id, event_id);
    return maybe_ref orelse error.GateRefMissing;
}

/// Approve through the same durable path used by every production channel.
fn approveGate(h: anytype, ref: *const approval_gate_async.EventGateRef) !void {
    var outcome = try approval_gate.resolve(h.pool, &h.queue, ALLOC, .{
        .action_id = ref.actionId(),
        .outcome = .approved,
        .by = TEST_RESOLVER,
    });
    defer switch (outcome) {
        .resolved => |*row| @constCast(row).deinit(ALLOC),
        .already_resolved => |*row| @constCast(row).deinit(ALLOC),
        .not_found => {},
    };
    try std.testing.expect(outcome == .resolved);
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

fn expectNoLeaseFor(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !void {
    var query = PgQuery.from(try conn.query(
        "SELECT count(*) FROM fleet.runner_leases WHERE fleet_id = $1::uuid AND event_id = $2",
        .{ fleet_id, event_id },
    ));
    defer query.deinit();
    const row = try query.next() orelse return error.LeaseCountMissing;
    try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
}

fn expectCount(conn: *pg.Conn, statement: []const u8, args: anytype, expected: i64) !void {
    var query = PgQuery.from(try conn.query(statement, args));
    defer query.deinit();
    const row = try query.next() orelse return error.CountMissing;
    try std.testing.expectEqual(expected, try row.get(i64, 0));
}

fn approvedRepairBranch(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) ![repair_branch.BRANCH_LEN]u8 {
    var query = PgQuery.from(try conn.query(
        \\SELECT id::text FROM core.fleet_approval_gates
        \\WHERE fleet_id = $1::uuid AND event_id = $2
        \\  AND gate_kind = $3 AND status = $4
    , .{ fleet_id, event_id, gate_constants.GATE_KIND_REPOSITORY_WRITE, approval_gate.GateStatus.approved.toSlice() }));
    defer query.deinit();
    const row = try query.next() orelse return error.ApprovedGateRowMissing;
    return repair_branch.fromGateId(try row.get([]const u8, 0));
}

fn expectIssuedLease(h: anytype, fleet_id: []const u8, event_id: []const u8, expected_branch: ?[]const u8) !void {
    const request = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json(protocol.LEASE_REQUEST_CURRENT_JSON);
    const response = try request.send();
    defer response.deinit();
    try response.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(protocol.LeaseResponse, ALLOC, response.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.lease orelse return error.ReleasedLeaseMissing;
    try std.testing.expectEqualStrings(fleet_id, lease.event.fleet_id);
    try std.testing.expectEqualStrings(event_id, lease.event.event_id);
    if (expected_branch == null) {
        try std.testing.expectEqual(@as(usize, 0), lease.policy.http_origin_policies.len);
        return;
    }
    try std.testing.expectEqual(@as(usize, 1), lease.policy.http_origin_policies.len);
    const origin = lease.policy.http_origin_policies[0];
    try std.testing.expectEqualStrings("api.github.com", origin.host);
    try std.testing.expectEqual(@as(usize, 7), origin.requests.len);
    try std.testing.expectEqualStrings(REPOSITORY_BASE, origin.requests[6].json_fields[1].string_value.?);
    try std.testing.expectEqualStrings(expected_branch.?, origin.requests[6].json_fields[0].string_value.?);
    try std.testing.expect(std.mem.indexOf(u8, lease.instructions, expected_branch.?) != null);
}

fn expectNetworkPolicyLease(
    h: anytype,
    fleet_id: []const u8,
    event_id: []const u8,
    read_only: bool,
    expected_post_paths: usize,
) !void {
    const request = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json(protocol.LEASE_REQUEST_CURRENT_JSON);
    const response = try request.send();
    defer response.deinit();
    try response.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(protocol.LeaseResponse, ALLOC, response.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.lease orelse return error.ReleasedLeaseMissing;
    try std.testing.expectEqualStrings(fleet_id, lease.event.fleet_id);
    try std.testing.expectEqualStrings(event_id, lease.event.event_id);
    try std.testing.expectEqual(read_only, lease.policy.network_policy.read_only);
    try std.testing.expectEqual(expected_post_paths, lease.policy.network_policy.read_post_paths.len);
}

fn expectReadBindingLease(h: anytype, fleet_id: []const u8, event_id: []const u8) !void {
    const request = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json(protocol.LEASE_REQUEST_CURRENT_JSON);
    const response = try request.send();
    defer response.deinit();
    try response.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(protocol.LeaseResponse, ALLOC, response.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.lease orelse return error.ReleasedLeaseMissing;
    try std.testing.expectEqualStrings(fleet_id, lease.event.fleet_id);
    try std.testing.expectEqualStrings(event_id, lease.event.event_id);
    try std.testing.expectEqualStrings("You are an event-lifecycle test fleet.", lease.instructions);
    const binding = lease.policy.repository_binding orelse return error.RepositoryBindingMissing;
    try std.testing.expectEqual(@as(usize, 1), binding.repositories.len);
    try std.testing.expectEqualStrings(REPOS_BOUND[0], binding.repositories[0]);
    try std.testing.expect(binding.access == .read);
    try std.testing.expectEqual(@as(usize, 1), lease.policy.http_origin_policies.len);
    const origin = lease.policy.http_origin_policies[0];
    try std.testing.expectEqualStrings("api.github.com", origin.host);
    try std.testing.expectEqual(@as(usize, 1), origin.credential_names.len);
    try std.testing.expectEqualStrings(CREDENTIAL_GITHUB, origin.credential_names[0]);
    try std.testing.expectEqual(@as(usize, 2), origin.requests.len);
    try std.testing.expect(origin.requests[0].method == .get);
    try std.testing.expect(origin.requests[1].method == .head);
    for (origin.requests) |rule| {
        try std.testing.expectEqualStrings("/repos/acme/payments/", rule.path);
        try std.testing.expect(rule.path_match == .prefix);
    }
}

fn runVersionOneNetworkRefusal(
    fleet_id: []const u8,
    fleet_name: []const u8,
    config: []const u8,
    read_only: bool,
    expected_post_paths: usize,
) !void {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, fleet_id) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    try life.seedFleetWithConfig(conn, fleet_id, fleet_name, config);
    const event_id = try life.publishEvent(env.h, fleet_id);
    defer env.h.queue.alloc.free(event_id);

    const old_request = try (try env.h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("");
    const old_response = try old_request.send();
    defer old_response.deinit();
    try old_response.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, old_response.body, "\"lease\":null") != null);
    try expectNoLeaseFor(conn, fleet_id, event_id);
    try expectNetworkPolicyLease(env.h, fleet_id, event_id, read_only, expected_post_paths);
    try expectLeaseFor(conn, fleet_id, event_id);
}

fn runParkApproveRelease(env: *life.Env, fleet_id: []const u8, expect_repair: bool) !void {
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const event_id = try life.publishEvent(h, fleet_id);
    defer h.queue.alloc.free(event_id);

    // Parked: the gate holds, no lease is issued, and the daemon recorded the
    // question it raised.
    try std.testing.expect(!try life.pollLease(h));
    const ref = try gateRefFor(h, fleet_id, event_id);
    try approveGate(h, &ref);
    const repair = if (expect_repair) try approvedRepairBranch(conn, fleet_id, event_id) else null;

    // Approval is what releases the run: the same poll that refused a moment
    // ago now issues the lease — and the lease is THIS fleet's, for THIS event.
    try expectIssuedLease(h, fleet_id, event_id, if (repair) |*branch| branch[0..] else null);
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
    try runParkApproveRelease(&env, life.AGENTSFLEET_GATED, false);
}

test "test_write_kind_parks_without_gates_config" {
    // Dimension 1.1 (+1.4): a write-access fleet with NO gates config still
    // parks — and the approval still releases the run, owned by this fleet and
    // event. `runParkApproveRelease` asserts both halves: the refusing poll
    // (the park) and the releasing one (the answer).
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_WRITE_NO_GATES) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});

    {
        const conn = try env.h.acquireConn();
        defer env.h.releaseConn(conn);
        try life.seedFleetWithConfig(conn, FLEET_WRITE_NO_GATES, "write-kind-ungated", CONFIG_WRITE_NO_GATES);
    }
    try runParkApproveRelease(&env, FLEET_WRITE_NO_GATES, true);
}

test "integration: version-one runner cannot claim version-two repair policy" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_WRITE_NO_GATES) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    try life.seedFleetWithConfig(conn, FLEET_WRITE_NO_GATES, "write-kind-ungated", CONFIG_WRITE_NO_GATES);
    const event_id = try life.publishEvent(env.h, FLEET_WRITE_NO_GATES);
    defer env.h.queue.alloc.free(event_id);
    try std.testing.expect(!try life.pollLease(env.h));
    const ref = try gateRefFor(env.h, FLEET_WRITE_NO_GATES, event_id);
    try approveGate(env.h, &ref);
    const old_request = try (try env.h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("");
    const old_response = try old_request.send();
    defer old_response.deinit();
    try old_response.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, old_response.body, "\"lease\":null") != null);
    try expectNoLeaseFor(conn, FLEET_WRITE_NO_GATES, event_id);
    const branch = try approvedRepairBranch(conn, FLEET_WRITE_NO_GATES, event_id);
    try expectIssuedLease(env.h, FLEET_WRITE_NO_GATES, event_id, &branch);
    try expectLeaseFor(conn, FLEET_WRITE_NO_GATES, event_id);
}

test "integration: version-one runner cannot claim read-only network policy" {
    try runVersionOneNetworkRefusal(
        FLEET_NETWORK_READ_ONLY,
        "network-read-only",
        CONFIG_NETWORK_READ_ONLY,
        true,
        0,
    );
}

test "integration: version-one runner cannot claim read-post network policy" {
    try runVersionOneNetworkRefusal(
        FLEET_NETWORK_READ_POST,
        "network-read-post",
        CONFIG_NETWORK_READ_POST,
        false,
        1,
    );
}

test "integration: read binding authors an enforceable lease policy" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_READ_BINDING) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    try life.seedFleetWithConfig(conn, FLEET_READ_BINDING, "read-binding", CONFIG_READ_BINDING);
    const event_id = try life.publishEvent(env.h, FLEET_READ_BINDING);
    defer env.h.queue.alloc.free(event_id);

    const old_request = try (try env.h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("");
    const old_response = try old_request.send();
    defer old_response.deinit();
    try old_response.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, old_response.body, "\"lease\":null") != null);
    try expectNoLeaseFor(conn, FLEET_READ_BINDING, event_id);
    try expectReadBindingLease(env.h, FLEET_READ_BINDING, event_id);
    try expectLeaseFor(conn, FLEET_READ_BINDING, event_id);
}

test "test_write_kind_ignores_rule_fallthrough" {
    // Dimension 1.2: gate rules that match nothing fall through to
    // `.auto_approve` — the write kind must park anyway, because rules are
    // `fleet:write`-PATCHable and cannot hold this boundary.
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_WRITE_RULE_FALLTHROUGH) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});

    {
        const conn = try env.h.acquireConn();
        defer env.h.releaseConn(conn);
        try life.seedFleetWithConfig(conn, FLEET_WRITE_RULE_FALLTHROUGH, "write-kind-fallthrough", CONFIG_WRITE_RULE_FALLTHROUGH);
    }
    try runParkApproveRelease(&env, FLEET_WRITE_RULE_FALLTHROUGH, true);
}

test "integration: stored write binding without base refuses before billing and approval" {
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_WRITE_LEGACY) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    try life.seedFleetWithConfig(conn, FLEET_WRITE_LEGACY, "write-kind-legacy", CONFIG_WRITE_LEGACY);

    const refused_id = try life.publishEvent(env.h, FLEET_WRITE_LEGACY);
    defer env.h.queue.alloc.free(refused_id);
    try std.testing.expect(!try life.pollLease(env.h));
    try life.expectRow(
        conn,
        FLEET_WRITE_LEGACY,
        refused_id,
        event_rows.STATUS_GATE_BLOCKED,
        event_rows.LABEL_REPOSITORY_BASE_REQUIRED,
    );
    try std.testing.expectEqual(@as(i64, 0), try life.pendingCount(env.h, FLEET_WRITE_LEGACY));
    try expectNoLeaseFor(conn, FLEET_WRITE_LEGACY, refused_id);
    try expectCount(conn, "SELECT count(*) FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid AND event_id = $2", .{ FLEET_WRITE_LEGACY, refused_id }, 0);
    try expectCount(conn, "SELECT count(*) FROM billing.usage_ledger WHERE event_id = $1", .{refused_id}, 0);

    {
        var detail_query = PgQuery.from(try conn.query(
            "SELECT failure_detail FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
            .{ FLEET_WRITE_LEGACY, refused_id },
        ));
        defer detail_query.deinit();
        const detail_row = try detail_query.next() orelse return error.EventRowMissing;
        try std.testing.expectEqualStrings(event_rows.DETAIL_REPOSITORY_BASE_REQUIRED, try detail_row.get([]const u8, 0));
    }

    _ = try conn.exec("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid", .{ FLEET_WRITE_LEGACY, CONFIG_WRITE_NO_GATES });
    const corrected_id = try life.publishEvent(env.h, FLEET_WRITE_LEGACY);
    defer env.h.queue.alloc.free(corrected_id);
    try std.testing.expect(!try life.pollLease(env.h));
    try life.expectRow(conn, FLEET_WRITE_LEGACY, corrected_id, event_rows.STATUS_RECEIVED, "");
    try expectCount(conn, "SELECT count(*) FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid AND event_id = $2", .{ FLEET_WRITE_LEGACY, corrected_id }, 1);
}

test "test_write_kind_park_records_the_row_the_mint_reads" {
    // Dimension 1.3 — the park's DURABLE half, which every other test in this
    // file is blind to. The release tests resolve through Redis, so a park that
    // wrote a malformed row (or none at all) leaves them green while every
    // production write mint refuses forever: the mint reads THIS row and
    // nothing else, and it reads three fields the park is the only writer of.
    var env = life.setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    defer redis_fleet.purgeFleetRedisState(&env.h.queue, FLEET_WRITE_ROW) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});

    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try life.seedFleetWithConfig(conn, FLEET_WRITE_ROW, "write-kind-row", CONFIG_WRITE_NO_GATES);

    const event_id = try life.publishEvent(h, FLEET_WRITE_ROW);
    defer h.queue.alloc.free(event_id);
    try std.testing.expect(!try life.pollLease(h));

    // Keyed by (fleet, event) exactly as the mint keys it: a park that recorded
    // the wrong event id — or none — returns no row here and no token there.
    var q = PgQuery.from(try conn.query(
        \\SELECT gate_kind, status, stated_binding::text, spend_count, spend_ceiling
        \\FROM core.fleet_approval_gates
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ FLEET_WRITE_ROW, event_id }));
    defer q.deinit();
    const row = try q.next() orelse return error.ParkedGateRowMissing;

    try std.testing.expectEqualStrings(gate_constants.GATE_KIND_REPOSITORY_WRITE, try row.get([]const u8, 0));
    try std.testing.expectEqualStrings(approval_gate.GateStatus.pending.toSlice(), try row.get([]const u8, 1));

    // The reach the card stated, compared the way the mint compares it. A
    // swapped insert parameter puts the event id in this column instead, which
    // no comparison against the fleet's binding can accept.
    const stated = try row.get(?[]const u8, 2) orelse return error.StatedBindingMissing;
    try std.testing.expect(binding_json.matches(ALLOC, stated, .{
        .repositories = &REPOS_BOUND,
        .access = .write,
        .base_branch = REPOSITORY_BASE,
    }));
    try std.testing.expectEqual(@as(i64, 0), (try row.get(?i64, 3)).?);
    try std.testing.expectEqual(gate_constants.REPOSITORY_WRITE_SPEND_CEILING, (try row.get(?i64, 4)).?);
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
    try runParkApproveRelease(&env, FLEET_GATED_CRED, false);
}
