//! The command-line device-login endpoints, driven over the wire.
//!
//! The two suites that already sit beside this one are static analyses — they
//! scan this handler's SOURCE for unredacted session identifiers and for raw
//! error tags leaking into responses. Neither one ever executes a handler, so
//! every branch below was unproven until now.
//!
//! What these tests hold down is the state machine in `docs/AUTH_DEVICE_LOGIN.md`:
//!
//!     pending ──approve──► verification_pending ──verify(correct)──► consumed
//!        │                          │
//!        └────── delete ────────────┴──► aborted        (both terminal)
//!
//! The machine is monotonic and there is deliberately NO path from `pending`
//! straight to `consumed` — presenting the verification code is mandatory,
//! because that code is the only thing binding the human approving in the
//! browser to the human typing in the terminal. A regression that let approve
//! alone consume a session would silently remove that binding, and nothing
//! else in the suite would notice. The fifth test below is the one that would.
//!
//! The handler treats the transport-encryption material (`dashboard_public_key`,
//! `ciphertext`, `nonce`) as opaque strings and forwards them to the store, so
//! these tests carry fixture values rather than performing a real Elliptic-Curve
//! Diffie-Hellman exchange. That is the handler's actual boundary; testing the
//! crypto here would be testing the store through a keyhole.
//!
//! Requires TEST_DATABASE_URL and a live Redis — skipped gracefully otherwise.

const std = @import("std");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");

const ALLOC = std.testing.allocator;

const SESSIONS_PATH = "/v1/auth/sessions";

/// The harness reports the raw wire status, so the comparisons below carry the
/// numeric form rather than the enum.
const STATUS_OK: u16 = @intFromEnum(std.http.Status.ok);
const STATUS_BAD_REQUEST: u16 = @intFromEnum(std.http.Status.bad_request);
const STATUS_NOT_FOUND: u16 = @intFromEnum(std.http.Status.not_found);
const STATUS_NO_CONTENT: u16 = @intFromEnum(std.http.Status.no_content);

/// Any persona works: approve and delete authorize on the token's `sub` claim
/// rather than on a scope, and every fixture persona carries one.
const TOKEN_DASHBOARD = scope_fixtures.VIEWER;

/// Stand-ins for the Elliptic-Curve Diffie-Hellman material. The handler never
/// interprets these — it parses them out of the body and hands them to the
/// store — so their only requirement is being present and stable.
const CLI_PUBLIC_KEY = "BJ7qBOmVdPk8kZ3WXn0mVQqLg5x4mB0Zr2vQ9pXcT8Y=";
const DASHBOARD_PUBLIC_KEY = "BKpL2nR4tYv6wX8zA0cE2gI4kM6oQ8sU0wY2aC4eG6I=";
const CIPHERTEXT = "3q2+7wAAAAAAAAAAAAAAAA==";
const NONCE = "EjRWeJCrze8=";
const VERIFICATION_CODE = "418902";
const WRONG_VERIFICATION_CODE = "000000";
const TOKEN_NAME = "laptop";

/// A syntactically valid version-7 identifier the store has never issued.
const NEVER_CREATED_SESSION_ID = "01960000-0000-7000-8000-000000000000";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

/// Creates a session and returns its identifier. Caller must free.
fn createSession(h: *TestHarness) ![]const u8 {
    const body = try std.fmt.allocPrint(
        ALLOC,
        "{{\"public_key\":\"{s}\",\"token_name\":\"{s}\"}}",
        .{ CLI_PUBLIC_KEY, TOKEN_NAME },
    );
    defer ALLOC.free(body);

    const r = try (try h.post(SESSIONS_PATH).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.created);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    return ALLOC.dupe(u8, parsed.value.object.get("session_id").?.string);
}

fn sessionPath(session_id: []const u8, action: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}{s}", .{ SESSIONS_PATH, session_id, action });
}

/// Reads the session's status field. Caller must free.
fn pollStatus(h: *TestHarness, session_id: []const u8) ![]const u8 {
    const path = try sessionPath(session_id, "");
    defer ALLOC.free(path);
    const r = try h.get(path).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, r.body, .{});
    defer parsed.deinit();
    return ALLOC.dupe(u8, parsed.value.object.get("status").?.string);
}

fn approve(h: *TestHarness, session_id: []const u8, code: []const u8) !u16 {
    const path = try sessionPath(session_id, "/approve");
    defer ALLOC.free(path);
    const body = try std.fmt.allocPrint(
        ALLOC,
        "{{\"dashboard_public_key\":\"{s}\",\"ciphertext\":\"{s}\",\"nonce\":\"{s}\",\"verification_code\":\"{s}\"}}",
        .{ DASHBOARD_PUBLIC_KEY, CIPHERTEXT, NONCE, code },
    );
    defer ALLOC.free(body);
    const r = try (try (try h.patch(path).bearer(TOKEN_DASHBOARD)).json(body)).send();
    defer r.deinit();
    return r.status;
}

fn verify(h: *TestHarness, session_id: []const u8, code: []const u8) !u16 {
    const path = try sessionPath(session_id, "/verify");
    defer ALLOC.free(path);
    const body = try std.fmt.allocPrint(ALLOC, "{{\"verification_code\":\"{s}\"}}", .{code});
    defer ALLOC.free(body);
    const r = try (try h.post(path).json(body)).send();
    defer r.deinit();
    return r.status;
}

