//! A grant is born at install, and approving its gate arms it.
//!
//! The defect these pin: `core.integration_grants` is the enforcement spine for
//! credential minting, and no production statement used to be able to create a row
//! an internally-installed fleet could reach. It survived a whole milestone
//! because every test that exercised minting hand-seeded an APPROVED grant, so
//! origination itself was never executed. Neither test below writes a grant or
//! a gate row — both come out of the real install handler, or they do not exist.
//!
//! The credential is deliberately not named after its service: the bundle
//! declares `gh_bot`, and the grant must still come out keyed on `github`. That
//! is what proves the service is read from the shared classifier
//! (`secrets_resolve.mintableId`, the same one the lease path enforces on)
//! rather than from whatever the operator happened to call the secret.
//!
//! Requires TEST_DATABASE_URL and Redis — skipped gracefully otherwise.

const std = @import("std");
const pg = @import("pg");
const shared = @import("common");
const clock = shared.clock;

const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const id_format = @import("../../../types/id_format.zig");
const vault = @import("../../../state/vault.zig");
const crypto_primitives = @import("../../../secrets/crypto_primitives.zig");
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");
const gate_sql = @import("../../../fleet_runtime/sql.zig");
const grant_lookup = @import("../../../state/integration_grant_lookup.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const create_grants = @import("create_grants.zig");
const github_sql = @import("../connectors/github/sql.zig");

const ALLOC = std.testing.allocator;

// The token fixture's own tenant/workspace, shared with every other HTTP suite
// — `authorizeWorkspace` reads the workspace out of the JWT claims, so a
// private id here is a 403, not an isolated fixture. Isolation comes from the
// fleet ids (minted by the handler, purged per test) and from the repository
// name below, which no other suite routes on.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const TOKEN_ADMIN = scope_fixtures.TENANT_ADMIN;

/// A mintable handle stored under a name that is NOT its service.
const CREDENTIAL_MINTABLE = "gh_bot";
const HANDLE_MINTABLE = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
/// A stored value with no `integration` field — static, never mintable.
const CREDENTIAL_STATIC = "plain_token";
const HANDLE_STATIC = "{\"api_token\":\"static-not-a-handle\"}";

/// Owned by this suite alone. The ingress read is scoped by workspace, and the
/// workspace is shared, so the repository is what keeps a neighbouring suite's
/// fleet out of this file's target counts.
const REPOSITORY = "acme/grant-origination";
const EVENT = "pull_request";
/// The ingress read's fan-out ceiling is not what these tests measure — one
/// target is the entire assertion, so any limit above it answers the same
/// question. Production's real ceiling is pinned by the ingress suite.
const TARGET_LIMIT: i64 = 8;

const TEMPLATE_REQUIREMENTS =
    \\{"credentials":[],"tools":[],"network_hosts":[],"support_files":[],"trigger_present":true}
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn seedWorkspace(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'GrantOriginationTest', $2, $2) ON CONFLICT (id) DO NOTHING
    , .{ TEST_TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2, $3) ON CONFLICT (id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now_ms });
}

/// Remove the fleet and everything cascading off it. The gate rows are
/// append-only by trigger, so the delete opts into the same transaction-scoped
/// bypass the production hard-purge paths use — a bare DELETE would raise and
/// strand the fixture for the next run.
fn purgeFleet(conn: *pg.Conn, fleet_id: []const u8) void {
    _ = conn.exec("BEGIN", .{}) catch |err| return warnPurge("begin", err);
    // Past BEGIN the transaction closes on every path. This connection goes
    // straight back to the pool, and one left open would poison whichever test
    // acquires it next — a far worse failure than the purge it was cleaning up
    // after. On an aborted transaction Postgres treats COMMIT as ROLLBACK,
    // which is exactly what a failed DELETE wants.
    defer {
        _ = conn.exec("COMMIT", .{}) catch |err| warnPurge("commit", err);
    }
    _ = conn.exec(gate_sql.SET_GATE_PURGE_BYPASS_SQL, .{}) catch |err| return warnPurge("bypass", err);
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{fleet_id}) catch |err| warnPurge("delete", err);
}

fn warnPurge(step: []const u8, err: anyerror) void {
    std.log.warn("grant-origination fixture purge failed at {s}: {s}", .{ step, @errorName(err) });
}

