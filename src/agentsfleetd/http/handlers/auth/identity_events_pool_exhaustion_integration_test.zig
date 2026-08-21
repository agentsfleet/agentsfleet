//! Pool starvation on the Clerk identity-event ingress.
//!
//! Sibling of `http/pool_exhaustion_integration_test.zig`, whose probe table
//! cannot carry these two routes: every row there is a static method, path,
//! token and body, while a Svix-authenticated request needs a signature
//! computed over the body it is about to send, against a secret the harness
//! installs. So the arms live here, where the signing fixture already is.
//!
//! ## Why the signature has to be real
//!
//! `ingress/github` takes its connection BEFORE it checks the delivery
//! signature — the secret the check needs is what that connection loads — so
//! its starved row needs headers present and nothing more. This handler is the
//! opposite shape: the Clerk secret comes off `hx.ctx`, never the pool, so
//! `innerClerkWebhook` verifies first and only reaches an `acquire` once the
//! payload is authentic. An unsigned request stops at 401 `UZ-WH-010` with
//! every arm below unexecuted, while still reporting a pass.
//!
//! ## One request, four arms
//!
//! `user.deleted` reaches two acquires in a single starved request.
//! `enumerateTenantFleets` fails its acquire and answers null — a swallowed
//! failure, by design, because erasure is never blocked on the unregister pass
//! — so `runDelete` carries on to its own acquire and fails that too. Both are
//! covered by the one probe below; neither needs a second connection to have
//! succeeded first.

const std = @import("std");

const ec = @import("../../../errors/error_registry.zig");
const svix = @import("../../../auth/crypto/svix_verify.zig");
const starve = @import("../../pool_exhaustion_integration_test.zig");
const harness_mod = @import("../../test_harness.zig");
const clerk = @import("identity_events_clerk_integration_test.zig");

const ALLOC = std.testing.allocator;

const PATH = "/v1/auth/identity-events/clerk";

/// Subjects of their own, so a starved run cannot disturb the rows the signed
/// happy-path tests in the sibling file seed and count.
const OIDC_STARVED_CREATE: []const u8 = "oidc-clerk-http-starved-create-01";
const OIDC_STARVED_DELETE: []const u8 = "oidc-clerk-http-starved-delete-02";

/// Send one Svix-signed webhook, signing whatever body is handed in.
///
/// Takes the harness rather than a connection: by the time this runs the pool
/// is empty, and the request under test is the only thing entitled to discover
/// that.
fn sendSigned(h: *harness_mod.TestHarness, svix_id: []const u8, body: []const u8) !harness_mod.Response {
    const ts = try clerk.nowTsAlloc(ALLOC);
    defer ALLOC.free(ts);
    const sig = try clerk.signEntry(ALLOC, svix_id, ts, body);
    defer ALLOC.free(sig);

    return (try (try (try (try h.post(PATH)
        .header(svix.SVIX_ID_HEADER, svix_id))
        .header(svix.SVIX_TS_HEADER, ts))
        .header(svix.SVIX_SIG_HEADER, sig))
        .json(body)).send();
}

test "integration: test_clerk_created_pool_starved — the bootstrap acquire answers 503" {
    const h = clerk.startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        clerk.cleanupAccount(conn, OIDC_STARVED_CREATE);
    }

    const body = try clerk.userCreatedBody(ALLOC, OIDC_STARVED_CREATE, "starved@acme.test");
    defer ALLOC.free(body);

    var held: starve.Held = .empty;
    // Released explicitly below; this catches the drain failing part-way, where
    // the connections taken so far would otherwise be stranded.
    defer starve.releaseAll(h, &held);
    try starve.drainPool(h, &held);

    {
        const resp = try sendSigned(h, "msg_clerk_starved_create_01", body);
        defer resp.deinit();
        try resp.expectStatus(.service_unavailable);
        try std.testing.expect(resp.bodyContains(ec.ERR_INTERNAL_DB_UNAVAILABLE));
    }

    starve.releaseAll(h, &held);
    held = .empty;

    // A signature that verified and a bootstrap that never ran: the request was
    // authentic, so a 503 here has to mean the pool, not the payload. Nothing
    // else in the suite would catch this arm writing a half-built account.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer clerk.cleanupAccount(conn, OIDC_STARVED_CREATE);
    try std.testing.expectEqual(@as(i64, 0), try clerk.countUsers(conn, OIDC_STARVED_CREATE));
}

test "integration: test_clerk_deleted_pool_starved — both delete acquires answer 503" {
    const h = clerk.startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const body = try clerk.userDeletedBody(ALLOC, OIDC_STARVED_DELETE);
    defer ALLOC.free(body);

    var held: starve.Held = .empty;
    defer starve.releaseAll(h, &held);
    try starve.drainPool(h, &held);

    const resp = try sendSigned(h, "msg_clerk_starved_delete_01", body);
    defer resp.deinit();
    // The enumeration's failure is swallowed on purpose, so this status can
    // only come from the purge acquire — which is what makes the pair provable
    // from one response.
    try resp.expectStatus(.service_unavailable);
    try std.testing.expect(resp.bodyContains(ec.ERR_INTERNAL_DB_UNAVAILABLE));
}
