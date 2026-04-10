// Slack Block Kit message builder for approval gate messages.
//
// Builds interactive message payloads with [Approve] and [Deny] buttons
// for the approval gate flow.

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
    return std.fmt.allocPrint(alloc,
        \\{{"blocks":[{{"type":"section","text":{{"type":"mrkdwn","text":"*Approval Required*\nZombie `{s}` wants to execute:\n- Tool: `{s}`\n- Action: `{s}`\n- Details: {s}"}}}},{{"type":"actions","block_id":"gate_{s}","elements":[{{"type":"button","text":{{"type":"plain_text","text":"Approve"}},"style":"primary","action_id":"gate_approve","value":"{s}"}},{{"type":"button","text":{{"type":"plain_text","text":"Deny"}},"style":"danger","action_id":"gate_deny","value":"{s}"}}]}}],"text":"Approval required for {s}: {s}.{s} — {s}"}}
    , .{
        zombie_name,
        detail.tool,
        detail.action,
        detail.params_summary,
        action_id,
        action_id,
        action_id,
        zombie_name,
        detail.tool,
        detail.action,
        callback_url,
    });
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
