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
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");
const approval_gate_rt = @import("../../../fleet_runtime/approval_gate.zig");
const approval_gate_db = @import("../../../fleet_runtime/approval_gate_db.zig");
const write_gate = @import("credentials_mint_write_gate.zig");

const GrantStatus = grant_lookup.GrantStatus;

const ALLOC = std.testing.allocator;

// Distinct UUIDv7 literals (version nibble 7) — no collision with sibling
// runner-handler integration suites.
const WORKSPACE_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
const WORKSPACE_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012";
const RUNNER_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a01";
const RUNNER_ATTACKER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a02";
const FLEET_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c01";

/// A fleet config carrying the repository EGRESS binding the GitHub mint scopes
/// its token by (M157 §2).
///
/// Every field here is load-bearing, because the binding reaches the mint only
/// if the WHOLE config parses: `credentials_mint_scope.repositoryBinding`
/// degrades a parse failure to "no binding", and a binding-free GitHub mint
/// fails closed (Dimension 2.4). So an omitted `budget` does not surface as a
/// budget error — it surfaces as `UZ-GH-002 Credential mint failed`, which is
/// why this constant spells out a whole valid fleet rather than the binding
/// alone. `name`, `triggers`, and `budget` are each required by
/// `fleet_runtime/config_parser`; the runtime keys live INSIDE `x-agentsfleet`
/// (one at the top level is rejected outright); and `repositories` +
/// `repository_access` are optional TOGETHER — one without the other is an
/// authoring error, not a half-binding.
const CONFIG_WITH_BINDING =
    \\{"name":"cred-owner","x-agentsfleet":{"triggers":[{"type":"webhook","source":"github"}],"credentials":["github"],"tools":["git"],"budget":{"daily_dollars":1.0},"repositories":["acme/payments"],"repository_access":"write","repository_base":"main"}}
;
/// The same binding, as a slice — a fake GitHub has to state the reach this
/// fleet declared or the mint refuses the token it returns (RULE UFS: one
/// spelling, and the config above is where it is authored).
const FLEET_OWNER_REPOSITORIES = [_][]const u8{"acme/payments"};
const FLEET_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c02";
const LEASE_OWNER = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e01";
const LEASE_FOREIGN = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e02";
const LEASE_STALE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e03";
// Write-gate leases: each keys its OWN event id, because the append-only gates
// table makes rows for a shared event permanent across the whole suite run.
const LEASE_WRITE_UNAPPROVED = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e04";
const LEASE_WRITE_DRIFT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e05";
const LEASE_WRITE_FAILURE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e06";
const LEASE_WRITE_CEILING = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e07";
const LEASE_WRITE_CONCURRENT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1e08";
const EVENT_WRITE_UNAPPROVED = "evt-cred-write-unappr";
const EVENT_WRITE_DRIFT = "evt-cred-write-drift";
const EVENT_WRITE_FAILURE = "evt-cred-write-failure";
const EVENT_WRITE_CEILING = "evt-cred-write-ceiling";
const EVENT_WRITE_CONCURRENT = "evt-cred-write-concurrent";
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
const INTEGRATION_JIRA = "jira";
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
fn seedLeaseFull(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, workspace_id: []const u8, event_id: []const u8, lease_expires_at: i64, status: []const u8) !void {
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
    , .{ lease_id, runner_id, fleet_id, workspace_id, base.TEST_TENANT_ID, event_id, lease_expires_at, status });
}

/// Seed an active, unexpired lease binding `runner_id` → `workspace_id`.
fn seedLease(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, workspace_id: []const u8) !void {
    return seedLeaseFull(conn, lease_id, runner_id, fleet_id, workspace_id, EVENT_ID, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_ACTIVE);
}

/// `seedLease` keyed to a caller-owned event id — the write-gate tests' shape.
fn seedLeaseForEvent(conn: *pg.Conn, lease_id: []const u8, runner_id: []const u8, fleet_id: []const u8, workspace_id: []const u8, event_id: []const u8) !void {
    return seedLeaseFull(conn, lease_id, runner_id, fleet_id, workspace_id, event_id, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_ACTIVE);
}

/// Upsert the fleet's grant row for `service` at the given status — the grant
/// gate reads it before any vault load (on-demand integrations only). Idempotent
/// across the suite's shared-id reruns.
fn setGrantStatus(conn: *pg.Conn, fleet_id: []const u8, service: []const u8, status: GrantStatus) !void {
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (id, fleet_id, service, status, created_at, requested_reason)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, 0, 'mint integration test')
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

