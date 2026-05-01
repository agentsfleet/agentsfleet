// Tests for http/router.zig — split out to keep router.zig under 400 lines.

const std = @import("std");
const httpz = @import("httpz");
const router = @import("router.zig");
const matchers = @import("route_matchers.zig");
const Route = router.Route;
const match = router.match;

fn parseMethod(s: []const u8) httpz.Method {
    if (std.mem.eql(u8, s, "GET")) return .GET;
    if (std.mem.eql(u8, s, "POST")) return .POST;
    if (std.mem.eql(u8, s, "PUT")) return .PUT;
    if (std.mem.eql(u8, s, "PATCH")) return .PATCH;
    if (std.mem.eql(u8, s, "DELETE")) return .DELETE;
    if (std.mem.eql(u8, s, "HEAD")) return .HEAD;
    if (std.mem.eql(u8, s, "OPTIONS")) return .OPTIONS;
    return .OTHER;
}

test "tenant billing route resolves" {
    try std.testing.expectEqualDeep(Route.get_tenant_billing, match("/v1/tenants/me/billing", .GET).?);
}

test "removed workspace billing routes are 404 (pre-v2.0 per RULE EP4)" {
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/events", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/scale", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/summary", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/zombies/z_1/billing/summary", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/scoring/config", .GET) == null);
}

test "match rejects /v1/agents paths after agent_profiles removal (M17_001)" {
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/scores", .GET) == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/improvement-report", .GET) == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals", .GET) == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/harness/changes/c1:revert", .GET) == null);
    try std.testing.expect(match("/v1/agents/", .GET) == null);
    try std.testing.expect(match("/v1/agents/foo/bar/scores", .GET) == null);
}

test "match resolves auth routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions", .GET).?);
    // GET dispatches to poll_auth_session.
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .GET).?) {
            .poll_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    // PATCH dispatches to patch_auth_session.
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .PATCH).?) {
            .patch_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    // The retired POST .../complete suffix no longer dispatches to any route.
    try std.testing.expect(match("/v1/auth/sessions/sess_1/complete", .POST) == null);
}

// M26_001: memory recall/list are matched regardless of HTTP method — method
// enforcement happens in route_table_invoke.zig. This test pins the path match
// so a regression that drops the routes can be caught without spinning up a
// server. Spec §2.1 — GET /v1/memory/recall resolves to .memory_recall.
test "match resolves memory recall/list/store/forget routes" {
    try std.testing.expectEqualDeep(Route.memory_store, match("/v1/memory/store", .GET).?);
    try std.testing.expectEqualDeep(Route.memory_recall, match("/v1/memory/recall", .GET).?);
    try std.testing.expectEqualDeep(Route.memory_list, match("/v1/memory/list", .GET).?);
    try std.testing.expectEqualDeep(Route.memory_forget, match("/v1/memory/forget", .GET).?);
    // Query-string suffixes are stripped by the httpz layer before match() runs;
    // path-only match is what we pin here.
    try std.testing.expect(match("/v1/memory/recall/", .GET) == null); // trailing slash is NOT accepted
    try std.testing.expect(match("/v1/memory/unknown", .GET) == null);
}

// M10_001: /v1/runs/* routes removed — get_run, retry_run, replay_run,
// stream_run, cancel_run variants deleted from Route union.
test "M10_001: run paths no longer match any route" {
    try std.testing.expect(match("/v1/runs/run_1", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:retry", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:replay", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:stream", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:cancel", .GET) == null);
}

// ── M16_004 route tests ───────────────────────────────────────────────────────

test "match resolves admin platform key routes (M16_004)" {
    try std.testing.expectEqualDeep(Route.admin_platform_keys, match("/v1/admin/platform-keys", .GET).?);
    try std.testing.expectEqualStrings(
        "anthropic",
        switch (match("/v1/admin/platform-keys/anthropic", .GET).?) {
            .delete_admin_platform_key => |provider| provider,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/admin/platform-keys/a/b", .GET) == null);
    try std.testing.expect(match("/v1/admin/platform-keys/", .GET) == null);
}

test "match returns null for removed spec relay routes" {
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/template", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/preview", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/spec/template", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/spec", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/spec/", .GET) == null);
}

test "match resolves workspace LLM credential route (M16_004)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm", .GET).?) {
            .workspace_llm_credential => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/credentials/llm", .GET) == null);
}

