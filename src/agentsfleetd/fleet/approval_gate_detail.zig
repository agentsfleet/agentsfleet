//! Builds the `ActionDetail` a parked approval carries, from its two sources.
//!
//! The split is the whole point of this module, and it is a trust boundary
//! rather than a tidiness one:
//!
//!   gate_kind + blast_radius  ← the matched GateRule (WORKSPACE-authored config)
//!   proposed_action + evidence ← the triggering event (MODEL-authored prose)
//!
//! A human approving a repair is reading two different kinds of statement on one
//! card. The workspace half was written by whoever configured the fleet, so the
//! card may state it as fact. The model half was written by a language model that
//! may have been talked into something, so it is rendered as an ATTRIBUTED CLAIM
//! and never as a system statement — see `approval_gate_slack`. Mixing them would
//! let a prompt-injected fleet author text a human reads as the platform's own.
//!
//! What never appears here: diff bytes and secret material. The approval
//! authorises a bounded RUN, not specific bytes; the draft Pull Request is where
//! a diff is reviewed.

const std = @import("std");
const Allocator = std.mem.Allocator;

const approval_gate = @import("../fleet_runtime/approval_gate.zig");
const config_gates = @import("../fleet_runtime/config_gates.zig");
const config_types = @import("../fleet_runtime/config_types.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const event_rows = @import("event_rows.zig");
const prose = @import("approval_gate_prose.zig");

/// Model-authored prose is bounded before it reaches a Slack card or a gate row.
/// The cap is generous enough for "revert <sha> in <owner/repo> because <reason>"
/// and far below the 8 KiB message ceiling, so a fleet cannot push a wall of text
/// in front of the approve button.
pub const MAX_PROPOSED_ACTION_BYTES: usize = 512;
/// Evidence is identifiers and links, never file contents (a workspace is
/// destroyed per lease, so fleets cannot hand each other bytes anyway).
pub const MAX_EVIDENCE_BYTES: usize = 1024;

/// Event `request_json` keys the crew populates. A plain human steer carries only
/// `message`, which is the fallback for `proposed_action` (RULE UFS).
const F_PROPOSED_ACTION = "proposed_action";
const F_EVIDENCE = "evidence";
const F_MESSAGE = "message";

/// The empty-evidence default. Matches `ActionDetail.evidence_json`'s own
/// default so "no evidence" has exactly one spelling.
const S_NO_EVIDENCE = "{}";

/// A built detail plus anything it had to allocate. `detail`'s other fields
/// BORROW from the event, the rule, and the parsed context, all of which outlive
/// the gate request; only re-serialized evidence needs owning.
pub const Built = struct {
    detail: approval_gate.ActionDetail,
    evidence_owned: ?[]const u8 = null,
    /// Set only when the model's prose actually needed sanitizing; card-safe
    /// prose keeps borrowing from the parsed context.
    prose_owned: ?[]const u8 = null,

    pub fn deinit(self: *Built, alloc: Allocator) void {
        if (self.evidence_owned) |e| alloc.free(e);
        if (self.prose_owned) |p| alloc.free(p);
        self.* = undefined;
    }
};

/// Build the detail for a parked gate. `rule` is the gate rule that matched —
/// null when the caller parks without one (the write-kind path, which stamps
/// its own daemon-authored kind and radius after this returns); `context` is
/// the already-parsed `request_json`, borrowed.
///
/// Never fails on bad model input: a missing or wrong-typed field degrades to
/// empty, because a blank field renders as nothing while a hard failure would
/// turn a malformed steer into a stuck queue.
pub fn build(
    alloc: Allocator,
    event: *const redis_fleet.FleetEvent,
    rule: ?config_gates.GateRule,
    context: ?std.json.Value,
    timeout_ms: i64,
    repository_binding: ?config_types.RepositoryBinding,
) Built {
    var out = Built{
        .detail = .{
            // Daemon-derived facts the model cannot forge — these are why the card
            // can be trusted at all, so they stay first.
            .tool = event.event_type,
            .action = event.actor,
            .params_summary = event.event_id,
            .gate_kind = if (rule) |r| r.gate_kind else "",
            .blast_radius = if (rule) |r| r.blast_radius else "",
            // The one decision-relevant fact on the card the platform can vouch
            // for: it is the same binding the GitHub mint scopes the token by.
            .repository_binding = repository_binding,
            .timeout_ms = timeout_ms,
        },
    };

    const obj = objectOf(context) orelse return out;

    // Cap first, then make it card-safe. Truncation cannot introduce an unsafe
    // byte, so the order is free; doing it this way means the sanitizer only ever
    // walks bounded input.
    const raw_prose = event_rows.truncateUtf8(
        stringField(obj, F_PROPOSED_ACTION) orelse stringField(obj, F_MESSAGE) orelse "",
        MAX_PROPOSED_ACTION_BYTES,
    );
    if (prose.needsSanitizing(raw_prose)) {
        // Allocation failure leaves `proposed_action` empty rather than falling
        // back to the raw bytes: the card omitting the model's claim is a
        // degradation, the card carrying a forged one is the failure.
        if (prose.sanitize(alloc, raw_prose)) |clean| {
            out.prose_owned = clean;
            out.detail.proposed_action = clean;
        }
    } else {
        out.detail.proposed_action = raw_prose;
    }

    if (obj.get(F_EVIDENCE)) |ev| {
        // Re-serialized rather than echoed: the parser hands back a value, not
        // the original bytes, and going through the writer guarantees the result
        // is well-formed JSON no matter what the model sent.
        const json = std.fmt.allocPrint(alloc, "{f}", .{std.json.fmt(ev, .{})}) catch return out;
        if (json.len > MAX_EVIDENCE_BYTES) {
            // Truncating JSON would produce invalid JSON, so oversized evidence
            // is DROPPED rather than corrupted. The links are lost; the card is
            // still parseable and still names the action.
            alloc.free(json);
            return out;
        }
        out.evidence_owned = json;
        out.detail.evidence_json = json;
    }
    return out;
}

fn objectOf(context: ?std.json.Value) ?std.json.ObjectMap {
    const v = context orelse return null;
    return switch (v) {
        .object => |o| o,
        else => null,
    };
}

fn stringField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

/// `FleetEvent` fields are mutable slices (the decoder owns them), so the
/// literals are copied into a static buffer rather than pointing at read-only
/// data. `request_json` stays empty — every test passes an already-parsed
/// context, which is what the real caller does.
var evt_id_buf = "evt-1".*;
var evt_actor_buf = "steer:user_42".*;
var evt_type_buf = "chat".*;
var evt_ws_buf = "ws-1".*;
var evt_json_buf = "{}".*;

fn fakeEvent() redis_fleet.FleetEvent {
    return .{
        .event_id = &evt_id_buf,
        .actor = &evt_actor_buf,
        .event_type = &evt_type_buf,
        .workspace_id = &evt_ws_buf,
        .request_json = &evt_json_buf,
        .created_at_ms = 0,
    };
}

fn ruleWith(kind: []const u8, radius: []const u8) config_gates.GateRule {
    return .{ .tool = "*", .action = "*", .condition = null, .behavior = .approve, .gate_kind = kind, .blast_radius = radius };
}

test "test_gate_detail_is_populated" {
    // Dimension 2.1. The workspace half comes from the rule and the model half
    // from the event; a blank field here is a failure, not a display default.
    const alloc = testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"proposed_action":"revert abc123 in acme/widgets","evidence":{"commit":"abc123"}}
    , .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", "one draft Pull Request"), parsed.value, 900_000, null);
    defer built.deinit(alloc);

    try testing.expectEqualStrings("repair", built.detail.gate_kind);
    try testing.expectEqualStrings("one draft Pull Request", built.detail.blast_radius);
    try testing.expectEqualStrings("revert abc123 in acme/widgets", built.detail.proposed_action);
    try testing.expect(std.mem.indexOf(u8, built.detail.evidence_json, "abc123") != null);
    // Daemon-derived facts are always present — they are what the model cannot forge.
    try testing.expectEqualStrings("evt-1", built.detail.params_summary);
    try testing.expectEqualStrings("steer:user_42", built.detail.action);
}