fn skillFor(name: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC,
        \\---
        \\name: {s}
        \\description: proves install originates the grant and raises its gate
        \\version: 0.1.0
        \\---
        \\Body.
    , .{name});
}

fn triggerFor(name: []const u8, credential: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC,
        \\---
        \\name: {s}
        \\x-agentsfleet:
        \\  triggers:
        \\    - type: webhook
        \\      source: github
        \\      events:
        \\        - {s}
        \\      repositories:
        \\        - {s}
        \\  tools:
        \\    - agentmail
        \\  credentials:
        \\    - {s}
        \\  budget:
        \\    daily_dollars: 1.0
        \\---
    , .{ name, EVENT, REPOSITORY, credential });
}

fn seedTemplate(conn: *pg.Conn, name: []const u8, skill_md: []const u8, trigger_md: []const u8) ![]const u8 {
    const id = try id_format.generateFleetLibraryId(ALLOC);
    errdefer ALLOC.free(id);
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(skill_md, &digest, .{});
    const content_hash = std.fmt.bytesToHex(digest, .lower);
    _ = try conn.exec(
        \\INSERT INTO core.tenant_fleet_library
        \\  (id, workspace_id, name, description, source_kind, source_ref, visibility,
        \\   content_hash, skill_markdown, trigger_markdown, support_files_json,
        \\   requirements_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, 'grant origination fixture', 'upload', 'unit', 'tenant',
        \\        $4, $5, $6, '[]'::jsonb, $7::jsonb, 0, 0)
    , .{ id, TEST_WORKSPACE_ID, name, &content_hash, skill_md, trigger_md, TEMPLATE_REQUIREMENTS });
    return id;
}

