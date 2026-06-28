// Integration tests for POST /v1/runners/me/credentials/mint (M102 §3, Dimension
// 3.2) — the on-demand credential-mint endpoint, driven end-to-end over the live
// test DB + the real runner-bearer middleware + a real CredentialBroker. The
// broker runs the production `static` integration, so the mint is deterministic
// (the handle carries the token; no network / App key / RS256 signer on the path).
//
// The spine is Invariant 2 (workspace scope): the wire carries NO workspace — it
// is derived from the lease, scoped to the presenting runner
// (`fleet.runner_leases WHERE id = $lease_id AND runner_id = $runner_id`). So a
// prompt-injected child has nothing to forge: a foreign or stale lease_id resolves
// to no row → 404, never another tenant's workspace. `test_mint_scoped_to_lease_workspace`
// proves all three faces of that contract in one live setup:
//   * a connected lease with no handle is a typed not-connected (UZ-CRED-001),
//   * the owner mints from ITS OWN workspace handle (the token VALUE, not just its
//     presence, distinguishes the owner workspace from a sibling — the positive),
//   * the owner runner cannot mint on another runner's lease (UZ-RUN-006, the IDOR
//     negative) and no foreign token leaks.
//
// Requires TEST_DATABASE_URL (TestHarness.start → SkipZigTest otherwise) and the
// test KEK (setTestKek) for the vault handle round-trip. Per the harness contract,
// cleanup runs in the test body (deferred cleanup leaks pool connections).

const std = @import("std");
const pg = @import("pg");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const ec = @import("../../../errors/error_registry.zig");
const api_key = @import("../../../auth/api_key.zig");
const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const crypto_primitives = @import("../../../secrets/crypto_primitives.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const integration = @import("../../../credentials/integration.zig");
const CredentialBroker = @import("../../../credentials/broker.zig");
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;

const ALLOC = std.testing.allocator;

// Distinct UUIDv7 literals (version nibble 7) — no collision with sibling
// runner-handler integration suites.
const WORKSPACE_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
const WORKSPACE_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012";
const RUNNER_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a01";
const RUNNER_ATTACKER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a02";
const FLEET_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c01";
const FLEET_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c02";
const LEASE_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e01";
const LEASE_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e02";
const LEASE_STALE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e03";
// A lease_expires_at in the distant past (1970) — guaranteed < the handler's
// wall-clock now, so the live-lease gate must reject it regardless of run date.
const PAST_MS: i64 = 1000;
const EVENT_ID = "evt-cred-mint-1";
const NOW_MS: i64 = 1_900_000_000_000;

// Runner bearers — the raw tokens; their SHA-256-hex digests are the stored
// `token_hash` the runnerBearer lookup gates on (admin_state active).
const TOKEN_OWNER = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "o" ** 60;
const TOKEN_ATTACKER = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "a" ** 60;

// Per-workspace static-handle tokens — distinct so a wrong-workspace resolution
// is caught by VALUE, not merely by absence (the scope proof).
const SENTINEL_OWNER = "ghs_owner_workspace_token";
const SENTINEL_FOREIGN = "ghs_foreign_workspace_token";

const INTEGRATION_STATIC = "static";

// SAFETY: populated by configureRegistry before the runner_bearer middleware
// (and thus the lookup) ever reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry });
}

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, raw_token: []const u8) !void {
    const hash = api_key.sha256Hex(raw_token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'cred-mint-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, hash[0..] });
}

/// Seed a lease binding `runner_id` → `workspace_id` with an explicit
/// `lease_expires_at` + `status`, so a test can assert the mint handler's
/// live-lease gate (active + unexpired) rejects a cancelled/expired row.
fn seedLeaseFull(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, workspace_id: []const u8, lease_expires_at: i64, status: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test',
        \\        'chat', '{"message":"hi"}', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
        \\        5, $7, $8, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, runner_id, fleet_id, workspace_id, base.TEST_TENANT_ID, EVENT_ID, lease_expires_at, status });
}

/// Seed an active, unexpired lease binding `runner_id` → `workspace_id`.
fn seedLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, workspace_id: []const u8) !void {
    return seedLeaseFull(conn, lease_id, runner_id, fleet_id, workspace_id, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_ACTIVE);
}

/// Store a `static` integration handle `{integration, token}` at (workspace, key)
/// — the vault row the mint handler loads and hands to the broker.
fn seedStaticHandle(conn: *pg.Conn, workspace_id: []const u8, token: []const u8) !void {
    const key_name = try credential_key.allocKeyName(ALLOC, INTEGRATION_STATIC);
    defer ALLOC.free(key_name);
    const handle = try std.fmt.allocPrint(ALLOC, "{{\"integration\":\"static\",\"token\":\"{s}\"}}", .{token});
    defer ALLOC.free(handle);
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, key_name, handle);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id IN ($1::uuid, $2::uuid, $3::uuid)", .{ LEASE_OWNER, LEASE_FOREIGN, LEASE_STALE });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_OWNER, RUNNER_ATTACKER });
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{WORKSPACE_OWNER});
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{WORKSPACE_FOREIGN});
    base.teardownWorkspace(conn, WORKSPACE_OWNER);
    base.teardownWorkspace(conn, WORKSPACE_FOREIGN);
}