test "integration: a created session polls back as pending, carrying the key the caller sent" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const body = try std.fmt.allocPrint(
        ALLOC,
        "{{\"public_key\":\"{s}\",\"token_name\":\"{s}\"}}",
        .{ CLI_PUBLIC_KEY, TOKEN_NAME },
    );
    defer ALLOC.free(body);

    const create = try (try h.post(SESSIONS_PATH).json(body)).send();
    defer create.deinit();
    try create.expectStatus(.created);

    const created = try std.json.parseFromSlice(std.json.Value, ALLOC, create.body, .{});
    defer created.deinit();
    const session_id = created.value.object.get("session_id").?.string;
    const login_url = created.value.object.get("login_url").?.string;

    // The browser is sent to a URL carrying the session id; the human never
    // types it, so a malformed one strands the login with no error anywhere.
    try std.testing.expect(std.mem.endsWith(u8, login_url, session_id));
    try std.testing.expect(std.mem.indexOf(u8, login_url, "/cli-auth/") != null);

    // The poll is what the dashboard reads to render the approve page, so the
    // key and the name have to survive the round trip intact.
    const path = try sessionPath(session_id, "");
    defer ALLOC.free(path);
    const poll = try h.get(path).send();
    defer poll.deinit();
    try poll.expectStatus(.ok);

    const polled = try std.json.parseFromSlice(std.json.Value, ALLOC, poll.body, .{});
    defer polled.deinit();
    const obj = polled.value.object;
    try std.testing.expectEqualStrings("pending", obj.get("status").?.string);
    try std.testing.expectEqualStrings(CLI_PUBLIC_KEY, obj.get("cli_public_key").?.string);
    try std.testing.expectEqualStrings(TOKEN_NAME, obj.get("token_name").?.string);
    // Sessions are bounded at five minutes; an absent or past expiry would mean
    // the session outlives its window.
    try std.testing.expect(obj.get("expires_at_ms").?.integer > 0);
}

test "integration: create refuses a missing body and a malformed payload alike" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const empty = try (try h.post(SESSIONS_PATH).json("")).send();
    defer empty.deinit();
    try std.testing.expectEqual(STATUS_BAD_REQUEST, empty.status);

    // Well-formed JavaScript Object Notation, wrong shape: the public key the
    // whole transport encryption hangs off is absent.
    const malformed = try (try h.post(SESSIONS_PATH).json("{\"token_name\":\"laptop\"}")).send();
    defer malformed.deinit();
    try std.testing.expectEqual(STATUS_BAD_REQUEST, malformed.status);
}

test "integration: polling a session that was never created is a clean not-found" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // Well-formed and absent, not malformed: session identifiers are version-7
    // Universally Unique Identifiers, and the interesting answer is what an id
    // the store has simply never seen returns — a session that expired out of
    // Redis reaches this same branch.
    const path = try sessionPath(NEVER_CREATED_SESSION_ID, "");
    defer ALLOC.free(path);
    const r = try h.get(path).send();
    defer r.deinit();
    try std.testing.expectEqual(STATUS_NOT_FOUND, r.status);
}

test "integration: approving a pending session moves it to verification_pending" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const session_id = try createSession(h);
    defer ALLOC.free(session_id);

    try std.testing.expectEqual(STATUS_OK, try approve(h, session_id, VERIFICATION_CODE));

    const status = try pollStatus(h, session_id);
    defer ALLOC.free(status);
    try std.testing.expectEqualStrings("verification_pending", status);
}

test "integration: approval alone never consumes — the code is what consumes" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const session_id = try createSession(h);
    defer ALLOC.free(session_id);
    try std.testing.expectEqual(STATUS_OK, try approve(h, session_id, VERIFICATION_CODE));

    // A wrong code must not advance the machine. If this ever returns 200 the
    // browser-to-terminal binding is gone and anyone who reaches the endpoint
    // with a session id collects the token.
    const wrong = try verify(h, session_id, WRONG_VERIFICATION_CODE);
    try std.testing.expect(wrong != STATUS_OK);

    const still_pending = try pollStatus(h, session_id);
    defer ALLOC.free(still_pending);
    try std.testing.expectEqualStrings("verification_pending", still_pending);

    // The right code consumes, and the poll afterwards refuses rather than
    // handing the payload out a second time.
    try std.testing.expectEqual(STATUS_OK, try verify(h, session_id, VERIFICATION_CODE));

    const path = try sessionPath(session_id, "");
    defer ALLOC.free(path);
    const after = try h.get(path).send();
    defer after.deinit();
    try std.testing.expect(after.status != STATUS_OK);
}

test "integration: deleting a session aborts it and the poll says so" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const session_id = try createSession(h);
    defer ALLOC.free(session_id);

    const path = try sessionPath(session_id, "");
    defer ALLOC.free(path);

    // Cancel is bound to the approver: the handler refuses unless the caller's
    // Clerk subject matches the one stored on the session, and a session only
    // acquires that subject at approve. So a still-pending session cannot be
    // cancelled by anyone — including the user who is about to approve it.
    const too_early = try (try h.delete(path).bearer(TOKEN_DASHBOARD)).send();
    defer too_early.deinit();
    try std.testing.expect(too_early.status != STATUS_OK);

    try std.testing.expectEqual(STATUS_OK, try approve(h, session_id, VERIFICATION_CODE));

    // Cancel answers 204: the session is gone, so there is no body to return.
    const del = try (try h.delete(path).bearer(TOKEN_DASHBOARD)).send();
    defer del.deinit();
    try std.testing.expectEqual(STATUS_NO_CONTENT, del.status);

    // `aborted` is terminal, so the poll answers with a refusal rather than a
    // status body — a cancelled login must never look resumable.
    const after = try h.get(path).send();
    defer after.deinit();
    try std.testing.expect(after.status != STATUS_OK);
}
