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
const integration = @import("../../../credentials/integration.zig");
const CredentialBroker = @import("../../../credentials/broker.zig");
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;
const grant_lookup = @import("../../../state/integration_grant_lookup.zig");
const cred_testing = @import("../../../credentials/testing.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

const GrantStatus = grant_lookup.GrantStatus;

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
const GRANT_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1f01";
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
const INTEGRATION_GITHUB = "github";
const INTEGRATION_ZOHO = "zoho";
// The token the FakeGitHub exchange returns on a successful installation mint.
const GITHUB_MINTED = "ghs_minted";

// Rotated-refresh write-back fixtures: the seeded refresh token, the rotated
// one the fake token endpoint returns, and the vendor response bodies.
const RT_SEEDED = "rt_seeded_0";
const RT_ROTATED = "rt_rotated_1";
const ZOHO_ACCESS_1 = "at_minted_1";
const ZOHO_ACCESS_2 = "at_minted_2";
const ROTATING_RESP = "{\"access_token\":\"" ++ ZOHO_ACCESS_1 ++ "\",\"expires_in\":3600,\"refresh_token\":\"" ++ RT_ROTATED ++ "\"}";
const ECHO_RESP = "{\"access_token\":\"" ++ ZOHO_ACCESS_1 ++ "\",\"expires_in\":3600,\"refresh_token\":\"" ++ RT_SEEDED ++ "\"}";
const NO_ROTATE_RESP = "{\"access_token\":\"" ++ ZOHO_ACCESS_2 ++ "\",\"expires_in\":3600}";
// Sentinel for "this row was never rewritten": the row's updated_at is pinned
// to this after seeding; any write-back would stamp wall-clock now (≫ this).
const PINNED_UPDATED_AT_MS: i64 = 12345;

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

/// Upsert the fleet's grant row for `service` at the given status — the grant
/// gate reads it before any vault load (on-demand integrations only). Idempotent
/// across the suite's shared-id reruns.
fn setGrantStatus(conn: *pg.Conn, fleet_id: []const u8, service: []const u8, status: GrantStatus) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (uid, grant_id, fleet_id, service, status, requested_at, requested_reason)
        \\VALUES ($1::uuid, $1, $2::uuid, $3, $4, 0, 'mint integration test')
        \\ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status
    , .{ GRANT_OWNER, fleet_id, service, status.toSlice() });
}

/// Store a `static` integration handle `{integration, token}` at (workspace, key)
/// — the vault row the mint handler loads and hands to the broker.
fn seedStaticHandle(conn: *pg.Conn, workspace_id: []const u8, token: []const u8) !void {
    const handle = try std.fmt.allocPrint(ALLOC, "{{\"integration\":\"static\",\"token\":\"{s}\"}}", .{token});
    defer ALLOC.free(handle);
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, INTEGRATION_STATIC, handle);
}

/// Store a `github` App-installation handle — the shape the connect
/// callback writes; the broker mints an installation token from it.
fn seedGithubHandle(conn: *pg.Conn, workspace_id: []const u8) !void {
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, INTEGRATION_GITHUB, "{\"integration\":\"github\",\"installation_id\":\"42\"}");
}

fn mintBodyFor(lease_id: []const u8, integration_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "{{\"lease_id\":\"{s}\",\"integration\":\"{s}\"}}", .{ lease_id, integration_id });
}

