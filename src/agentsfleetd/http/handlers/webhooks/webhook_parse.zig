//! Inbound-webhook body parsing — the two ingress envelope shapes that
//! `fleet.zig` orchestrates. Kept separate so the orchestration file stays
//! under the FLL cap and the parsing rules are unit-testable in isolation.
//!
//!  * `parseBody`     — the agentsfleet `{event_id, type, data}` envelope used
//!                      by the generic per-fleet receiver (HMAC-signed).
//!  * `parseSvixBody` — a Svix/Clerk delivery, where the `svix-id` header is
//!                      the idempotency key and the whole body is the data.

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const hx_mod = @import("../hx.zig");
const svix_verify = @import("../../../auth/crypto/svix_verify.zig");

const log = logging.scoped(.http_webhook);
const Hx = hx_mod.Hx;

/// Event-type label applied to a Svix delivery whose body carries no top-level
/// `type` string. Svix/Clerk envelopes normally include one.
const SVIX_DEFAULT_TYPE = "svix.event";

// Structured-log event names, shared by both parse paths.
const EV_NO_BODY = "no_body";
const EV_MALFORMED_JSON = "malformed_json";

/// The normalized inbound event both ingress shapes resolve to before the
/// shared dedup-and-enqueue path in `fleet.zig`.
pub const WebhookPayload = struct {
    event_id: []const u8,
    type: []const u8,
    data: std.json.Value,
};

/// Parse the agentsfleet `{event_id, type, data}` envelope. Rejects a missing
/// body, malformed JSON, or an empty `event_id`/`type` with 400 UZ-WH-002.
pub fn parseBody(hx: Hx, req: *httpz.Request, fleet_id: []const u8) ?WebhookPayload {
    const body = req.body() orelse {
        log.warn(EV_NO_BODY, .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(WebhookPayload, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn(EV_MALFORMED_JSON, .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return null;
    };
    const payload = parsed.value;
    if (payload.event_id.len == 0 or payload.type.len == 0) {
        log.warn("missing_fields", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MISSING_FIELDS);
        parsed.deinit();
        return null;
    }
    return payload;
}

/// Build a WebhookPayload from a Svix delivery: the `svix-id` header is the
/// idempotency `event_id`, the body's top-level `type` (when present) is the
/// event type, and the whole parsed body is forwarded as `data`.
pub fn parseSvixBody(hx: Hx, req: *httpz.Request, fleet_id: []const u8) ?WebhookPayload {
    const svix_id = req.header(svix_verify.SVIX_ID_HEADER) orelse {
        log.warn("missing_svix_id", .{ .error_code = ec.ERR_WEBHOOK_MALFORMED, .fleet_id = fleet_id, .req_id = hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MISSING_FIELDS);
        return null;
    };
    const body = req.body() orelse {
        log.warn(EV_NO_BODY, .{ .error_code = ec.ERR_WEBHOOK_MALFORMED, .fleet_id = fleet_id, .req_id = hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch {
        log.warn(EV_MALFORMED_JSON, .{ .error_code = ec.ERR_WEBHOOK_MALFORMED, .fleet_id = fleet_id, .req_id = hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return .{ .event_id = svix_id, .type = svixEventType(parsed.value), .data = parsed.value };
}

/// Extract the Svix/Clerk event type from a parsed envelope, falling back to
/// `SVIX_DEFAULT_TYPE` when the body is not an object or has no `type` string.
fn svixEventType(value: std.json.Value) []const u8 {
    if (value == .object) {
        if (value.object.get("type")) |t| {
            if (t == .string and t.string.len > 0) return t.string;
        }
    }
    return SVIX_DEFAULT_TYPE;
}

test "WebhookPayload parses valid event" {
    const alloc = std.testing.allocator;
    const body =
        \\{"event_id":"evt_001","type":"email.received","data":{"from":"a@b.com"}}
    ;
    const parsed = try std.json.parseFromSlice(WebhookPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("evt_001", parsed.value.event_id);
    try std.testing.expectEqualStrings("email.received", parsed.value.type);
}

test "WebhookPayload rejects missing event_id" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"email.received","data":{}}
    ;
    const result = std.json.parseFromSlice(WebhookPayload, alloc, body, .{});
    try std.testing.expect(if (result) |_| false else |_| true);
}

test "svixEventType reads top-level type, falls back to default" {
    const alloc = std.testing.allocator;
    // Clerk-style envelope with a top-level type.
    const clerk = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"type":"user.created","data":{"id":"u_1"}}
    , .{});
    defer clerk.deinit();
    try std.testing.expectEqualStrings("user.created", svixEventType(clerk.value));

    // No type field → default label.
    const typeless = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"data":{}}
    , .{});
    defer typeless.deinit();
    try std.testing.expectEqualStrings(SVIX_DEFAULT_TYPE, svixEventType(typeless.value));

    // Non-object body → default label.
    const scalar = try std.json.parseFromSlice(std.json.Value, alloc, "42", .{});
    defer scalar.deinit();
    try std.testing.expectEqualStrings(SVIX_DEFAULT_TYPE, svixEventType(scalar.value));
}