/// The stated binding the write-kind park would have recorded for
/// CONFIG_WITH_BINDING — what the approval card told the human (RULE UFS: the
/// one JSON spelling `repository_binding_json.serialize` produces).
const STATED_BINDING_OWNER = "{\"repositories\":[\"acme/payments\"],\"access\":\"write\",\"base\":\"main\"}";
/// A stated binding naming a DIFFERENT repository — the drift fixture.
const STATED_BINDING_DRIFTED = "{\"repositories\":[\"acme/other\"],\"access\":\"write\",\"base\":\"main\"}";

// v7-shaped gate row ids (the schema CHECK pins the version nibble). The table
// is APPEND-ONLY — no DELETE ever, UPDATE only while pending — so every seed
// is a fresh insert under its own id, idempotent via DO NOTHING, and each test
// keys its lease to its OWN event id so a sibling's row can never satisfy or
// starve its check.
const GATE_ROW_RECHECKS = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e01";
const GATE_ROW_PENDING = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e02";
const GATE_ROW_DRIFTED = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e03";
const GATE_ROW_FAILURE = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e04";
const GATE_ROW_CEILING = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e05";
const GATE_ROW_CONCURRENT = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e06";
const GATE_ROW_SEMANTIC_BINDING = "0195c9db-4a01-7f13-8abc-2b3e1e0d7e08";
const EVENT_WRITE_SEMANTIC_BINDING = "evt-cred-write-semantic";
const CONCURRENT_WRITE_REQUESTS = 100;
const MIN_SERVER_PEAK = 2;

/// Seed a gate row of the given kind/status for (fleet, event) — what the
/// write-kind park writes, reduced to the columns the write mint reads.
fn seedGateRow(conn: *pg.Conn, gate_id: []const u8, fleet_id: []const u8, workspace_id: []const u8, event_id: []const u8, gate_kind: []const u8, status: []const u8, stated_binding: ?[]const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_approval_gates
        \\  (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind,
        \\   proposed_action, evidence, blast_radius, timeout_at, resolved_by, status,
        \\   detail, created_at, updated_at, event_id, stated_binding,
        \\   spend_count, spend_ceiling)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 'act-' || $4, 'webhook', 'webhook:github', $5,
        \\        '', '{}'::jsonb, '', 9999999999999, '', $6,
        \\        '', 1, CASE WHEN $6 = $9 THEN 1 ELSE NULL END,
        \\        $4, $7::jsonb, 0, $8)
        \\ON CONFLICT (id) DO NOTHING
    , .{
        gate_id,
        fleet_id,
        workspace_id,
        event_id,
        gate_kind,
        status,
        stated_binding,
        gate_constants.REPOSITORY_WRITE_SPEND_CEILING,
        approval_gate_rt.GateStatus.approved.toSlice(),
    });
}

/// The approved repository-write gate the write mint requires.
fn seedApprovedWriteGate(conn: *pg.Conn, gate_id: []const u8, fleet_id: []const u8, workspace_id: []const u8, event_id: []const u8, stated_binding: []const u8) !void {
    try seedGateRow(conn, gate_id, fleet_id, workspace_id, event_id, gate_constants.GATE_KIND_REPOSITORY_WRITE, approval_gate_rt.GateStatus.approved.toSlice(), stated_binding);
}

fn seedSpendFixture(h: *TestHarness, lease_id: []const u8, event_id: []const u8, gate_id: []const u8) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_OWNER);
    try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", CONFIG_WITH_BINDING, "# z");
    try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
    try seedLeaseForEvent(conn, lease_id, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, event_id);
    try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
    try seedApprovedWriteGate(conn, gate_id, FLEET_OWNER, WORKSPACE_OWNER, event_id, STATED_BINDING_OWNER);
}