fn githubMintBody(lease_id: []const u8) ![]u8 {
    return mintBodyFor(lease_id, INTEGRATION_GITHUB);
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn teardown(conn: *pg.Conn) void {
    dropWriteBackBlock(conn); // residue from an aborted write-back-failure test run
    execIgnore(conn, "DELETE FROM core.integration_grants WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ FLEET_OWNER, FLEET_FOREIGN });
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id IN ($1::uuid, $2::uuid, $3::uuid)", .{ LEASE_OWNER, LEASE_FOREIGN, LEASE_STALE });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_OWNER, RUNNER_ATTACKER });
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{WORKSPACE_OWNER});
    execIgnore(conn, "DELETE FROM vault.secrets WHERE workspace_id = $1", .{WORKSPACE_FOREIGN});
    base.teardownFleets(conn, WORKSPACE_OWNER);
    base.teardownWorkspace(conn, WORKSPACE_OWNER);
    base.teardownFleets(conn, WORKSPACE_FOREIGN);
    base.teardownWorkspace(conn, WORKSPACE_FOREIGN);
}

fn mintBody(lease_id: []const u8) ![]u8 {
    return mintBodyFor(lease_id, INTEGRATION_STATIC);
}

/// Store a zoho refresh handle — the shape the connect callback writes; the
/// broker mints fresh access tokens from it via the oauth2_refresh strategy.
fn seedZohoHandle(conn: *pg.Conn, workspace_id: []const u8, refresh_token: []const u8) !void {
    const handle = try std.fmt.allocPrint(
        ALLOC,
        "{{\"integration\":\"{s}\",\"refresh_token\":\"{s}\",\"access_token\":\"at_seeded\",\"expires_at_ms\":1,\"accounts_base\":\"https://accounts.test\",\"label\":\"test-dc\"}}",
        .{ INTEGRATION_ZOHO, refresh_token },
    );
    defer ALLOC.free(handle);
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, INTEGRATION_ZOHO, handle);
}

/// The vaulted zoho handle's current refresh_token, duped for the caller.
fn vaultRefreshToken(conn: *pg.Conn, workspace_id: []const u8) ![]u8 {
    var parsed = try vault.loadJson(ALLOC, conn, workspace_id, INTEGRATION_ZOHO);
    defer parsed.deinit();
    const rt = switch (parsed.value.object.get(integration.FIELD_REFRESH_TOKEN).?) {
        .string => |s| s,
        else => return error.TestUnexpectedResult,
    };
    return ALLOC.dupe(u8, rt);
}

/// The vault row's updated_at — the write-back detector (any store rewrites it).
fn vaultUpdatedAt(conn: *pg.Conn, workspace_id: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT updated_at FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2
    , .{ workspace_id, INTEGRATION_ZOHO }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return try row.get(i64, 0);
}

fn pinVaultUpdatedAt(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\UPDATE vault.secrets SET updated_at = $1 WHERE workspace_id = $2::uuid AND key_name = $3
    , .{ PINNED_UPDATED_AT_MS, workspace_id, INTEGRATION_ZOHO });
}

// Failure injection for the write-back path: a scoped trigger that rejects any
// rewrite of the owner workspace's vault rows. The mint's vault LOAD (a SELECT)
// is untouched; only the post-mint persist hits it — deterministic, no timing.
const CREATE_BLOCK_FN =
    \\CREATE OR REPLACE FUNCTION test_block_vault_writeback() RETURNS trigger AS $fn$
    \\BEGIN RAISE EXCEPTION 'vault write blocked by test'; END
    \\$fn$ LANGUAGE plpgsql
;
const CREATE_BLOCK_TRIGGER = std.fmt.comptimePrint(
    \\CREATE TRIGGER test_block_vault_writeback BEFORE INSERT OR UPDATE ON vault.secrets
    \\FOR EACH ROW WHEN (NEW.workspace_id = '{s}'::uuid) EXECUTE FUNCTION test_block_vault_writeback()
, .{WORKSPACE_OWNER});
const DROP_BLOCK_TRIGGER = "DROP TRIGGER IF EXISTS test_block_vault_writeback ON vault.secrets";
const DROP_BLOCK_FN = "DROP FUNCTION IF EXISTS test_block_vault_writeback()";

fn dropWriteBackBlock(conn: *pg.Conn) void {
    execIgnore(conn, DROP_BLOCK_TRIGGER, .{});
    execIgnore(conn, DROP_BLOCK_FN, .{});
}

/// Teardown under a freshly-acquired connection (a `defer` cannot `return`, so the
/// acquire/release lives here rather than inline).
fn cleanupAll(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    teardown(conn);
}

