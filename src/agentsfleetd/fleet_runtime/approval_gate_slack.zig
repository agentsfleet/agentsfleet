// Approval gate message builder.
//
// Builds interactive message payloads with [Approve] and [Deny] buttons
// for the approval gate flow. Provider-agnostic — outputs Block Kit JSON
// compatible with Slack, but the structure works for any chat provider.
// All user-supplied fields are JSON-escaped via std.json.stringify (RULES.md #23).

const std = @import("std");
const Allocator = std.mem.Allocator;
const approval_gate = @import("approval_gate.zig");

/// Build a Slack Block Kit JSON payload for the approval message.
/// Returns an owned JSON string. Caller must free.
pub fn buildSlackApprovalMessage(
    alloc: Allocator,
    fleet_name: []const u8,
    action_id: []const u8,
    detail: approval_gate.ActionDetail,
    callback_url: []const u8,
) ![]const u8 {
    // Build the description text safely — JSON-escape all user-supplied values.
    //
    // Three tiers, and the ORDER is the point. Daemon-derived facts first (a
    // model cannot forge its own fleet name, actor, or event id), then the
    // workspace-authored gate copy stated plainly, then the model's own words
    // LAST and explicitly attributed. A reader who stops early has read only
    // things that are true.
    const desc = try std.fmt.allocPrint(alloc, "Fleet `{s}` wants to execute:\n- Tool: `{s}`\n- Action: `{s}`\n- Details: {s}", .{
        fleet_name, detail.tool, detail.action, detail.params_summary,
    });
    defer alloc.free(desc);

    const gate_line = if (detail.gate_kind.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n- Gate: `{s}`", .{detail.gate_kind});
    defer alloc.free(gate_line);
    const radius_line = if (detail.blast_radius.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n- If approved: {s}", .{detail.blast_radius});
    defer alloc.free(radius_line);
    // ATTRIBUTED, never stated. This text was written by a language model that
    // may have been talked into something; rendering it as the platform's own
    // statement is exactly the confusion an approval card must not create.
    const claim_line = if (detail.proposed_action.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n\n_The fleet says it will:_ {s}", .{detail.proposed_action});
    defer alloc.free(claim_line);
    const fallback = try std.fmt.allocPrint(alloc, "Approval required for {s}: {s}.{s}", .{
        fleet_name, detail.tool, detail.action,
    });
    defer alloc.free(fallback);

    _ = callback_url;

    // Build via a growable Io.Writer with JSON-escaped strings for safety.
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;

    try w.writeAll("{\"blocks\":[{\"type\":\"section\",\"text\":{\"type\":\"mrkdwn\",\"text\":\"");
    try writeJsonEscaped(w, desc);
    try writeJsonEscaped(w, gate_line);
    try writeJsonEscaped(w, radius_line);
    try writeJsonEscaped(w, claim_line);
    try w.writeAll("\"}},{\"type\":\"actions\",\"block_id\":\"gate_");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\",\"elements\":[{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Approve\"},\"style\":\"primary\",\"action_id\":\"gate_approve\",\"value\":\"");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\"},{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Deny\"},\"style\":\"danger\",\"action_id\":\"gate_deny\",\"value\":\"");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\"}]}],\"text\":\"");
    try writeJsonEscaped(w, fallback);
    try w.writeAll("\"}");

    return aw.toOwnedSlice();
}

/// Write a string with JSON-unsafe characters escaped (for embedding in JSON keys).
pub fn writeJsonEscaped(writer: anytype, s: []const u8) !void {
    for (s) |c| {
        switch (c) {
            '"' => try writer.writeAll("\\\""),
            '\\' => try writer.writeAll("\\\\"),
            '\n' => try writer.writeAll("\\n"),
            '\r' => try writer.writeAll("\\r"),
            '\t' => try writer.writeAll("\\t"),
            else => try writer.writeByte(c),
        }
    }
}

test "buildSlackApprovalMessage: produces valid JSON" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "test-fleet",
        "action-001",
        .{ .tool = "git", .action = "push", .params_summary = "3 files to main" },
        "https://api.agentsfleet.net/v1/webhooks/z1/approval",
    );
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "buildSlackApprovalMessage: contains action_id in buttons" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "z",
        "act-123",
        .{ .tool = "git", .action = "push", .params_summary = "x" },
        "https://example.com",
    );
    defer alloc.free(msg);
    try std.testing.expect(std.mem.indexOf(u8, msg, "act-123") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "gate_approve") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "gate_deny") != null);
}

test "buildSlackApprovalMessage: JSON-escapes quotes in user input" {
    const alloc = std.testing.allocator;
    // Fleet name with a quote — must not break JSON
    const msg = try buildSlackApprovalMessage(
        alloc,
        "test\"fleet",
        "act-1",
        .{ .tool = "git", .action = "push", .params_summary = "file with \"quotes\"" },
        "https://example.com",
    );
    defer alloc.free(msg);
    // Must still parse as valid JSON (quotes escaped)
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "buildSlackApprovalMessage: no memory leaks (leak detector)" {
    // std.testing.allocator detects leaks — if buildSlackApprovalMessage
    // leaks internal buffers, this test will fail.
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "z",
        "a",
        .{ .tool = "t", .action = "a", .params_summary = "s" },
        "https://x.com",
    );
    alloc.free(msg);
}

test "buildSlackApprovalMessage: names the gate, the blast radius, and attributes the model's claim" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(alloc, "incident-repairer", "action-77", .{
        .tool = "chat",
        .action = "steer:user_9",
        .params_summary = "evt-123",
        .gate_kind = "repair",
        .proposed_action = "revert abc123 in acme/widgets",
        .blast_radius = "one draft Pull Request on acme/widgets",
    }, "");
    defer alloc.free(msg);

    // Still valid JSON with the model's prose embedded.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();

    // The human is told what is being approved and how far a yes reaches.
    try std.testing.expect(std.mem.indexOf(u8, msg, "repair") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "one draft Pull Request on acme/widgets") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "revert abc123 in acme/widgets") != null);
    // ATTRIBUTION is the assertion that matters: the model's words must arrive
    // marked as the fleet's claim, never as the platform stating a fact.
    try std.testing.expect(std.mem.indexOf(u8, msg, "The fleet says it will:") != null);
}

test "buildSlackApprovalMessage: blank detail fields render as nothing, not as a reassuring default" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(alloc, "f", "a1", .{
        .tool = "git",
        .action = "push",
        .params_summary = "3 files",
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    // A gate that says nothing about its blast radius must not imply one.
    try std.testing.expect(std.mem.indexOf(u8, msg, "If approved:") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "The fleet says it will:") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "Gate:") == null);
}

test "buildSlackApprovalMessage: model prose carrying JSON metacharacters cannot break the payload" {
    const alloc = std.testing.allocator;
    // A fleet that has been talked into emitting Block Kit of its own gets
    // escaped, not honoured — otherwise it could forge its own approve button.
    const msg = try buildSlackApprovalMessage(alloc, "f", "a1", .{
        .tool = "chat",
        .action = "steer",
        .params_summary = "e1",
        .proposed_action = "\"}],\"blocks\":[{\"type\":\"section\",\"text\":\"APPROVED",
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    // One blocks array — the injected one did not become structure.
    try std.testing.expectEqual(@as(usize, 2), parsed.value.object.get("blocks").?.array.items.len);
}