fn gateSpendCount(h: *TestHarness, gate_id: []const u8) !i64 {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT spend_count FROM core.fleet_approval_gates WHERE id = $1::uuid",
        .{gate_id},
    ));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return try row.get(i64, 0);
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
    // No gate-row cleanup on purpose: core.fleet_approval_gates is append-only
    // (schema trigger refuses DELETE), so the write-gate fixtures use unique
    // ids + per-test event ids and re-seed idempotently instead.
    execIgnore(conn, "DELETE FROM core.integration_grants WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ FLEET_OWNER, FLEET_FOREIGN });
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ FLEET_OWNER, FLEET_FOREIGN });
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
        "{{\"integration\":\"{s}\",\"refresh_token\":\"{s}\",\"access_token\":\"at_seeded\",\"expires_at_ms\":1,\"connected_at_ms\":1,\"accounts_base\":\"https://accounts.test\",\"label\":\"test-dc\"}}",
        .{ INTEGRATION_ZOHO, refresh_token },
    );
    defer ALLOC.free(handle);
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, INTEGRATION_ZOHO, handle);
}

/// Store a jira refresh handle — Jira is the provider whose rotating 3LO
/// tokens motivated the write-back, so the persist round-trip pins it.
fn seedJiraHandle(conn: *pg.Conn, workspace_id: []const u8, refresh_token: []const u8) !void {
    const handle = try std.fmt.allocPrint(
        ALLOC,
        "{{\"integration\":\"{s}\",\"refresh_token\":\"{s}\",\"access_token\":\"at_seeded\",\"expires_at_ms\":1,\"connected_at_ms\":1,\"cloud_id\":\"cloud-test\",\"site_url\":\"https://acme.atlassian.net\",\"label\":\"Acme Jira\"}}",
        .{ INTEGRATION_JIRA, refresh_token },
    );
    defer ALLOC.free(handle);
    try vault.storeJsonPlaintext(ALLOC, conn, workspace_id, INTEGRATION_JIRA, handle);
}

/// The vaulted handle's current refresh_token, duped for the caller.
fn vaultRefreshToken(conn: *pg.Conn, workspace_id: []const u8, provider: []const u8) ![]u8 {
    var parsed = try vault.loadJson(ALLOC, conn, workspace_id, provider);
    defer parsed.deinit();
    const rt = switch (parsed.value.object.get(integration.FIELD_REFRESH_TOKEN).?) {
        .string => |s| s,
        else => return error.TestUnexpectedResult,
    };
    return ALLOC.dupe(u8, rt);
}

/// The vault row's updated_at — the write-back detector (any store rewrites it).
fn vaultUpdatedAt(conn: *pg.Conn, workspace_id: []const u8, provider: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT updated_at FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2
    , .{ workspace_id, provider }));
    defer q.deinit();
    const row = try q.next() orelse return error.TestUnexpectedResult;
    return try row.get(i64, 0);
}

fn pinVaultUpdatedAt(conn: *pg.Conn, workspace_id: []const u8, provider: []const u8) !void {
    _ = try conn.exec(
        \\UPDATE vault.secrets SET updated_at = $1 WHERE workspace_id = $2::uuid AND key_name = $3
    , .{ PINNED_UPDATED_AT_MS, workspace_id, provider });
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

/// Whether the write-back block trigger is installed. Its query result is
/// fully closed at return, so the caller may write on the same conn — the
/// prior inline shape exec'd the DROP while its own SELECT was still open,
/// which failed ConnectionBusy and left the residue trigger installed forever,
/// poisoning every later vault write in this file.
fn writeBackBlockPresent(conn: *pg.Conn) bool {
    var q = PgQuery.from(conn.query(
        \\SELECT 1 FROM pg_trigger WHERE tgname = 'test_block_vault_writeback'
    , .{}) catch return false);
    defer q.deinit();
    return (q.next() catch return false) != null;
}

fn dropWriteBackBlock(conn: *pg.Conn) void {
    // DROP TRIGGER (even IF EXISTS) takes an ACCESS EXCLUSIVE lock on the
    // table; gate on pg_trigger so the common no-residue path — this runs at
    // the START of every test in this file — takes no lock on the shared vault.
    if (!writeBackBlockPresent(conn)) return;
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
        try seedLeaseFull(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, EVENT_ID, PAST_MS, protocol.RUNNER_LEASE_STATUS_ACTIVE);
        try seedLeaseFull(conn, LEASE_STALE, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, EVENT_ID, NOW_MS + 30_000, protocol.RUNNER_LEASE_STATUS_EXPIRED);
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
    //
    // This is the only test in the file that mints GITHUB expecting success, so
    // it is the only one that needs a repository binding on its fleet: M157 §2
    // made the GitHub mint fail closed without one (Dimension 2.4), and a
    // binding-free fleet now gets `UZ-GH-002` before the grant gate is ever
    // consulted. The binding is scaffolding for THIS test's subject, not its
    // subject — `test_unbound_fleet_mints_nothing` owns the refusal itself.
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // That same binding is why the fake must state `acme/payments` as its reach:
    // the mint verifies what GitHub says the token covers against the binding,
    // and refuses a mismatch before the token is handed on.
    const reach = try cred_testing.reachResponse(ALLOC, &FLEET_OWNER_REPOSITORIES, .write);
    defer ALLOC.free(reach);

    var gh = cred_testing.FakeGitHub{ .alloc = ALLOC, .resp_body = reach };
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
        // `seedFleet` is ON CONFLICT DO NOTHING and every test in this file
        // shares FLEET_OWNER, so an earlier test's binding-free `{}` config
        // would survive and this seed would be a silent no-op. Drop the row
        // first: this is the one test whose fleet config is load-bearing.
        execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_OWNER});
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", CONFIG_WITH_BINDING, "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLease(conn, LEASE_OWNER, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER);
        try seedGithubHandle(conn, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
        // CONFIG_WITH_BINDING declares WRITE access, so this fleet's mint also
        // rides the write gate: an approved repository-write gate for the
        // lease's event, stating the same binding, is part of the baseline.
        try seedApprovedWriteGate(conn, GATE_ROW_RECHECKS, FLEET_OWNER, WORKSPACE_OWNER, EVENT_ID, STATED_BINDING_OWNER);
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
    try std.testing.expectEqual(@as(i64, 2), try gateSpendCount(h, GATE_ROW_RECHECKS));
    try std.testing.expectEqual(@as(usize, 1), gh.calls);
}