// M10_001: matchRunAction tests, M16_002 run action tests, M17_001 cancel tests
// all removed — Route variants (retry_run, replay_run, stream_run, cancel_run,
// get_run) deleted. Run path null-match covered in "run paths no longer match" above.

// ── M1_001 webhook route tests ────────────────────────────────────────────

test "M1_001: webhook routes resolve and reject correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    // Webhook tests for matchWebhookRoute are in route_matchers.zig.
    // Test via match() integration:
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .receive_webhook => |r| r.zombie_id,
        else => return error.TestExpectedEqual,
    });
}

// ── M2_001 zombie CRUD route tests ────────────────────────────────────

test "M24_001: workspace-scoped zombie collection resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies", .GET).?) {
        .workspace_zombies => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M24_001: flat /v1/zombies/ is removed (pre-v2.0 bare 404 per RULE EP4)" {
    try std.testing.expect(match("/v1/zombies/", .GET) == null);
}

test "workspace-scoped zombie PATCH resolves to patch_workspace_zombie" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const r = match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET).?;
    switch (r) {
        .patch_workspace_zombie => |route| {
            try std.testing.expectEqualStrings(ws_id, route.workspace_id);
            try std.testing.expectEqualStrings(zid, route.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
}

test "retired path: /v1/workspaces/{ws}/zombies/{id}/kill no longer resolves" {
    // The standalone POST .../kill endpoint folded into PATCH .../zombies/{id}
    // with body {status:"killed"}. The verb path 404s; aborting the in-flight
    // stage (a different semantic) still lives at DELETE .../current-run.
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/kill", .POST) == null);
}

test "M24_001: workspace-scoped credentials route resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials", .GET).?) {
        .workspace_credentials => |id| id,
        else => return error.TestExpectedEqual,
    });
    try std.testing.expect(match("/v1/zombies/credentials", .GET) == null);
}

test "M24_001: /credentials/llm still distinct from /credentials" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm", .GET).?) {
        .workspace_llm_credential => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M24_001: flat /v1/zombies/{id} DELETE path is removed (bare 404 per RULE EP4)" {
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
}

test "M2_001: zombie routes reject invalid paths" {
    try std.testing.expect(match("/v1/zombies/a/b", .GET) == null);
    try std.testing.expect(match("/v1/zombies", .GET) == null);
}

// ── M4_001 approval gate route tests ────────────────────────────────────

