//! Shared fixtures for the command-line credential integration suites.
//!
//! Three suites consume this: `cli_credentials_integration_test.zig` drives
//! what a credential reaches over the real router,
//! `cli_credentials_admission_integration_test.zig` proves who those routes
//! admit, and `cli_credentials_index_integration_test.zig` proves what the
//! DATABASE refuses. All need the same things, so they live here rather than
//! being copied: the registry wiring that makes an `afc_` bearer resolvable,
//! the tenant and user rows the credential's foreign keys require, and the
//! mint helper that captures the raw value returned exactly once.
//!
//! **What is real and what is stubbed.** The lookup is the production
//! `cmd/cli_credential_lookup.zig` against the harness pool, so every
//! assertion below travels the real digest → row → principal path. Only
//! `resolveScopes` is stubbed, and that is the identity provider's network
//! boundary — the seam `auth/middleware/cli_credential.zig` defines precisely
//! so the middleware's branches are provable without reaching Clerk. Stubbing
//! it mocks an external edge, never an internal layer.
//!
//! **Why these personas.** `oidc_subject` carries a unique index, so two
//! suites seeding the same subject collide inside one test binary.
//! `user_patch_concurrent` is claimed by no other suite's `core.users` row
//! (the fleet PATCH suite uses the token but seeds no user), and the peer
//! below carries a subject owned by this file alone.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;

const auth_mw = @import("../../../auth/middleware/mod.zig");
const harness_mod = @import("../../test_harness.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const cli_credential_lookup = @import("../../../cmd/cli_credential_lookup.zig");
const api_key_lookup = @import("../../../cmd/api_key_lookup.zig");
const api_key = @import("../../../auth/api_key.zig");
const store = @import("../../../state/cli_credentials.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

pub const TestHarness = harness_mod.TestHarness;
pub const ALLOC = std.testing.allocator;

pub const PATH = "/v1/cli-credentials";

/// The authenticated probe every suite shares: an ordinary tenant route that
/// answers 200 for a live credential. The credential family has no GET of its
/// own — the list endpoint was removed as unused surface — so "does this
/// bearer authenticate, and as whom" is asked of a route real work uses.
pub const PROBE_PATH = "/v1/workspaces/" ++ WORKSPACE_ID ++ "/fleets";

/// The item-form path for a revoke. Caller frees. Shared rather than copied:
/// both router-driven suites build it, and two spellings of one route are how
/// a path change fixes half the tests and silently strands the other half.
pub fn revokePath(credential_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ PATH, credential_id });
}

/// Shared namespace for every identifier this suite owns. Single-sourced so
/// the fixtures cannot drift onto a neighbouring suite's rows — and so no line
/// below carries a full identifier as its own literal, which a secret scanner
/// reads as a high-entropy credential when the name beside it says "key".
const FIXTURE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f70";

/// This suite's own tenant. Not one of the shared fixture tenants: these tests
/// delete their user rows, and `core.cli_credentials` cascades from both
/// `core.users` and `core.tenants`, so an isolated tenant keeps that cleanup
/// from reaching a sibling suite's rows.
pub const TENANT_ID = FIXTURE_ID ++ "01";

/// The operator these tests act as. The token's own tenant claim is not read
/// on this route family — the handler resolves the tenant from `core.users` —
/// so the seeded row below is the one that decides what a mint records.
pub const TOKEN_OWNER = scope_fixtures.PATCH_CONCURRENT_ADMIN;
pub const OWNER_USER_ID = FIXTURE_ID ++ "11";
pub const OWNER_SUBJECT = "user_patch_concurrent";

/// A second person, deliberately without a token. Their credential is seeded
/// through the store rather than the endpoint, which is what makes the
/// ownership assertions real: the statements' `user_id` predicates are the
/// only thing keeping this row out of the owner's list and out of reach of the
/// owner's revoke.
pub const PEER_USER_ID = FIXTURE_ID ++ "12";
pub const PEER_SUBJECT = "user_cli_credential_peer";

