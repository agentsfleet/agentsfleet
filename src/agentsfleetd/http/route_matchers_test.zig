// Tests for route_matchers.zig — kept in a sibling file so the production
// module stays under the file-length budget.

const std = @import("std");
const matchers = @import("route_matchers.zig");

/// Test helper: parse a `/v1/...` path and return the version-stripped Path
/// that matchers operate on. Mirrors the strip the dispatcher does in
/// `router.zig::match()` before calling `matchV1`.
fn parse(s: []const u8, buf: *[matchers.PATH_MAX_SEGMENTS][]const u8) matchers.Path {
    const full = matchers.Path.parse(s, buf);
    if (full.segs.len > 0 and std.mem.eql(u8, full.segs[0], "v1")) return full.tail(1);
    return full;
}

test "Path.parse: preserves empty segments (trailing/double slash visible to matchers)" {
    var b: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    // Note: `parse` here is the test helper that strips a leading `/v1/`. These
    // paths have no version prefix so the helper returns the raw parse.
    try std.testing.expectEqual(@as(usize, 3), matchers.Path.parse("/a/b/c", &b).segs.len);
    // Trailing slash adds an empty trailing segment.
    try std.testing.expectEqual(@as(usize, 4), matchers.Path.parse("/a/b/c/", &b).segs.len);
    // Leading-slash-less paths skip the (absent) leading marker.
    try std.testing.expectEqual(@as(usize, 3), matchers.Path.parse("a/b/c", &b).segs.len);
    // Double slash inside leaves an empty internal segment.
    try std.testing.expectEqual(@as(usize, 3), matchers.Path.parse("/a//b", &b).segs.len);
    // Empty path → no segments.
    try std.testing.expectEqual(@as(usize, 0), matchers.Path.parse("", &b).segs.len);
    // Bare slash → no segments.
    try std.testing.expectEqual(@as(usize, 0), matchers.Path.parse("/", &b).segs.len);
}

test "Path.param: returns null for empty segments and out-of-bounds" {
    var b: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const p = matchers.Path.parse("/a//c", &b);
    try std.testing.expectEqual(@as(usize, 3), p.segs.len);
    try std.testing.expectEqualStrings("a", p.param(0).?);
    try std.testing.expect(p.param(1) == null); // empty middle segment
    try std.testing.expectEqualStrings("c", p.param(2).?);
    try std.testing.expect(p.param(3) == null); // out of bounds
}

test "Path.parse: overflow returns empty view (no partial match)" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    // Build a path with PATH_MAX_SEGMENTS + 2 segments.
    var deep_buf: [256]u8 = undefined;
    var n: usize = 0;
    var i: usize = 0;
    while (i < matchers.PATH_MAX_SEGMENTS + 2) : (i += 1) {
        deep_buf[n] = '/';
        deep_buf[n + 1] = 'a';
        n += 2;
    }
    const view = parse(deep_buf[0..n], &buf);
    try std.testing.expectEqual(@as(usize, 0), view.segs.len);
}