test "integration: test_write_mint_refuses_unapproved" {
    // Write-gate Dimension 2.2: a WRITE-access fleet with a live lease, a
    // connected handle, and an APPROVED integration grant still refuses the
    // github mint when no repository-write gate was approved for the lease's
    // event — and a PENDING gate is equally not an approval. The broker is
    // never reached, so no token exists to leak.
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
        teardown(conn);
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_OWNER});
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", CONFIG_WITH_BINDING, "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLeaseForEvent(conn, LEASE_WRITE_UNAPPROVED, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, EVENT_WRITE_UNAPPROVED);
        try seedGithubHandle(conn, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
        // Deliberately NO write gate row for THIS lease's event.
    }
    defer cleanupAll(h);

    // (1) No gate row at all → 403 UZ-REPAIR-010.
    {
        const body = try githubMintBody(LEASE_WRITE_UNAPPROVED);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_REPAIR_WRITE_UNAPPROVED));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }

    // (2) A PENDING write gate is not an approval — still 403.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try seedGateRow(conn, GATE_ROW_PENDING, FLEET_OWNER, WORKSPACE_OWNER, EVENT_WRITE_UNAPPROVED, gate_constants.GATE_KIND_REPOSITORY_WRITE, approval_gate_rt.GateStatus.pending.toSlice(), STATED_BINDING_OWNER);
    }
    {
        const body = try githubMintBody(LEASE_WRITE_UNAPPROVED);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_REPAIR_WRITE_UNAPPROVED));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }

    // The broker was never reached across either refusal.
    try std.testing.expectEqual(@as(usize, 0), gh.calls);
}

test "integration: test_write_mint_refuses_binding_drift" {
    // Write-gate Dimension 2.3: the gate IS approved, but the binding it stated
    // to the human no longer matches the fleet's current config — the mint
    // refuses (403 UZ-REPAIR-011) rather than minting a reach nobody approved.
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
        teardown(conn);
        try base.seedTenant(conn);
        try base.seedWorkspace(conn, WORKSPACE_OWNER);
        execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_OWNER});
        try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", CONFIG_WITH_BINDING, "# z");
        try seedRunner(conn, RUNNER_OWNER, TOKEN_OWNER);
        try seedLeaseForEvent(conn, LEASE_WRITE_DRIFT, RUNNER_OWNER, FLEET_OWNER, WORKSPACE_OWNER, EVENT_WRITE_DRIFT);
        try seedGithubHandle(conn, WORKSPACE_OWNER);
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_GITHUB, .approved);
        // Approved — but for a DIFFERENT repository than the config now binds.
        try seedApprovedWriteGate(conn, GATE_ROW_DRIFTED, FLEET_OWNER, WORKSPACE_OWNER, EVENT_WRITE_DRIFT, STATED_BINDING_DRIFTED);
    }
    defer cleanupAll(h);

    {
        const body = try githubMintBody(LEASE_WRITE_DRIFT);
        defer ALLOC.free(body);
        const resp = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer resp.deinit();
        try resp.expectStatus(.forbidden);
        try std.testing.expect(resp.bodyContains(ec.ERR_REPAIR_BINDING_DRIFT));
        try std.testing.expect(!resp.bodyContains(GITHUB_MINTED));
    }
    try std.testing.expectEqual(@as(usize, 0), gh.calls);
}