/// A tenant key, for the principal-mode refusal. `agt_t` carries the whole
/// tenant grant, so no required scope could refuse it on these routes.
pub const TENANT_KEY = auth_mw.tenant_api_key.TENANT_KEY_PREFIX ++ "c" ** TENANT_KEY_BODY_CHARS;
const TENANT_KEY_BODY_CHARS = 48;
const TENANT_KEY_ROW_ID = FIXTURE_ID ++ "21";

pub const MACHINE_NAME = "indy-macbook.local";
pub const OTHER_MACHINE_NAME = "indy-desktop.local";

/// A workspace holding one fleet, so a credential can be pointed at an
/// ordinary business route rather than only at the credential routes it
/// manages. Listing fleets is the first thing a terminal does after logging
/// in, and it is the assertion that catches a credential which authenticates
/// its own endpoints but resolves to nothing usable anywhere else.
pub const WORKSPACE_ID = FIXTURE_ID ++ "31";
pub const FLEET_ID = FIXTURE_ID ++ "41";
pub const FLEET_NAME = "cli-credential-fleet";

/// What the stubbed resolver answers. The credential routes require no scope
/// (a tenant key already holds every scope they could name, so the refusal
/// that matters is on principal mode), which is why a fixed claim is enough
/// here — nothing in this suite turns on its contents.
const SCOPE_CLAIM = "fleet:read schedule:read";

// SAFETY: written by configureRegistry before the server accepts a request,
// and the harness owns the pool for the whole life of the registry.
var credential_ctx: cli_credential_lookup.Ctx = undefined;
// SAFETY: same — configureRegistry writes it before any request is served.
var tenant_key_ctx: api_key_lookup.Ctx = undefined;

fn resolveScopes(_: *anyopaque, alloc: std.mem.Allocator, _: []const u8) anyerror![]const u8 {
    return alloc.dupe(u8, SCOPE_CLAIM);
}

pub fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    credential_ctx = .{ .pool = h.pool };
    reg.cli_credential_mw = .{
        .host = &credential_ctx,
        .lookup = cli_credential_lookup.lookup,
        // The scope host is unused by the stub, but the field must point at
        // something live rather than carrying an undefined pointer.
        .scope_host = &credential_ctx,
        .resolveScopes = resolveScopes,
    };
    tenant_key_ctx = .{ .pool = h.pool };
    reg.tenant_api_key_mw = .{
        .host = &tenant_key_ctx,
        .lookup = api_key_lookup.lookup,
        // Since §6 a tenant key resolves its creator's capabilities; without a
        // resolver the key authenticates and then fails every gate behind it.
        .scope_host = &tenant_key_ctx,
        .resolveScopes = scope_fixtures.ownerScopes,
    };
}

/// Start a harness with the credential path wired and the rows seeded.
pub fn seededHarness() !*TestHarness {
    const h = try TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seed(conn);
    return h;
}

/// Injected fault: make every insert into `core.cli_credentials` fail, so the
/// revoke-then-insert pair can be observed failing at its second statement.
/// A trigger rather than a dropped table, because the row must be refused
/// while the transaction is otherwise healthy — the same shape
/// `account_teardown_test.zig` uses to prove its own rollback.
const INSTALL_INSERT_FAULT =
    \\CREATE OR REPLACE FUNCTION core.test_block_cli_credential_insert() RETURNS trigger AS $$
    \\BEGIN RAISE EXCEPTION 'injected test failure'; END; $$ LANGUAGE plpgsql
;
const ATTACH_INSERT_FAULT =
    \\CREATE TRIGGER trg_test_block_cli_credential_insert
    \\  BEFORE INSERT ON core.cli_credentials
    \\  FOR EACH ROW EXECUTE FUNCTION core.test_block_cli_credential_insert()
;
const DETACH_INSERT_FAULT =
    "DROP TRIGGER IF EXISTS trg_test_block_cli_credential_insert ON core.cli_credentials";