test "M4_001: approval webhook route resolves correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/approval", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .approval_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M4_001: approval route does not interfere with regular webhook" {
    const route = match("/v1/webhooks/z1", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "M4_001: approval route resolves before webhook route" {
    // /approval suffix is matched before the generic webhook route
    const route = match("/v1/webhooks/z1/approval", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .approval_webhook => {},
        else => return error.TestExpectedEqual,
    }
    // Without /approval suffix, goes to receive_webhook
    const route2 = match("/v1/webhooks/z1", .GET) orelse return error.TestExpectedMatch;
    switch (route2) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "svix webhook route resolves with zombie_id" {
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const route = match("/v1/webhooks/svix/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zid, switch (route) {
        .receive_svix_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "svix route takes precedence over generic /v1/webhooks/{id}/{secret}" {
    // /v1/webhooks/svix/{id} must NOT be interpreted as {zombie_id=svix, secret=id}.
    const route = match("/v1/webhooks/svix/zomb-abc", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .receive_svix_webhook => |id| try std.testing.expectEqualStrings("zomb-abc", id),
        else => return error.TestExpectedEqual,
    }
}

test "svix route rejects empty and multi-segment zombie_id" {
    try std.testing.expect(match("/v1/webhooks/svix/", .GET) == null);
    try std.testing.expect(match("/v1/webhooks/svix/a/b", .GET) == null);
}

// ── Custom-method → subpath migration (see v0.19.0 Upgrading) ─────────

test "custom-method subpath: /grant-approval resolves before /approval" {
    // grant_approval_webhook must match first because "/grant-approval" ends with "/approval"
    // only by coincidence; the longer suffix is checked first in router.zig.
    const zid = "z1";
    const route = match("/v1/webhooks/z1/grant-approval", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zid, switch (route) {
        .grant_approval_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "custom-method subpath: /grant-approval is distinct from /approval" {
    // A path ending in "/approval" must NOT route to grant_approval_webhook.
    const route = match("/v1/webhooks/z1/approval", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .approval_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "PATCH /v1/workspaces/{id} resolves to patch_workspace" {
    const route = match("/v1/workspaces/ws_123", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("ws_123", switch (route) {
        .patch_workspace => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "old verb-suffix /v1/workspaces/{ws}/pause no longer resolves" {
    // Migrated to PATCH /v1/workspaces/{id} body {pause, reason, version}.
    // Multi-segment path is rejected by isSingleSegment guard.
    try std.testing.expect(match("/v1/workspaces/ws_123/pause", .GET) == null);
}

test "retired path: /v1/workspaces/{ws}/sync no longer resolves" {
    // Pipeline v1 sync_specs endpoint removed. Requests against the URL
    // must not match any current route — any future re-introduction of a
    // workspace-scoped /sync action should be deliberate.
    try std.testing.expect(match("/v1/workspaces/ws_123/sync", .GET) == null);
}

test "custom-method subpath: zombie /messages resolves" {
    const ws_id = "ws_abc";
    const zid = "z_xyz";
    const route = match("/v1/workspaces/ws_abc/zombies/z_xyz/messages", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .workspace_zombie_messages => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
}

test "subpath: zombie /current-run resolves" {
    const route = match("/v1/workspaces/ws_abc/zombies/z_xyz/current-run", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .workspace_zombie_current_run => |r| {
            try std.testing.expectEqualStrings("ws_abc", r.workspace_id);
            try std.testing.expectEqualStrings("z_xyz", r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
}

test "retired path: /stop no longer resolves as a zombie action" {
    // /stop was the pre-hygiene path-verb form. After the REST cleanup it must
    // either not match or fall through to a plain resource route — it must not
    // dispatch to the current-run handler.
    const stop_retired = match("/v1/workspaces/ws1/zombies/z1/stop", .GET);
    if (stop_retired) |r| switch (r) {
        .workspace_zombie_current_run => return error.TestExpectedNotAction,
        else => {},
    };
}

test "custom-method regression: old colon-action forms no longer hit the migrated routes" {
    // The v0.19.0 migration replaced :action with /action. Requests against the
    // legacy URLs must not dispatch to the action handler. They may fall through
    // to a generic resource route (with a garbled id that will 404 at the
    // handler layer — fail-closed) or return null outright; both are acceptable.
    // What's NOT acceptable is the old URL silently routing to the NEW action.
    // Legacy colon-op URLs were historically POST (or PATCH for workspace
    // pause); the regression test invokes each with its original method so
    // that any future method-aware dispatch still exercises the right
    // negative path.
    const approval_old = match("/v1/webhooks/z1:approval", .POST);
    if (approval_old) |r| switch (r) {
        .approval_webhook, .grant_approval_webhook => return error.TestExpectedNotApproval,
        else => {},
    };
    const grant_old = match("/v1/webhooks/z1:grant-approval", .POST);
    if (grant_old) |r| switch (r) {
        .approval_webhook, .grant_approval_webhook => return error.TestExpectedNotApproval,
        else => {},
    };
    const messages_colon_old = match("/v1/workspaces/ws1/zombies/z1:messages", .POST);
    if (messages_colon_old) |r| switch (r) {
        .workspace_zombie_messages, .workspace_zombie_current_run => return error.TestExpectedNotAction,
        else => {},
    };
    const stop_old = match("/v1/workspaces/ws1/zombies/z1:stop", .POST);
    if (stop_old) |r| switch (r) {
        .workspace_zombie_messages, .workspace_zombie_current_run => return error.TestExpectedNotAction,
        else => {},
    };
    // /v1/workspaces/ws1:pause used to be the colon-op form (POST). It now
    // falls through to the generic patch_workspace handler with a garbled id
    // ("ws1:pause"), which the handler 404s at the requireUuidV7Id check.
    // That's the test comment's "fail-closed at the handler layer" path —
    // no assertion needed, just don't crash.
    _ = match("/v1/workspaces/ws1:pause", .POST);
}

test "custom-method reserved segments: approval/grant-approval/svix win over url-secret slot" {
    // /v1/webhooks/{id}/{secret} is the URL-secret form used by agentmail etc.
    // The literals "approval", "grant-approval", and "svix" are reserved: a
    // webhook configured with one of these as its URL secret must never cause
    // the request to route to receive_webhook. These three action routes MUST
    // always win the match.
    const approval = match("/v1/webhooks/z1/approval", .GET) orelse return error.TestExpectedMatch;
    switch (approval) {
        .approval_webhook => {},
        else => return error.TestExpectedEqual,
    }
    const grant = match("/v1/webhooks/z1/grant-approval", .GET) orelse return error.TestExpectedMatch;
    switch (grant) {
        .grant_approval_webhook => {},
        else => return error.TestExpectedEqual,
    }
    // "svix" as the first segment after /webhooks/ is the Svix route prefix,
    // not a zombie_id. /v1/webhooks/svix/{id} means "svix-signed webhook for
    // zombie {id}", not "zombie=svix, secret={id}".
    const svix = match("/v1/webhooks/svix/zid", .GET) orelse return error.TestExpectedMatch;
    switch (svix) {
        .receive_svix_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "custom-method subpath: trailing segments after action are rejected" {
    // /v1/webhooks/{id}/approval/extra must not match approval_webhook.
    try std.testing.expect(match("/v1/webhooks/z1/approval/extra", .GET) == null);
    try std.testing.expect(match("/v1/webhooks/z1/grant-approval/extra", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/z1/messages/extra", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/z1/current-run/extra", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/pause/extra", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/sync/extra", .GET) == null);
}

test "custom-method subpath: empty ids are rejected" {
    try std.testing.expect(match("/v1/webhooks//approval", .GET) == null);
    try std.testing.expect(match("/v1/webhooks//grant-approval", .GET) == null);
    try std.testing.expect(match("/v1/workspaces//zombies/z1/messages", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/zombies//current-run", .GET) == null);
    try std.testing.expect(match("/v1/workspaces//pause", .GET) == null);
    try std.testing.expect(match("/v1/workspaces//sync", .GET) == null);
}

// Every entry in the route manifest must be dispatchable through match().
// Guards the in-repo invariant that route_manifest.zig stays aligned with
// router.zig's match() body.
//
// Scope: (method, path) dispatchability. match() now takes a method as well
// as a path, so a manifest entry with a wrong method (e.g. DELETE where the
// server actually implements PATCH) will fail this test if the matcher
// dispatches on method. For method-agnostic matchers, full (method, path)
// parity against public/openapi.json is still enforced by
// scripts/check_openapi_sync.py.
//
// Placeholders are substituted with a UUIDv7-shaped fixture rather than a
// single char so that today's isSingleSegment checks AND any future
// format-stricter matcher (e.g. stdlib UUID parse on `{workspace_id}`)
// both succeed with the same test.
test "route_manifest: every entry dispatches through match()" {
    const manifest = @import("route_manifest.zig");
    const fixture = "01234567-89ab-7def-8123-456789abcdef"; // 36-char UUIDv7 shape
    var buf: [512]u8 = undefined;

    for (manifest.entries) |entry| {
        var out_len: usize = 0;
        var i: usize = 0;
        while (i < entry.path.len) : (i += 1) {
            if (entry.path[i] == '{') {
                // Skip to '}' and emit the UUID fixture.
                while (i < entry.path.len and entry.path[i] != '}') : (i += 1) {}
                if (out_len + fixture.len > buf.len) return error.ManifestPathTooLongForTestBuffer;
                @memcpy(buf[out_len .. out_len + fixture.len], fixture);
                out_len += fixture.len;
            } else {
                if (out_len >= buf.len) return error.ManifestPathTooLongForTestBuffer;
                buf[out_len] = entry.path[i];
                out_len += 1;
            }
        }
        const concrete = buf[0..out_len];

        if (match(concrete, parseMethod(entry.method)) == null) {
            std.debug.print(
                "route_manifest: {s} {s} (concrete: {s}) did not dispatch\n",
                .{ entry.method, entry.path, concrete },
            );
            return error.ManifestEntryDoesNotDispatch;
        }
    }
}