fn mintBody(lease_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "{{\"lease_id\":\"{s}\",\"integration\":\"{s}\"}}", .{ lease_id, INTEGRATION_STATIC });
}

/// Teardown under a freshly-acquired connection (a `defer` cannot `return`, so the
/// acquire/release lives here rather than inline).
fn cleanupAll(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    teardown(conn);
}

test "test_mint_scoped_to_lease_workspace" {
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // A real broker over the PRODUCTION registry. The `static` integration mints
    // deterministically from the vault handle (no network / App key / signer), so
    // `nullDeps()` suffices. Injected onto the live Context (the harness's Option-C
    // convention: set the field on `&h.ctx` before the request).
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedWorkspace(conn, WORKSPACE_FOREIGN);
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedRunner(conn, RUNNER_ATTACKER, TOKEN_ATTACKER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try seedLease(conn, LEASE_FOREIGN, RUNNER_ATTACKER, FLEET_FOREIGN, WORKSPACE_FOREIGN);
        // Seed ONLY the foreign workspace's handle for now: the first mint below
        // proves the owner's missing-handle path, then we seed the owner's.
        try seedStaticHandle(conn, WORKSPACE_FOREIGN, SENTINEL_FOREIGN);
    }
    defer cleanupAll(h);

    // (1) A connected lease whose workspace has no integration handle is a typed
    // not-connected — never a silent 200, never a token from a sibling workspace.
    {
        const body = try mintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
        try std.testing.expect(resp.bodyContains(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED));
        try std.testing.expect(!resp.bodyContains(SENTINEL_FOREIGN));
    }

    // Now connect the OWNER workspace.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedStaticHandle(conn, WORKSPACE_OWNER, SENTINEL_OWNER);
    }

    // (2) The owner runner mints on its own lease → 200, and the token is the
    // OWNER workspace's handle value, NEVER the foreign one. The wire never named
    // a workspace; it was derived from the lease (Invariant 2, the positive face).
    {
        const body = try mintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(SENTINEL_OWNER));
        try std.testing.expect(!resp.bodyContains(SENTINEL_FOREIGN));
    }

    // (3) IDOR: the owner runner presents the ATTACKER's lease_id. That lease
    // exists (in the foreign workspace) but is not owned by this runner, so the
    // runner-scoped query resolves no row → 404 UZ-RUN-006. The mint never crosses
    // into the foreign workspace and no foreign token leaks (Invariant 2, negative).
    {
        const body = try mintBody(LEASE_FOREIGN);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
        try std.testing.expect(resp.bodyContains(ec.ERR_RUN_LEASE_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(SENTINEL_FOREIGN));
        try std.testing.expect(!resp.bodyContains(SENTINEL_OWNER));
    }
}

test "test_mint_rejects_cancelled_or_expired_lease" {
    // Mint authority is bound to the lease's lifetime, not the runner's. The lease
    // lookup gates on `status = active AND lease_expires_at > now`, so a runner that
    // legitimately held a lease cannot mint once that lease is no longer live —
    // neither a cancelled lease (reclaim flips status → 'expired') nor a lapsed TTL
    // (status still 'active' but past expiry) resolves a workspace → 404
    // ERR_RUN_LEASE_NOT_FOUND. This closes the cancel-vs-mint race and bounds a
    // compromised runner replaying a stale lease_id past kill. Both leases belong to
    // RUNNER_OWNER in WORKSPACE_OWNER and the owner handle IS seeded, so a 404 can
    // only be the live-lease gate — never a missing-handle or wrong-runner masquerade.
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        // Only the lifecycle differs between the two leases.
        try seedLeaseFull(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, PAST_MS, protocol.RUNNER_LEASE_STATUS_ACTIVE);
        try seedLeaseFull(conn, LEASE_STALE, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_EXPIRED);
        try seedStaticHandle(conn, WORKSPACE_OWNER, SENTINEL_OWNER);
    }
    defer cleanupAll(h);

    // (A) status 'active' but EXPIRED by time (TTL lapsed) → 404, no token minted.
    {
        const body = try mintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
        try std.testing.expect(resp.bodyContains(ec.ERR_RUN_LEASE_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(SENTINEL_OWNER));
    }

    // (B) future expiry but status 'expired' (the cancel/reclaim outcome) → 404, no token.
    {
        const body = try mintBody(LEASE_STALE);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.not_found);
        try std.testing.expect(resp.bodyContains(ec.ERR_RUN_LEASE_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(SENTINEL_OWNER));
    }
}