const DROP_INSERT_FAULT =
    "DROP FUNCTION IF EXISTS core.test_block_cli_credential_insert()";

/// Every credential this suite could have created, in one statement. Used by
/// `seed` (a prior run that died before cleanup leaves live rows, and the
/// partial unique index would refuse this run's first mint) and by `cleanup`.
const DELETE_SUITE_CREDENTIALS =
    "DELETE FROM core.cli_credentials WHERE user_id IN ($1::uuid, $2::uuid)";

pub fn blockCredentialInserts(conn: *pg.Conn) !void {
    _ = try conn.exec(INSTALL_INSERT_FAULT, .{});
    _ = try conn.exec(ATTACH_INSERT_FAULT, .{});
}

/// Idempotent, and called from `seed` as well as from the test that installs
/// the fault. A crash between install and removal would otherwise refuse every
/// later mint in the whole binary, so the next harness start clears it.
pub fn unblockCredentialInserts(conn: *pg.Conn) void {
    drop(conn, DETACH_INSERT_FAULT, .{});
    drop(conn, DROP_INSERT_FAULT, .{});
}

fn seed(conn: *pg.Conn) !void {
    const now_ms = clock.nowMillis();
    unblockCredentialInserts(conn);
    _ = try conn.exec(
        \\INSERT INTO core.tenants (id, name, created_at, updated_at)
        \\VALUES ($1::uuid, 'CLI Credential Test Tenant', $2::bigint, $2::bigint)
        \\ON CONFLICT (id) DO NOTHING
    , .{ TENANT_ID, now_ms });
    try seedUser(conn, OWNER_USER_ID, OWNER_SUBJECT, "owner@cli-credential.test", now_ms);
    try seedUser(conn, PEER_USER_ID, PEER_SUBJECT, "peer@cli-credential.test", now_ms);
    _ = try conn.exec(DELETE_SUITE_CREDENTIALS, .{ OWNER_USER_ID, PEER_USER_ID });
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (id, tenant_id, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::bigint)
        \\ON CONFLICT (id) DO NOTHING
    , .{ WORKSPACE_ID, TENANT_ID, now_ms });
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, trigger_markdown,
        \\   config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, '# test', '# test',
        \\        '{}'::jsonb, 'active', $5::bigint, $5::bigint)
        \\ON CONFLICT (id) DO NOTHING
    , .{ FLEET_ID, WORKSPACE_ID, TENANT_ID, FLEET_NAME, now_ms });
    const key_hash = api_key.sha256Hex(TENANT_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys
        \\  (id, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'cli-credential-test-key', '', $3::text,
        \\        'user_cli_credential_admin', TRUE, $4::bigint, $4::bigint)
        \\ON CONFLICT (key_hash) DO NOTHING
    , .{ TENANT_KEY_ROW_ID, TENANT_ID, key_hash[0..], now_ms });
}

fn seedUser(conn: *pg.Conn, id: []const u8, subject: []const u8, email: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.users (id, tenant_id, oidc_subject, email, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)
        \\ON CONFLICT (id) DO NOTHING
    , .{ id, TENANT_ID, subject, email, now_ms });
}

/// Called from the test body, never from a `defer` — deferred cleanup holds a
/// pool connection past `pool.deinit()` and leaks it (harness contract).
pub fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    unblockCredentialInserts(conn);
    // Credentials cascade from both parents, but they are deleted explicitly so
    // a failure to remove them surfaces here rather than as a silent survivor.
    drop(conn, DELETE_SUITE_CREDENTIALS, .{ OWNER_USER_ID, PEER_USER_ID });
    drop(conn, "DELETE FROM core.api_keys WHERE id = $1::uuid", .{TENANT_KEY_ROW_ID});
    drop(conn, "DELETE FROM core.users WHERE id IN ($1::uuid, $2::uuid)", .{ OWNER_USER_ID, PEER_USER_ID });
    drop(conn, "DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID});
}