test "integration: semantic repository equality spends approved write gate" {
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_OWNER);
    try base.seedFleet(conn, FLEET_OWNER, WORKSPACE_OWNER, "cred-owner", CONFIG_WITH_BINDING, "# z");
    const stated = "{\"repositories\":[\"ACME/WIDGETS\",\"Acme/Payments\"],\"access\":\"write\",\"base\":\"main\"}";
    try seedApprovedWriteGate(
        conn,
        GATE_ROW_SEMANTIC_BINDING,
        FLEET_OWNER,
        WORKSPACE_OWNER,
        EVENT_WRITE_SEMANTIC_BINDING,
        stated,
    );
    const repositories = [_][]const u8{ "acme/payments", "acme/widgets" };
    const branch_gate_id = try approval_gate_db.approvedWriteGateId(
        h.ctx.pool,
        ALLOC,
        FLEET_OWNER,
        EVENT_WRITE_SEMANTIC_BINDING,
        .{ .repositories = &repositories, .access = .write, .base_branch = "main" },
    ) orelse return error.TestUnexpectedResult;
    defer ALLOC.free(branch_gate_id);
    try std.testing.expectEqualStrings(GATE_ROW_SEMANTIC_BINDING, branch_gate_id);
    try std.testing.expectEqual(write_gate.WriteApproval.approved, try write_gate.reserveWriteApproval(
        ALLOC,
        conn,
        FLEET_OWNER,
        EVENT_WRITE_SEMANTIC_BINDING,
        .{ .repositories = &repositories, .access = .write, .base_branch = "main" },
    ));
    var query = PgQuery.from(try conn.query(
        "SELECT spend_count FROM core.fleet_approval_gates WHERE id = $1::uuid",
        .{GATE_ROW_SEMANTIC_BINDING},
    ));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
}

test "integration: test_failed_write_request_still_spends" {
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;
    try seedSpendFixture(h, LEASE_WRITE_FAILURE, EVENT_WRITE_FAILURE, GATE_ROW_FAILURE);
    defer cleanupAll(h);

    const body = try githubMintBody(LEASE_WRITE_FAILURE);
    defer ALLOC.free(body);
    const response = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
    defer response.deinit();
    try response.expectStatus(.not_found);
    try std.testing.expect(response.bodyContains(ec.ERR_CRED_INTEGRATION_NOT_CONNECTED));
    try std.testing.expectEqual(@as(i64, 1), try gateSpendCount(h, GATE_ROW_FAILURE));
}

test "integration: test_write_request_past_ceiling_refuses" {
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;
    try seedSpendFixture(h, LEASE_WRITE_CEILING, EVENT_WRITE_CEILING, GATE_ROW_CEILING);
    defer cleanupAll(h);

    const body = try githubMintBody(LEASE_WRITE_CEILING);
    defer ALLOC.free(body);
    for (0..gate_constants.REPOSITORY_WRITE_SPEND_CEILING) |_| {
        const spent = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
        defer spent.deinit();
        try spent.expectStatus(.not_found);
    }
    const refused = try (try (try h.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER)).json(body)).send();
    defer refused.deinit();
    try refused.expectStatus(.forbidden);
    try std.testing.expect(refused.bodyContains(ec.ERR_REPAIR_SPEND_EXHAUSTED));
    try std.testing.expectEqual(gate_constants.REPOSITORY_WRITE_SPEND_CEILING, try gateSpendCount(h, GATE_ROW_CEILING));
}