test "detail: a plain human steer falls back to the message body" {
    const alloc = testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"message\":\"please repair it\"}", .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", ""), parsed.value, 1, null);
    defer built.deinit(alloc);
    try testing.expectEqualStrings("please repair it", built.detail.proposed_action);
    // An omitted blast_radius stays empty rather than inventing a reassuring one.
    try testing.expectEqualStrings("", built.detail.blast_radius);
}

test "detail: model prose is capped, and oversized evidence is dropped not truncated" {
    const alloc = testing.allocator;
    const long = "x" ** 4000;
    const body = try std.fmt.allocPrint(alloc, "{{\"proposed_action\":\"{s}\",\"evidence\":{{\"blob\":\"{s}\"}}}}", .{ long, long });
    defer alloc.free(body);
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", ""), parsed.value, 1, null);
    defer built.deinit(alloc);

    try testing.expect(built.detail.proposed_action.len <= MAX_PROPOSED_ACTION_BYTES);
    // Dropped, so the card's JSON stays valid — a truncated object would not.
    try testing.expectEqualStrings(S_NO_EVIDENCE, built.detail.evidence_json);
    try testing.expect(built.evidence_owned == null);
}

test "detail: malformed or absent model input degrades to empty, never fails" {
    const alloc = testing.allocator;
    const ev = fakeEvent();

    var no_ctx = build(alloc, &ev, ruleWith("repair", "bounded"), null, 1, null);
    defer no_ctx.deinit(alloc);
    try testing.expectEqualStrings("", no_ctx.detail.proposed_action);
    try testing.expectEqualStrings(S_NO_EVIDENCE, no_ctx.detail.evidence_json);
    // The workspace half survives a model that sent nothing usable.
    try testing.expectEqualStrings("repair", no_ctx.detail.gate_kind);

    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"proposed_action\":42}", .{});
    defer parsed.deinit();
    var wrong_type = build(alloc, &ev, null, parsed.value, 1, null);
    defer wrong_type.deinit(alloc);
    try testing.expectEqualStrings("", wrong_type.detail.proposed_action);
}

