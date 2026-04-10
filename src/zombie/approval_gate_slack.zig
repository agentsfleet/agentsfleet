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
    zombie_name: []const u8,
    action_id: []const u8,
    detail: approval_gate.ActionDetail,
    callback_url: []const u8,
) ![]const u8 {
    // Build the description text safely — JSON-escape all user-supplied values
    const desc = try std.fmt.allocPrint(alloc, "Zombie `{s}` wants to execute:\n- Tool: `{s}`\n- Action: `{s}`\n- Details: {s}", .{
        zombie_name, detail.tool, detail.action, detail.params_summary,
    });
    defer alloc.free(desc);
    const fallback = try std.fmt.allocPrint(alloc, "Approval required for {s}: {s}.{s}", .{
        zombie_name, detail.tool, detail.action,
    });
    defer alloc.free(fallback);

    // Use std.json.stringify for the full payload to ensure valid JSON output
    const payload = .{
        .blocks = [_]@TypeOf(sectionBlock(desc)){sectionBlock(desc)} ++
            [_]@TypeOf(actionsBlock(action_id)){actionsBlock(action_id)},
        .text = fallback,
    };
    _ = payload;
    _ = callback_url;

    // Build via allocPrint with JSON-escaped strings for safety
    var buf = std.ArrayList(u8).init(alloc);
    errdefer buf.deinit();
    const w = buf.writer();

    try w.writeAll("{\"blocks\":[{\"type\":\"section\",\"text\":{\"type\":\"mrkdwn\",\"text\":");
    try std.json.stringify(desc, .{}, w);
    try w.writeAll("}},{\"type\":\"actions\",\"block_id\":\"gate_");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\",\"elements\":[{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Approve\"},\"style\":\"primary\",\"action_id\":\"gate_approve\",\"value\":");
    try std.json.stringify(action_id, .{}, w);
    try w.writeAll("},{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Deny\"},\"style\":\"danger\",\"action_id\":\"gate_deny\",\"value\":");
    try std.json.stringify(action_id, .{}, w);
    try w.writeAll("}]}],\"text\":");
    try std.json.stringify(fallback, .{}, w);
    try w.writeAll("}");

    return buf.toOwnedSlice();
}

fn sectionBlock(text: []const u8) struct { type: []const u8, text: []const u8 } {
    return .{ .type = "section", .text = text };
}

fn actionsBlock(action_id: []const u8) struct { type: []const u8, block_id: []const u8 } {
    return .{ .type = "actions", .block_id = action_id };
}

/// Write a string with JSON-unsafe characters escaped (for embedding in JSON keys).
fn writeJsonEscaped(writer: anytype, s: []const u8) !void {
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
        "test-zombie",
        "action-001",
        .{ .tool = "git", .action = "push", .params_summary = "3 files to main" },
        "https://api.usezombie.com/v1/webhooks/z1:approval",
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
    // Zombie name with a quote — must not break JSON
    const msg = try buildSlackApprovalMessage(
        alloc,
        "test\"zombie",
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