/// Install one fleet through the real handler and return its owned id.
///
/// The name carries a per-run stamp for two reasons the database enforces: a
/// fleet name is unique per workspace, and a library row is unique on
/// (workspace, content hash) — which the name feeds. Without it, a run that
/// died before its purge would 409 every run after it.
///
/// `install_wg.wait()` is the only synchronization: the install progression is
/// a detached worker, and waiting on its group is what the lifecycle suite
/// established as the deterministic alternative to sleeping. It also means the
/// row has reached `active` by the time this returns, which the ingress read
/// below requires.
fn installFleet(h: *TestHarness, conn: *pg.Conn, label: []const u8, credential: []const u8) ![]const u8 {
    const name = try std.fmt.allocPrint(ALLOC, "{s}-{d}", .{ label, clock.nowMillis() });
    defer ALLOC.free(name);
    const skill_md = try skillFor(name);
    defer ALLOC.free(skill_md);
    const trigger_md = try triggerFor(name, credential);
    defer ALLOC.free(trigger_md);

    const template_id = try seedTemplate(conn, name, skill_md, trigger_md);
    defer ALLOC.free(template_id);

    const body = try std.fmt.allocPrint(ALLOC, "{{\"tenant_library_id\":\"{s}\"}}", .{template_id});
    defer ALLOC.free(body);
    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);

    const r = try (try (try h.post(url).bearer(TOKEN_ADMIN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    const fleet_id = try ALLOC.dupe(u8, parsed.value.object.get("fleet_id").?.string);
    errdefer ALLOC.free(fleet_id);

    h.install_wg.wait();
    return fleet_id;
}

fn countGrants(conn: *pg.Conn, fleet_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT count(*)::bigint FROM core.integration_grants WHERE fleet_id = $1::uuid",
        .{fleet_id},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoRow;
    return row.get(i64, 0);
}

const Grant = struct { service: []const u8, status: []const u8, reason: []const u8, approved_at: ?i64, revoked_at: ?i64 };

/// The one grant this fleet holds. Slices borrow the live result set, so the
/// caller must finish reading before `q.deinit()` — hence the callback shape.
fn withGrant(conn: *pg.Conn, fleet_id: []const u8, ctx: anytype, comptime run: fn (@TypeOf(ctx), Grant) anyerror!void) !void {
    var q = PgQuery.from(try conn.query(
        \\SELECT service, status, requested_reason, approved_at, revoked_at
        \\FROM core.integration_grants WHERE fleet_id = $1::uuid
    , .{fleet_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoGrantRow;
    try run(ctx, .{
        .service = try row.get([]const u8, 0),
        .status = try row.get([]const u8, 1),
        .reason = try row.get([]const u8, 2),
        .approved_at = try row.get(?i64, 3),
        .revoked_at = try row.get(?i64, 4),
    });
}

/// The gate identifier plus the fields origination must have written. `id` is owned by
/// the caller; the rest are asserted here because they die with the result set.
fn readGateId(conn: *pg.Conn, fleet_id: []const u8, service: []const u8, credential: []const u8) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, action_id, gate_kind, status, proposed_action,
        \\       evidence->>'service', evidence->>'credential'
        \\FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid
    , .{fleet_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoGateRow;

    const expected_action_id = try std.fmt.allocPrint(ALLOC, "grant:{s}:{s}", .{ fleet_id, service });
    defer ALLOC.free(expected_action_id);
    try std.testing.expectEqualStrings(expected_action_id, try row.get([]const u8, 1));
    try std.testing.expectEqualStrings(gate_constants.GATE_KIND_INTEGRATION_GRANT, try row.get([]const u8, 2));
    try std.testing.expectEqualStrings("pending", try row.get([]const u8, 3));
    // The prose an operator reads names the SERVICE being authorized...
    try std.testing.expect(std.mem.indexOf(u8, try row.get([]const u8, 4), service) != null);
    // ...and the evidence carries both halves: the key RESOLVE_GATE joins the
    // grant on, and the credential that classified to it.
    try std.testing.expectEqualStrings(service, try row.get([]const u8, 5));
    try std.testing.expectEqualStrings(credential, try row.get([]const u8, 6));

    const id = try ALLOC.dupe(u8, try row.get([]const u8, 0));
    errdefer ALLOC.free(id);
    try std.testing.expect((try q.next()) == null); // exactly one gate, not a stack
    return id;
}

/// Run the App ingress routing read exactly as `ingress/github.zig` binds it,
/// and answer how many fleets it would deliver this webhook to.
fn countIngressTargets(conn: *pg.Conn) !usize {
    var q = PgQuery.from(try conn.query(github_sql.SELECT_APP_INGRESS_TARGETS, .{
        TEST_WORKSPACE_ID,
        fleet_config.FleetStatus.active.toSlice(),
        shared.PROVIDER_GITHUB,
        grant_lookup.GrantStatus.approved.toSlice(),
        REPOSITORY,
        EVENT,
        TARGET_LIMIT,
    }));
    defer q.deinit();
    var count: usize = 0;
    while (try q.next()) |_| count += 1;
    return count;
}

fn resolveGate(h: *TestHarness, gate_id: []const u8, decision: []const u8) !void {
    const url = try std.fmt.allocPrint(
        ALLOC,
        "/v1/workspaces/{s}/approvals/{s}:{s}",
        .{ TEST_WORKSPACE_ID, gate_id, decision },
    );
    defer ALLOC.free(url);
    const r = try (try (try h.post(url).bearer(TOKEN_ADMIN)).json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
}

// ── Origination: install seeds the grant and raises the gate ────────────────

fn expectPendingGrant(_: void, g: Grant) anyerror!void {
    try std.testing.expectEqualStrings(shared.PROVIDER_GITHUB, g.service);
    try std.testing.expectEqualStrings(grant_lookup.GrantStatus.pending.toSlice(), g.status);
    try std.testing.expectEqualStrings(create_grants.S_DEFAULT_REASON, g.reason);
    // Born undecided: neither domain instant has arrived.
    try std.testing.expect(g.approved_at == null);
    try std.testing.expect(g.revoked_at == null);
}

test "integration: test_install_seeds_pending_grant_and_gate" {
    crypto_primitives.setTestKek();
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);
    try vault.storeJsonPlaintext(ALLOC, conn, TEST_WORKSPACE_ID, CREDENTIAL_MINTABLE, HANDLE_MINTABLE);

    const fleet_id = try installFleet(h, conn, "grant-seeded", CREDENTIAL_MINTABLE);
    defer ALLOC.free(fleet_id);
    defer purgeFleet(conn, fleet_id);

    // One grant, pending, keyed on the SERVICE the classifier named — not on
    // `gh_bot`, the name the bundle actually declared.
    try std.testing.expectEqual(@as(i64, 1), try countGrants(conn, fleet_id));
    try withGrant(conn, fleet_id, {}, expectPendingGrant);

    // And exactly one gate asking for it, whose fields readGateId asserts.
    const gate_id = try readGateId(conn, fleet_id, shared.PROVIDER_GITHUB, CREDENTIAL_MINTABLE);
    ALLOC.free(gate_id);
}

test "integration: test_install_seeds_no_grant_for_a_static_credential" {
    // The classifier's other answer. A bundle credential that resolves to a
    // stored value rather than a mintable handle needs no standing
    // authorization, so install must raise nothing — a gate here would be an
    // inbox question no human can act on, for a credential the lease path
    // already ships without asking.
    crypto_primitives.setTestKek();
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);
    try vault.storeJsonPlaintext(ALLOC, conn, TEST_WORKSPACE_ID, CREDENTIAL_STATIC, HANDLE_STATIC);

    const fleet_id = try installFleet(h, conn, "grant-static", CREDENTIAL_STATIC);
    defer ALLOC.free(fleet_id);
    defer purgeFleet(conn, fleet_id);

    try std.testing.expectEqual(@as(i64, 0), try countGrants(conn, fleet_id));
    try std.testing.expectError(error.NoGateRow, readGateId(conn, fleet_id, shared.PROVIDER_GITHUB, CREDENTIAL_STATIC));
}

// ── Resolution: the answer moves the grant, and routing follows ─────────────

fn expectApprovedGrant(_: void, g: Grant) anyerror!void {
    try std.testing.expectEqualStrings(grant_lookup.GrantStatus.approved.toSlice(), g.status);
    try std.testing.expect(g.approved_at != null);
    try std.testing.expect(g.revoked_at == null);
}

fn expectRevokedGrant(_: void, g: Grant) anyerror!void {
    // Revoked, NOT back to pending: nothing re-raises a pending grant, so a
    // denied fleet would sit forever in a state that reads like "awaiting a
    // decision" when the decision has already been made.
    try std.testing.expectEqualStrings(grant_lookup.GrantStatus.revoked.toSlice(), g.status);
    try std.testing.expect(g.approved_at == null);
    try std.testing.expect(g.revoked_at != null);
}

test "integration: test_gate_approval_arms_webhook_routing" {
    crypto_primitives.setTestKek();
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);
    try vault.storeJsonPlaintext(ALLOC, conn, TEST_WORKSPACE_ID, CREDENTIAL_MINTABLE, HANDLE_MINTABLE);

    const fleet_id = try installFleet(h, conn, "grant-armed", CREDENTIAL_MINTABLE);
    defer ALLOC.free(fleet_id);
    defer purgeFleet(conn, fleet_id);

    // Installed, active, and declaring the repository — yet unroutable, because
    // the ingress read inner-joins an APPROVED grant and this one is pending.
    // This zero is the whole defect: there used to be no path to any other
    // number for an internally-installed fleet.
    try std.testing.expectEqual(@as(usize, 0), try countIngressTargets(conn));

    const gate_id = try readGateId(conn, fleet_id, shared.PROVIDER_GITHUB, CREDENTIAL_MINTABLE);
    defer ALLOC.free(gate_id);
    try resolveGate(h, gate_id, gate_constants.GATE_DECISION_APPROVE);

    // One statement moved both rows: the gate answered and the grant armed.
    try withGrant(conn, fleet_id, {}, expectApprovedGrant);
    try std.testing.expectEqual(@as(usize, 1), try countIngressTargets(conn));
}

test "integration: test_gate_denial_revokes_the_grant" {
    crypto_primitives.setTestKek();
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedWorkspace(conn);
    try vault.storeJsonPlaintext(ALLOC, conn, TEST_WORKSPACE_ID, CREDENTIAL_MINTABLE, HANDLE_MINTABLE);

    const fleet_id = try installFleet(h, conn, "grant-denied", CREDENTIAL_MINTABLE);
    defer ALLOC.free(fleet_id);
    defer purgeFleet(conn, fleet_id);

    const gate_id = try readGateId(conn, fleet_id, shared.PROVIDER_GITHUB, CREDENTIAL_MINTABLE);
    defer ALLOC.free(gate_id);
    try resolveGate(h, gate_id, gate_constants.GATE_DECISION_DENY);

    try withGrant(conn, fleet_id, {}, expectRevokedGrant);
    try std.testing.expectEqual(@as(usize, 0), try countIngressTargets(conn));
}