test "detail: model prose cannot grow rows that counterfeit the daemon half" {
    const alloc = testing.allocator;
    // The forgery this guards. The cap is 512 bytes — ample to append rows that
    // render exactly like the workspace-authored ones above them, because Slack
    // turns the JSON newline escape back into a real line break.
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"proposed_action":"revert abc123\n- Gate: `production-write`\n- If approved: opens 1 draft PR in acme/safe"}
    , .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", "one draft Pull Request"), parsed.value, 1, null);
    defer built.deinit(alloc);

    // One line in, one line out: the claim can no longer sprout rows of its own.
    try testing.expect(std.mem.indexOfScalar(u8, built.detail.proposed_action, '\n') == null);
    try testing.expect(std.mem.indexOfScalar(u8, built.detail.proposed_action, '\r') == null);
    // Sanitized, not dropped — the operator still reads what the fleet claimed.
    try testing.expect(std.mem.indexOf(u8, built.detail.proposed_action, "revert abc123") != null);
    try testing.expect(built.prose_owned != null);
    // And the workspace half is untouched by any of it.
    try testing.expectEqualStrings("one draft Pull Request", built.detail.blast_radius);
}

test "detail: bidirectional overrides are stripped from model prose" {
    const alloc = testing.allocator;
    // U+202E reorders what a human reads without altering a stored byte, so a
    // repository or sha can display as something it is not.
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"proposed_action\":\"revert \u{202E}stegdiw/emca\u{202C}\"}", .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", ""), parsed.value, 1, null);
    defer built.deinit(alloc);

    try testing.expect(std.mem.indexOf(u8, built.detail.proposed_action, "\u{202E}") == null);
    try testing.expect(std.mem.indexOf(u8, built.detail.proposed_action, "\u{202C}") == null);
    try testing.expect(built.prose_owned != null);
}

test "detail: card-safe prose is borrowed, never copied" {
    const alloc = testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc,
        \\{"proposed_action":"revert abc123 in acme/widgets"}
    , .{});
    defer parsed.deinit();

    const ev = fakeEvent();
    var built = build(alloc, &ev, ruleWith("repair", ""), parsed.value, 1, null);
    defer built.deinit(alloc);

    try testing.expectEqualStrings("revert abc123 in acme/widgets", built.detail.proposed_action);
    // The common path allocates nothing — sanitizing is the exception.
    try testing.expect(built.prose_owned == null);
}