fn drop(conn: *pg.Conn, statement: []const u8, args: anytype) void {
    _ = conn.exec(statement, args) catch |err|
        std.log.warn("cli credential cleanup ignored: {s}", .{@errorName(err)});
}

/// What a mint handed back. `secret` is the only copy of the raw value that
/// will ever exist outside the server, so the caller owns and frees both.
pub const Minted = struct {
    id: []const u8,
    secret: []const u8,

    pub fn deinit(self: Minted) void {
        ALLOC.free(self.id);
        ALLOC.free(self.secret);
    }
};

/// Mint through the endpoint, as `agentsfleet login` does.
pub fn mint(h: *TestHarness, bearer: []const u8, machine_name: []const u8) !Minted {
    const body = try std.fmt.allocPrint(ALLOC, "{{\"machine_name\":\"{s}\"}}", .{machine_name});
    defer ALLOC.free(body);

    const r = try (try (try h.post(PATH).bearer(bearer)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    const id = try ALLOC.dupe(u8, parsed.value.object.get("id").?.string);
    errdefer ALLOC.free(id);
    const secret = try ALLOC.dupe(u8, parsed.value.object.get("credential").?.string);
    return .{ .id = id, .secret = secret };
}

/// Mint straight through the store, bypassing the endpoint. Used for the peer,
/// who has no token — and for seeding a row the endpoint would refuse to
/// create, which is how the index itself gets tested.
pub fn mintDirect(conn: *pg.Conn, user_id: []const u8, machine_name: []const u8) !store.Minted {
    return store.mint(ALLOC, conn, .{
        .user_id = user_id,
        .tenant_id = TENANT_ID,
        .machine_name = machine_name,
        .deployment = "http://127.0.0.1:0",
        .created_from_address = "127.0.0.1",
    });
}

/// Every column of one credential row, concatenated. A single comparison then
/// covers the whole row rather than only the columns a test remembered to name
/// — which is what makes "nothing changed" provable rather than asserted.
/// `revoked_at` is the one nullable column, and `concat_ws` drops nulls, so it
/// is spelled out rather than silently vanishing from the comparison.
const SELECT_WHOLE_ROW =
    \\SELECT concat_ws('|', id::text, user_id::text, tenant_id::text, machine_name,
    \\                 credential_hash, credential_prefix, deployment,
    \\                 created_from_address, created_at::text,
    \\                 coalesce(revoked_at::text, 'NULL'))
    \\FROM core.cli_credentials WHERE id = $1::uuid
;

/// Snapshot one credential row. Caller frees.
pub fn wholeRow(h: *TestHarness, credential_id: []const u8) ![]const u8 {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(SELECT_WHOLE_ROW, .{credential_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return ALLOC.dupe(u8, try row.get([]u8, 0));
}

/// Columns on the credential table whose name suggests a clock that could
/// retire a row. The durability claim rests on there being none: revocation is
/// the only thing that ends a credential, and it is always somebody's
/// deliberate act. Asked of the live schema rather than asserted in prose, so
/// a future column named for an expiry fails here and has to be argued for
/// rather than landing quietly.
const SELECT_EXPIRY_LIKE_COLUMNS =
    \\SELECT COUNT(*)::bigint FROM information_schema.columns
    \\ WHERE table_schema = 'core' AND table_name = 'cli_credentials'
    \\   AND (column_name LIKE '%expire%' OR column_name LIKE '%expiry%'
    \\        OR column_name LIKE '%ttl%' OR column_name LIKE '%valid_until%')
;

pub fn expiryLikeColumnCount(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(SELECT_EXPIRY_LIKE_COLUMNS, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}

/// How many live credentials one user holds for one machine. The question the
/// partial unique index exists to bound, asked directly of the datastore.
pub fn liveCountForMachine(conn: *pg.Conn, user_id: []const u8, machine_name: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::bigint FROM core.cli_credentials
        \\WHERE user_id = $1::uuid AND machine_name = $2 AND revoked_at IS NULL
    , .{ user_id, machine_name }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowMissing;
    return row.get(i64, 0);
}