test "matchWorkspaceSecret: workspace_id and secret_name" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceSecret(parse("/v1/workspaces/ws1/secrets/fly", &buf)).?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("fly", r.secret_name);
    try std.testing.expect(matchers.matchWorkspaceSecret(parse("/v1/workspaces/ws1/secrets/", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceSecret(parse("/v1/workspaces//secrets/fly", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceSecret(parse("/v1/workspaces/ws1/secrets", &buf)) == null);
}

test "matchWorkspaceFleetKeyDelete: workspace_id and fleet_key_id" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceFleetKeyDelete(parse("/v1/workspaces/ws1/fleet-keys/ag1", &buf)).?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("ag1", r.fleet_key_id);
    try std.testing.expect(matchers.matchWorkspaceFleetKeyDelete(parse("/v1/workspaces/ws1/fleet-keys/", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleetKeyDelete(parse("/v1/workspaces//fleet-keys/ag1", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleetKeyDelete(parse("/v1/workspaces/a/b/fleet-keys/ag1", &buf)) == null);
}

test "matchWorkspaceFleetGrant: ws_id, fleet_id, grant_id" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces/ws1/fleets/z1/integration-grants/g1", &buf)).?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.fleet_id);
    try std.testing.expectEqualStrings("g1", r.grant_id);
    const f = matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces/ws1/fleets/z1/integration-grants/g1", &buf)).?;
    try std.testing.expectEqualStrings("ws1", f.workspace_id);
    try std.testing.expectEqualStrings("z1", f.fleet_id);
    try std.testing.expectEqualStrings("g1", f.grant_id);
    try std.testing.expect(matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces/ws1/fleets/z1/integration-grants/", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces//fleets/z1/integration-grants/g1", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces/ws1/fleets//integration-grants/g1", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleetGrant(parse("/v1/workspaces/ws1/fleets/z1/x/integration-grants/g1", &buf)) == null);
}

test "matchWorkspaceFleet: workspace_id and fleet_id extracted" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceFleet(parse("/v1/workspaces/ws_1/fleets/z_1", &buf)).?;
    try std.testing.expectEqualStrings("ws_1", r.workspace_id);
    try std.testing.expectEqualStrings("z_1", r.fleet_id);
    const f = matchers.matchWorkspaceFleet(parse("/v1/workspaces/ws_1/fleets/z_1", &buf)).?;
    try std.testing.expectEqualStrings("ws_1", f.workspace_id);
    try std.testing.expectEqualStrings("z_1", f.fleet_id);
    try std.testing.expect(matchers.matchWorkspaceFleet(parse("/v1/workspaces/ws_1/fleets/", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleet(parse("/v1/workspaces//fleets/z_1", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleet(parse("/v1/workspaces/a/b/fleets/z_1", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleet(parse("/v1/workspaces/ws_1/fleets/z_1/extra", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceFleet(parse("/v1/workspaces/ws_1/fleets/bundles", &buf)) == null);
}

test "matchWorkspaceFleetAction: /messages extracts ws_id + fleet_id" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/ws1/fleets/z1/messages", &buf), "messages").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.fleet_id);
    const f = matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/ws1/fleets/z1/messages", &buf), "messages").?;
    try std.testing.expectEqualStrings("ws1", f.workspace_id);
    try std.testing.expectEqualStrings("z1", f.fleet_id);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/ws1/fleets//messages", &buf), "messages") == null);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/workspaces//fleets/z1/messages", &buf), "messages") == null);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/ws1/fleets/a/b/messages", &buf), "messages") == null);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/a/b/fleets/z1/messages", &buf), "messages") == null);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/workspaces/ws1/fleets/z1/other-action", &buf), "messages") == null);
    try std.testing.expect(matchers.matchWorkspaceFleetAction(parse("/v1/fleets/z1/messages", &buf), "messages") == null);
}

test "matchWorkspaceFleetEventsStream: 7-segment shape" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceFleetEventsStream(parse("/v1/workspaces/ws_abc/fleets/z_123/events/stream", &buf)).?;
    try std.testing.expectEqualStrings("ws_abc", r.workspace_id);
    try std.testing.expectEqualStrings("z_123", r.fleet_id);
    const f = matchers.matchWorkspaceFleetEventsStream(parse("/v1/workspaces/ws_abc/fleets/z_123/events/stream", &buf)).?;
    try std.testing.expectEqualStrings("ws_abc", f.workspace_id);
    try std.testing.expectEqualStrings("z_123", f.fleet_id);
    try std.testing.expect(matchers.matchWorkspaceFleetEventsStream(parse("/v1/workspaces/ws_abc/fleets/z_123/events", &buf)) == null);
}

test "matchWebhook: HMAC-only 2-segment form" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const r1 = matchers.matchWebhook(parse("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", &buf)).?;
    try std.testing.expectEqualStrings(id, r1);
    // 3-segment forms are matched per-action by matchWebhookAction; matchWebhook
    // rejects them outright (the URL-embedded-secret variant was removed earlier).
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/kR7x2mN", &buf)) == null);
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/", &buf)) == null);
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks", &buf)) == null);
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/a/b/c", &buf)) == null);
}

test "matchWebhook: rejects reserved second segment (svix) and reserved actions" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    // /v1/webhooks/svix/{id} routes to receive_svix_webhook.
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/svix/zid", &buf)) == null);
    // /v1/webhooks/{id}/approval and /grant-approval route to dedicated handlers.
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/zid/approval", &buf)) == null);
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/zid/grant-approval", &buf)) == null);
}

test "matchWebhookAction: /approval, /grant-approval, /github; rejects /svix/* prefix" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    try std.testing.expectEqualStrings(
        "zid",
        matchers.matchWebhookAction(parse("/v1/webhooks/zid/approval", &buf), "approval").?,
    );
    try std.testing.expectEqualStrings(
        "zid",
        matchers.matchWebhookAction(parse("/v1/webhooks/zid/grant-approval", &buf), "grant-approval").?,
    );
    try std.testing.expectEqualStrings(
        "zid",
        matchers.matchWebhookAction(parse("/v1/webhooks/zid/github", &buf), "github").?,
    );
    try std.testing.expect(matchers.matchWebhookAction(parse("/v1/webhooks/svix/approval", &buf), "approval") == null);
}

test "matchSvixWebhook: /v1/webhooks/svix/{fleet_id}" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    try std.testing.expectEqualStrings(
        "zid_1",
        matchers.matchSvixWebhook(parse("/v1/webhooks/svix/zid_1", &buf)).?,
    );
    try std.testing.expect(matchers.matchSvixWebhook(parse("/v1/webhooks/svix/", &buf)) == null);
    try std.testing.expect(matchers.matchSvixWebhook(parse("/v1/webhooks/zid_1/svix", &buf)) == null);
}

test "matchWorkspaceApprovalResolve: approve and deny" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceApprovalResolve(parse("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999:approve", &buf)).?;
    try std.testing.expectEqualStrings("ws_1", r.workspace_id);
    try std.testing.expectEqualStrings("01999999-9999-7999-9999-999999999999", r.gate_id);
    try std.testing.expectEqual(matchers.ApprovalResolveDecision.approve, r.decision);
    const d = matchers.matchWorkspaceApprovalResolve(parse("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999:deny", &buf)).?;
    try std.testing.expectEqual(matchers.ApprovalResolveDecision.deny, d.decision);
}

test "matchWorkspaceApprovalResolve: rejects malformed paths" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    try std.testing.expect(matchers.matchWorkspaceApprovalResolve(parse("/v1/workspaces/ws_1/approvals/abc", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceApprovalResolve(parse("/v1/workspaces/ws_1/approvals/abc:other", &buf)) == null);
    try std.testing.expect(matchers.matchWorkspaceApprovalResolve(parse("/v1/workspaces/ws_1/approvals/abc/x:approve", &buf)) == null);
}

test "matchWorkspaceApprovalGate: bare gate id" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const r = matchers.matchWorkspaceApprovalGate(parse("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999", &buf)).?;
    try std.testing.expectEqualStrings("01999999-9999-7999-9999-999999999999", r.gate_id);
    try std.testing.expect(matchers.matchWorkspaceApprovalGate(parse("/v1/workspaces/ws_1/approvals/abc:approve", &buf)) == null);
}