test "integration: test_mint_scoped_to_lease_workspace" {
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
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try base.seedFleet(conn, FLEET_FOREIGN, WORKSPACE_FOREIGN, "cred-foreign", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedRunner(conn, RUNNER_ATTACKER, TOKEN_ATTACKER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try seedLease(conn, LEASE_FOREIGN, RUNNER_ATTACKER, FLEET_FOREIGN, WORKSPACE_FOREIGN);
        // Seed ONLY the foreign workspace's handle for now: the first mint below
        // proves the owner's missing-handle path, then we seed the owner's.
        // No grant row: `static` is not on-demand, so the grant gate does not
        // apply — this suite also proves static mints without a grant.
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

test "integration: test_mint_rejects_cancelled_or_expired_lease" {
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
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        // Only the lifecycle differs between the two leases. `static` is
        // ungated, so a 404 can only be the live-lease gate.
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

test "integration: test_mint_requires_approved_grant" {
    // Grant-gate dimension 2.1 — the grant gate precedes everything vault-shaped:
    // a live lease + a CONNECTED github handle still refuse (403 UZ-GRANT-001)
    // when the fleet holds no approved grant, and a PENDING grant is equally
    // refused. The fake GitHub exchange would return GITHUB_MINTED on a real
    // mint; asserting its absence proves the broker is never reached (refusal
    // precedes broker.mint). github is on-demand, so the gate applies.
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var gh = cred_testing.FakeGitHub{ .alloc = ALLOC };
    defer gh.deinit();
    var metrics = cred_testing.RecordingMetrics{};
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&gh, &metrics));
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try seedGithubHandle(conn, WORKSPACE_OWNER); // connected, but ungranted
        // Deliberately NO grant row.
    }
    defer cleanupAll(h);

    // (1) No grant row at all → 403 UZ-GRANT-001; broker never called (no token).
    {
        const body = try githubMintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_GRANT_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }

    // (2) A PENDING grant is not an approval — still 403, still no token.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .pending);
    }
    {
        const body = try githubMintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_GRANT_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }

    // The broker was never reached across either refusal.
    try std.testing.expectEqual(@as(usize, 0), gh.calls);
}

test "integration: test_mint_rechecks_revoked_grant" {
    // Grant-gate dimension 2.2 — mint-time re-check, not just lease-time: the
    // SAME live lease mints an installation token while approved, refuses after
    // a revoke, and mints again after re-approval. Proves grant authority is
    // read fresh per mint (a revoke mid-lease bites on the very next request).
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var gh = cred_testing.FakeGitHub{ .alloc = ALLOC };
    defer gh.deinit();
    var metrics = cred_testing.RecordingMetrics{};
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&gh, &metrics));
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try seedGithubHandle(conn, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
    }
    defer cleanupAll(h);

    // (1) Approved → 200 with the minted installation token.
    {
        const body = try githubMintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(GITHUB_MINTED));
    }

    // (2) Revoked mid-lease → the next mint refuses; no token bytes leak.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .revoked);
    }
    {
        const body = try githubMintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_GRANT_NOT_FOUND));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }

    // (3) Re-approved → minting resumes on the same lease.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
    }
    {
        const body = try githubMintBody(LEASE_OWNER);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(GITHUB_MINTED));
    }
}

