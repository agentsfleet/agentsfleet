//! Envelope-parsing proofs for the two inbound-webhook shapes.
//!
//! Both entry points are reachable by anything that clears the signature check,
//! so every refusal below is a shape a sender can actually put on the wire. The
//! existing tests in `webhook_parse.zig` drive `std.json` and `svixEventType`
//! directly; these drive the two `pub fn`s a request reaches, which is where the
//! body-absent, size, and empty-field refusals live.
//!
//! `hx.alloc` is an arena here because it is one in production: `server.zig`
//! hands each request an `ArenaAllocator` over a zeroizing allocator and drops
//! it at the end of dispatch. `parseBody` returns `parsed.value` without a
//! matching `deinit`, which is correct against that arena and would read as a
//! leak under `testing.allocator`.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const webhook_parse = @import("webhook_parse.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const svix_verify = @import("../../../auth/crypto/svix_verify.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-webhook-1";
const FLEET_ID = "01932b7c-0000-7000-8000-0000000000ff";
const SVIX_ID = "msg_2abcDEF";
const K_ERROR_CODE = "error_code";
const K_DETAIL = "detail";

const VALID_ENVELOPE =
    \\{"event_id":"evt_001","type":"email.received","data":{"from":"a@b.com"}}
;

/// Neither parse path reads the context or the principal — a webhook body is
/// parsed before anything about the caller matters.
fn buildHx(res: *httpz.Response, alloc: std.mem.Allocator) Hx {
    return Hx{
        .alloc = alloc,
        // SAFETY: unread on both parse paths.
        .principal = undefined,
        .req_id = REQ_ID,
        // SAFETY: unread on both parse paths.
        .ctx = undefined,
        .res = res,
    };
}

fn expectMalformed(ht: *httpz.testing.Testing, detail: []const u8) !void {
    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_WEBHOOK_MALFORMED, json.object.get(K_ERROR_CODE).?.string);
    try testing.expectEqualStrings(detail, json.object.get(K_DETAIL).?.string);
}

// ── parseBody: the agentsfleet envelope ──────────────────────────────────

test "should parse a well-formed agentsfleet envelope" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body(VALID_ENVELOPE);

    const got = webhook_parse.parseBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got != null);
    try testing.expectEqualStrings("evt_001", got.?.event_id);
    try testing.expectEqualStrings("email.received", got.?.type);
    try testing.expectEqualStrings("a@b.com", got.?.data.object.get("from").?.string);
}

test "should refuse an agentsfleet delivery that carries no body" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const got = webhook_parse.parseBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_BODY_REQUIRED);
}

test "should refuse an agentsfleet delivery whose body is not JSON" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body("{not json at all");

    const got = webhook_parse.parseBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_MALFORMED_JSON);
}

test "should refuse an envelope whose idempotency key is empty" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    // Present but empty: the JSON parses, so only the explicit length check
    // stands between this and an event that dedups against every other empty id.
    ht.body(
        \\{"event_id":"","type":"email.received","data":{}}
    );

    const got = webhook_parse.parseBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_MISSING_FIELDS);
}

test "should refuse an envelope whose event type is empty" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body(
        \\{"event_id":"evt_002","type":"","data":{}}
    );

    const got = webhook_parse.parseBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_MISSING_FIELDS);
}

// ── parseSvixBody: a Svix/Clerk delivery ─────────────────────────────────

test "should take the idempotency key from the svix header, not the body" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(svix_verify.SVIX_ID_HEADER, SVIX_ID);
    // The body carries its own `event_id`; the header is the one Svix retries
    // with, so dedup must key on it and ignore the body's.
    ht.body(
        \\{"event_id":"body_supplied","type":"user.created","data":{"id":"u_1"}}
    );

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got != null);
    try testing.expectEqualStrings(SVIX_ID, got.?.event_id);
    try testing.expectEqualStrings("user.created", got.?.type);
}

test "should forward the whole svix body as the event data" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(svix_verify.SVIX_ID_HEADER, SVIX_ID);
    ht.body(
        \\{"type":"user.created","data":{"id":"u_1"}}
    );

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    // Unlike the agentsfleet envelope, the Svix `data` is the envelope itself —
    // the consumer reads `data.data`, so dropping the outer object loses the type.
    try testing.expectEqualStrings("user.created", got.?.data.object.get("type").?.string);
}

test "should label a svix delivery that declares no type" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(svix_verify.SVIX_ID_HEADER, SVIX_ID);
    ht.body(
        \\{"data":{"id":"u_1"}}
    );

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got != null);
    try testing.expect(got.?.type.len > 0);
}

test "should refuse a svix delivery with no svix-id header" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body(VALID_ENVELOPE);

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    // Without it there is no idempotency key, so a Svix retry would enqueue the
    // event a second time.
    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_MISSING_FIELDS);
}

test "should refuse a svix delivery that carries no body" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(svix_verify.SVIX_ID_HEADER, SVIX_ID);

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_BODY_REQUIRED);
}

test "should refuse a svix delivery whose body is not JSON" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header(svix_verify.SVIX_ID_HEADER, SVIX_ID);
    ht.body("<xml>not json</xml>");

    const got = webhook_parse.parseSvixBody(buildHx(ht.res, arena.allocator()), ht.req, FLEET_ID);

    try testing.expect(got == null);
    try expectMalformed(&ht, ec.MSG_MALFORMED_JSON);
}
