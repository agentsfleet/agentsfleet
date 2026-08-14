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

test "matchWorkspaceSuffixAction: workspace events/stream is distinct from the bare events collection" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const ws = matchers.matchWorkspaceSuffixAction(parse("/v1/workspaces/ws_abc/events/stream", &buf), "events", "stream").?;
    try std.testing.expectEqualStrings("ws_abc", ws);
    // The 3-segment collection is NOT this shape — the two never collide.
    try std.testing.expect(matchers.matchWorkspaceSuffixAction(parse("/v1/workspaces/ws_abc/events", &buf), "events", "stream") == null);
    // The per-fleet stream (6 segments) is a different matcher entirely.
    try std.testing.expect(matchers.matchWorkspaceSuffixAction(parse("/v1/workspaces/ws_abc/fleets/z1/events/stream", &buf), "events", "stream") == null);
    // Empty workspace id rejects at the matcher.
    try std.testing.expect(matchers.matchWorkspaceSuffixAction(parse("/v1/workspaces//events/stream", &buf), "events", "stream") == null);
    // Wrong trailing action does not match.
    try std.testing.expect(matchers.matchWorkspaceSuffixAction(parse("/v1/workspaces/ws_abc/events/rollup", &buf), "events", "stream") == null);
}

test "match resolves the workspace multiplexed stream and rejects non-GET" {
    const router = @import("router.zig");
    switch (router.match("/v1/workspaces/ws_abc/events/stream", .GET).?) {
        .workspace_events_stream => |ws| try std.testing.expectEqualStrings("ws_abc", ws),
        else => return error.TestExpectedEqual,
    }
    // The bare collection still resolves to the list, not the stream.
    switch (router.match("/v1/workspaces/ws_abc/events", .GET).?) {
        .workspace_events => |ws| try std.testing.expectEqualStrings("ws_abc", ws),
        else => return error.TestExpectedEqual,
    }
    // Only GET streams.
    try std.testing.expect(router.match("/v1/workspaces/ws_abc/events/stream", .POST) == null);
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
    // A 3-segment path is an action route, never the bare webhook receiver.
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/zid/approval", &buf)) == null);
    try std.testing.expect(matchers.matchWebhook(parse("/v1/webhooks/zid/grant-approval", &buf)) == null);
}

test "matchWebhookAction: /approval, /github, and a hyphenated action; rejects /svix/* prefix" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    try std.testing.expectEqualStrings(
        "zid",
        matchers.matchWebhookAction(parse("/v1/webhooks/zid/approval", &buf), "approval").?,
    );
    // A hyphenated multi-word action still resolves. The matcher is generic:
    // it is the router that decides which actions exist, and "grant-approval"
    // is no longer one of them — see router_test.zig.
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

test "matchCliCredentialById: only the item form matches, and an empty id is refused" {
    var b: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    const id = "01920000-0000-7000-8000-000000000001";
    try std.testing.expectEqualStrings(
        id,
        matchers.matchCliCredentialById(parse("/v1/cli-credentials/" ++ id, &b)).?,
    );

    // The bare collection is exact-matched in `router.match()`. If this matcher
    // claimed it too, POST and GET would reach the item handler — which only
    // accepts DELETE — and minting would answer 405.
    try std.testing.expect(matchers.matchCliCredentialById(parse("/v1/cli-credentials", &b)) == null);

    // `/cli-credentials/` parses to an empty second segment. `param()` refuses
    // it at the matcher boundary, so the handler never receives an empty
    // identifier and never has to decide what one means.
    try std.testing.expect(matchers.matchCliCredentialById(parse("/v1/cli-credentials/", &b)) == null);

    // Anything deeper belongs to no route in this family.
    try std.testing.expect(matchers.matchCliCredentialById(parse("/v1/cli-credentials/" ++ id ++ "/extra", &b)) == null);

    // A neighbouring two-segment collection must not be captured — both are
    // `{collection}/{id}` and only the first segment tells them apart.
    try std.testing.expect(matchers.matchCliCredentialById(parse("/v1/api-keys/" ++ id, &b)) == null);

    // The segment is returned verbatim: shape-checking an identifier is the
    // handler's job, where it answers a typed refusal. A matcher that
    // pre-filtered it would turn a malformed id into a 404 instead.
    try std.testing.expectEqualStrings(
        "not-a-uuid",
        matchers.matchCliCredentialById(parse("/v1/cli-credentials/not-a-uuid", &b)).?,
    );
}