test "integration: test_mint_persists_rotated_refresh_token" {
    // Rotated-refresh write-back, end to end: a rotating provider (Atlassian
    // three-legged OAuth semantics) returns a NEW refresh token on the exchange;
    // the handler persists it to the vaulted handle, and the NEXT cold mint
    // posts the persisted token — instead of re-posting the dead one and dying
    // invalid_grant (the pre-fix roughly-hourly forced reconnect).
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var vendor = cred_testing.FakeGitHub{ .alloc = ALLOC, .status = 200, .resp_body = ROTATING_RESP };
    defer vendor.deinit();
    var metrics = cred_testing.RecordingMetrics{};
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&vendor, &metrics));
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_ZOHO, .approved);
        try seedZohoHandle(conn, WORKSPACE_OWNER, RT_SEEDED);
    }
    defer cleanupAll(h);

    // (1) Cold mint: 200 with the fresh access token; the exchange posted the
    // SEEDED refresh token, and the response's rotated one is vaulted.
    {
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_ZOHO);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(ZOHO_ACCESS_1));
        // The refresh token never rides the wire response (VLT).
        try std.testing.expect(!resp.bodyContains(RT_ROTATED));
        try std.testing.expect(std.mem.indexOf(u8, vendor.body, RT_SEEDED) != null);
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER);
        defer ALLOC.free(rt);
        try std.testing.expectEqualStrings(RT_ROTATED, rt);
    }

    // (2) A SECOND cold mint (fresh broker → empty cache) posts the PERSISTED
    // rotated token and succeeds — the invalid_grant loop is structurally gone.
    var vendor2 = cred_testing.FakeGitHub{ .alloc = ALLOC, .status = 200, .resp_body = NO_ROTATE_RESP };
    defer vendor2.deinit();
    var metrics2 = cred_testing.RecordingMetrics{};
    var broker2 = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&vendor2, &metrics2));
    defer broker2.deinit();
    h.ctx.broker = &broker2;
    {
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_ZOHO);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(ZOHO_ACCESS_2));
        try std.testing.expect(std.mem.indexOf(u8, vendor2.body, RT_ROTATED) != null);
        try std.testing.expect(std.mem.indexOf(u8, vendor2.body, RT_SEEDED) == null);
    }
}

test "integration: test_mint_no_rotation_leaves_handle_unchanged" {
    // A provider that echoes the posted refresh token (or omits it) rotated
    // nothing — the vault row must not be rewritten at all. The row's
    // updated_at is pinned to a sentinel after seeding: any write-back would
    // stamp wall-clock now, so sentinel-unchanged proves zero writes.
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var vendor = cred_testing.FakeGitHub{ .alloc = ALLOC, .status = 200, .resp_body = ECHO_RESP };
    defer vendor.deinit();
    var metrics = cred_testing.RecordingMetrics{};
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&vendor, &metrics));
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_ZOHO, .approved);
        try seedZohoHandle(conn, WORKSPACE_OWNER, RT_SEEDED);
        try pinVaultUpdatedAt(conn, WORKSPACE_OWNER);
    }
    defer cleanupAll(h);

    {
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_ZOHO);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(ZOHO_ACCESS_1));
    }
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(PINNED_UPDATED_AT_MS, try vaultUpdatedAt(conn, WORKSPACE_OWNER));
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER);
        defer ALLOC.free(rt);
        try std.testing.expectEqualStrings(RT_SEEDED, rt);
    }
}

test "integration: test_write_back_failure_logged_not_fatal" {
    // The write-back is best-effort: when the vault persist fails (here: a
    // scoped trigger rejects the rewrite — standing in for pool pressure or a
    // DB fault at persist time), the mint still returns 200 with the token.
    // The warn branch (`refresh_rotated` outcome=failed) is code-audited via
    // the spec's grep rubric — the harness has no runtime log capture.
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var vendor = cred_testing.FakeGitHub{ .alloc = ALLOC, .status = 200, .resp_body = ROTATING_RESP };
    defer vendor.deinit();
    var metrics = cred_testing.RecordingMetrics{};
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, cred_testing.brokerDeps(&vendor, &metrics));
    defer broker.deinit();
    h.ctx.broker = &broker;

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        teardown(conn); // clear any residue from an aborted prior run
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", "{}", "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_ZOHO, .approved);
        try seedZohoHandle(conn, WORKSPACE_OWNER, RT_SEEDED);
        // Arm the failure AFTER seeding: reads still work, rewrites raise.
        _ = try conn.exec(CREATE_BLOCK_FN, .{});
        _ = try conn.exec(CREATE_BLOCK_TRIGGER, .{});
    }
    defer cleanupAll(h);

    // The mint succeeds and returns the token even though the persist failed.
    {
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_ZOHO);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.ok);
        try std.testing.expect(resp.bodyContains(ZOHO_ACCESS_1));
    }
    // The blocked persist left the seeded token in place (the honest bound:
    // this workspace eats one forced reconnect later, not a failed request now).
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        dropWriteBackBlock(conn); // disarm before reading/cleanup
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER);
        defer ALLOC.free(rt);
        try std.testing.expectEqualStrings(RT_SEEDED, rt);
    }
}