test "integration: test_concurrent_write_requests_hold_ceiling" {
    crypto_primitives.setTestKek();
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    var broker = try CredentialBroker.init(ALLOC, integration.REGISTRY, integration.nullDeps());
    defer broker.deinit();
    h.ctx.broker = &broker;
    try seedSpendFixture(h, LEASE_WRITE_CONCURRENT, EVENT_WRITE_CONCURRENT, GATE_ROW_CONCURRENT);
    defer cleanupAll(h);

    const original_limit = h.ctx.api_max_in_flight_requests;
    h.ctx.api_max_in_flight_requests = CONCURRENT_WRITE_REQUESTS;
    defer h.ctx.api_max_in_flight_requests = original_limit;
    var server_peak = std.atomic.Value(u32).init(0);
    h.ctx.api_peak_in_flight_probe = &server_peak;
    defer h.ctx.api_peak_in_flight_probe = null;
    const body = try githubMintBody(LEASE_WRITE_CONCURRENT);
    defer ALLOC.free(body);
    var threads: [CONCURRENT_WRITE_REQUESTS]std.Thread = undefined;
    var statuses: [CONCURRENT_WRITE_REQUESTS]u16 = .{0} ** CONCURRENT_WRITE_REQUESTS;
    var exhausted: [CONCURRENT_WRITE_REQUESTS]bool = .{false} ** CONCURRENT_WRITE_REQUESTS;
    var ready = std.atomic.Value(usize).init(0);
    var gate = std.atomic.Value(bool).init(false);
    const Worker = struct {
        fn run(harness: *TestHarness, request_body: []const u8, status: *u16, was_exhausted: *bool, ready_count: *std.atomic.Value(usize), start_gate: *std.atomic.Value(bool)) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
            const request = (harness.post(protocol.PATH_RUNNER_CREDENTIALS_MINT).bearer(TOKEN_OWNER) catch return).json(request_body) catch return;
            const response = request.send() catch return;
            defer response.deinit();
            status.* = response.status;
            was_exhausted.* = response.bodyContains(ec.ERR_REPAIR_SPEND_EXHAUSTED);
        }
    };
    var spawned: usize = 0;
    errdefer {
        gate.store(true, .release);
        for (threads[0..spawned]) |thread| thread.join();
    }
    for (&threads, 0..) |*thread, index| {
        thread.* = try std.Thread.spawn(.{}, Worker.run, .{ h, body, &statuses[index], &exhausted[index], &ready, &gate });
        spawned += 1;
    }
    while (ready.load(.acquire) != CONCURRENT_WRITE_REQUESTS) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (threads) |thread| thread.join();
    spawned = 0;

    var spent_count: usize = 0;
    var refused_count: usize = 0;
    for (statuses, exhausted) |status, was_exhausted| {
        if (status == @intFromEnum(std.http.Status.not_found)) {
            spent_count += 1;
        } else if (status == @intFromEnum(std.http.Status.forbidden) and was_exhausted) {
            refused_count += 1;
        } else {
            return error.UnexpectedMintStatus;
        }
    }
    try std.testing.expectEqual(@as(usize, @intCast(gate_constants.REPOSITORY_WRITE_SPEND_CEILING)), spent_count);
    try std.testing.expectEqual(CONCURRENT_WRITE_REQUESTS - spent_count, refused_count);
    try std.testing.expect(server_peak.load(.acquire) >= MIN_SERVER_PEAK);
    try std.testing.expectEqual(gate_constants.REPOSITORY_WRITE_SPEND_CEILING, try gateSpendCount(h, GATE_ROW_CONCURRENT));
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
        try setGrantStatus(conn, FLEET_OWNER, INTEGRATION_JIRA, .approved);
        try seedJiraHandle(conn, WORKSPACE_OWNER, RT_SEEDED);
    }
    defer cleanupAll(h);

    // (1) Cold mint: 200 with the fresh access token; the exchange posted the
    // SEEDED refresh token, and the response's rotated one is vaulted.
    {
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_JIRA);
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
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER, INTEGRATION_JIRA);
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
        const body = try mintBodyFor(LEASE_OWNER, INTEGRATION_JIRA);
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
        try pinVaultUpdatedAt(conn, WORKSPACE_OWNER, INTEGRATION_ZOHO);
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
        try std.testing.expectEqual(PINNED_UPDATED_AT_MS, try vaultUpdatedAt(conn, WORKSPACE_OWNER, INTEGRATION_ZOHO));
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER, INTEGRATION_ZOHO);
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
        const rt = try vaultRefreshToken(conn, WORKSPACE_OWNER, INTEGRATION_ZOHO);
        defer ALLOC.free(rt);
        try std.testing.expectEqualStrings(RT_SEEDED, rt);
    }
}
