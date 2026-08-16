//! DB-backed refusal arms for the webhook-sig lookup.
//!
//! Every arm here is a credential that cannot be resolved — missing from the
//! vault, or present but shaped wrong. They matter because the middleware
//! decides whether to verify a signature from what these functions return: a
//! null secret must reach it as "fail closed", never as "no signature
//! required". The happy paths and the pure helpers are pinned inline in
//! `serve_webhook_lookup.zig`; these need a live Postgres and a real vault
//! write, so they live here and skip when neither TEST_DATABASE_URL nor
//! DATABASE_URL is set.
//!
//! Driven through the two `pub` entry points rather than the private
//! `loadWebhookSecret`, so the test exercises the path the middleware actually
//! calls and nothing widens its visibility to be testable.

const std = @import("std");
const pg = @import("pg");
const lookup_mod = @import("serve_webhook_lookup.zig");
const base = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

/// Own workspace and fleet ids, distinct from every other suite's, so these
/// tests neither collide with a peer nor clean up rows they did not write.
const TEST_WS_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000401";
const HMAC_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000402";
const SVIX_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-cd0000000403";

/// `github` is in the provider registry, so `detectProvider` recognizes it and
/// the lookup proceeds to resolve the credential — which is the arm under test.
const HMAC_CONFIG =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"github"}]}}
;

/// A Svix trigger naming a vault key that is never written.
const SVIX_CONFIG =
    \\{"x-agentsfleet":{"triggers":[{"type":"webhook","source":"clerk","signature":{"secret_ref":"whsec_never_stored"}}]}}
;

const Fixture = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    /// Live connection with the tenant + workspace seeded, or null to skip.
    fn open(alloc: std.mem.Allocator) !?Fixture {
        crypto_primitives.setTestKek();
        const handle = (try base.openTestConn(alloc)) orelse return null;
        errdefer {
            handle.pool.release(handle.conn);
            handle.pool.deinit();
        }
        try base.seedTenant(handle.conn);
        try base.seedWorkspace(handle.conn, TEST_WS_ID);
        return .{ .pool = handle.pool, .conn = handle.conn };
    }

    /// Drop only what this suite wrote, then release. Runs on a connection the
    /// caller still holds, so it must not be used after a `releaseConn`.
    fn close(self: Fixture) void {
        _ = self.conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1", .{TEST_WS_ID}) catch |err| {
            std.log.warn("ignored: {s}", .{@errorName(err)});
        };
        base.teardownFleets(self.conn, TEST_WS_ID);
        base.teardownWorkspace(self.conn, TEST_WS_ID);
        base.teardownTenant(self.conn);
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Store `value` at `key_name` for this suite's workspace.
fn storeCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    key_name: []const u8,
    pairs: []const [2][]const u8,
) !void {
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    for (pairs) |kv| try obj.put(alloc, kv[0], .{ .string = kv[1] });
    try base.storeVaultJson(alloc, conn, TEST_WS_ID, key_name, .{ .object = obj });
}

test "a webhook fleet whose credential was never stored resolves no secret" {
    // The vault has nothing at `github`, so the load fails outright. The
    // scheme must still come back populated: the middleware needs it to reject
    // the request as unverifiable (UZ-WH-020) rather than wave it through for
    // carrying no signature config at all.
    const alloc = std.testing.allocator;
    const fx = (try Fixture.open(alloc)) orelse return error.SkipZigTest;
    defer fx.close();

    try base.seedFleet(fx.conn, HMAC_FLEET_ID, TEST_WS_ID, "hmac-no-credential", HMAC_CONFIG, "# md");

    const result = (try lookup_mod.lookup(fx.pool, HMAC_FLEET_ID, alloc)).?;
    defer if (result.signature_scheme) |s| freeSchemeFields(alloc, s);
    defer if (result.signature_secret) |s| alloc.free(s);

    try std.testing.expect(result.signature_secret == null);
    try std.testing.expect(result.signature_scheme != null);
}

test "a webhook credential missing its secret field resolves no secret" {
    // The credential exists and parses as an object, but carries no
    // `webhook_secret`. A misconfigured credential must read the same as an
    // absent one — anything else verifies a signature against a key the
    // operator never set.
    const alloc = std.testing.allocator;
    const fx = (try Fixture.open(alloc)) orelse return error.SkipZigTest;
    defer fx.close();

    try base.seedFleet(fx.conn, HMAC_FLEET_ID, TEST_WS_ID, "hmac-wrong-shape", HMAC_CONFIG, "# md");
    try storeCredential(alloc, fx.conn, "github", &.{.{ "api_token", "not-the-webhook-secret" }});

    const result = (try lookup_mod.lookup(fx.pool, HMAC_FLEET_ID, alloc)).?;
    defer if (result.signature_scheme) |s| freeSchemeFields(alloc, s);
    defer if (result.signature_secret) |s| alloc.free(s);

    try std.testing.expect(result.signature_secret == null);
}

test "a Svix secret_ref pointing at nothing resolves no secret" {
    // `lookupSvix` acquires its own connection, so this suite's is released
    // first — the pool need not be wide enough for both.
    const alloc = std.testing.allocator;
    const fx = (try Fixture.open(alloc)) orelse return error.SkipZigTest;

    try base.seedFleet(fx.conn, SVIX_FLEET_ID, TEST_WS_ID, "svix-missing-ref", SVIX_CONFIG, "# md");
    fx.pool.release(fx.conn);

    const result = (try lookup_mod.lookupSvix(fx.pool, SVIX_FLEET_ID, alloc)).?;
    if (result.secret) |s| alloc.free(s);

    const cleanup_conn = try fx.pool.acquire();
    const reopened = Fixture{ .pool = fx.pool, .conn = cleanup_conn };
    defer reopened.close();

    try std.testing.expect(result.secret == null);
}

/// `SignatureScheme`'s fields are owned copies; the lookup's own `freeScheme`
/// is private, so the test releases them by the same shape.
fn freeSchemeFields(alloc: std.mem.Allocator, s: anytype) void {
    alloc.free(s.sig_header);
    alloc.free(s.prefix);
    if (s.ts_header) |t| alloc.free(t);
    alloc.free(s.hmac_version);
}
