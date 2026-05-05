// Tests for http/router.zig — split out to keep router.zig under 400 lines.

const std = @import("std");
const router = @import("router.zig");
const Route = router.Route;
const match = router.match;

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

test "match rejects /v1/agents paths after agent_profiles removal" {
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

// Memory API moved from /v1/memory/{store,recall,list,forget} to
// workspace-scoped /v1/workspaces/{ws}/zombies/{zid}/memories[/{key}].
// The retired top-level paths must 404.
test "match retires /v1/memory/* routes (pre-v2: 404 with no compat shim)" {
    try std.testing.expect(match("/v1/memory/store", .POST) == null);
    try std.testing.expect(match("/v1/memory/recall", .GET) == null);
    try std.testing.expect(match("/v1/memory/list", .GET) == null);
    try std.testing.expect(match("/v1/memory/forget", .POST) == null);
}

test "match resolves /v1/workspaces/{ws}/zombies/{zid}/memories collection" {
    switch (match("/v1/workspaces/ws1/zombies/z1/memories", .GET).?) {
        .workspace_zombie_memories => |r| {
            try std.testing.expectEqualStrings("ws1", r.workspace_id);
            try std.testing.expectEqualStrings("z1", r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
    switch (match("/v1/workspaces/ws1/zombies/z1/memories/incident:42", .DELETE).?) {
        .workspace_zombie_memory => |r| {
            try std.testing.expectEqualStrings("ws1", r.workspace_id);
            try std.testing.expectEqualStrings("z1", r.zombie_id);
            try std.testing.expectEqualStrings("incident:42", r.memory_key);
        },
        else => return error.TestExpectedEqual,
    }
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/z1/memories/", .GET) == null);
}

// /v1/runs/* routes removed (pipeline v1) — get_run, retry_run, replay_run,
// stream_run, cancel_run variants deleted from Route union.
test "match: run paths no longer match any route (post-pipeline-v1 removal)" {
    try std.testing.expect(match("/v1/runs/run_1", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:retry", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:replay", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:stream", .GET) == null);
    try std.testing.expect(match("/v1/runs/run_1:cancel", .GET) == null);
}

// ── admin platform key route tests ────────────────────────────────────────

test "match resolves admin platform key routes" {
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

// ── webhook route tests ───────────────────────────────────────────────────

test "webhook routes resolve and reject correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    // Webhook tests for matchWebhookRoute are in route_matchers.zig.
    // Test via match() integration:
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .receive_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

// ── zombie CRUD route tests ───────────────────────────────────────────────

test "match resolves workspace-scoped zombie collection" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies", .GET).?) {
        .workspace_zombies => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "match: flat /v1/zombies/ is removed (pre-v2.0 bare 404 per RULE EP4)" {
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
    // All status transitions fold into PATCH /zombies/{id} with body
    // {status: "active" | "stopped" | "killed"}. Verb-suffix paths (/kill,
    // /stop, /current-run) all 404.
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/kill", .POST) == null);
}

test "match resolves workspace-scoped credentials collection" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials", .GET).?) {
        .workspace_credentials => |id| id,
        else => return error.TestExpectedEqual,
    });
    try std.testing.expect(match("/v1/zombies/credentials", .GET) == null);
}

test "match: flat /v1/zombies/{id} DELETE path is removed (bare 404 per RULE EP4)" {
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
}

test "match: zombie routes reject invalid paths" {
    try std.testing.expect(match("/v1/zombies/a/b", .GET) == null);
    try std.testing.expect(match("/v1/zombies", .GET) == null);
}

// ── approval gate route tests ─────────────────────────────────────────────

test "match: approval webhook route resolves correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/approval", .GET) orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .approval_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "match: approval route does not interfere with regular webhook" {
    const route = match("/v1/webhooks/z1", .GET) orelse return error.TestExpectedMatch;
    switch (route) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "match: approval route resolves before webhook route" {
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

test "matchWebhookAction excludes reserved literals at slot 1" {
    // /v1/webhooks/{reserved}/approval must NOT dispatch to .approval_webhook
    // with zombie_id={reserved}. Symmetric with matchWebhook's reserved-segment
    // guard. (svix is excluded by matchWebhookAction too, but svix paths route
    // to the svix family via matchSvixWebhook — so they're tested separately.)
    const cases = [_][]const u8{
        "/v1/webhooks/clerk/approval",
        "/v1/webhooks/approval/approval",
        "/v1/webhooks/grant-approval/approval",
        "/v1/webhooks/clerk/grant-approval",
        "/v1/webhooks/approval/grant-approval",
        "/v1/webhooks/grant-approval/grant-approval",
    };
    for (cases) |p| {
        const r = match(p, .POST);
        if (r) |route| switch (route) {
            .approval_webhook, .grant_approval_webhook => return error.TestExpectedNoActionDispatch,
            else => {},
        };
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

test "retired path: bare /v1/workspaces/{id} no longer resolves" {
    // PATCH /v1/workspaces/{id} (workspace pause/unpause) was removed —
    // the bare workspace shape now has no matcher and must return null.
    try std.testing.expect(match("/v1/workspaces/ws_123", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_123", .PATCH) == null);
}

test "old verb-suffix /v1/workspaces/{ws}/pause no longer resolves" {
    // Multi-segment path with no matching action falls through to null.
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

test "retired path: zombie /current-run no longer resolves" {
    // /current-run was the singleton-sub-resource form for stop. After the
    // PATCH FSM unification (status: stopped|active|killed on the zombie
    // resource itself), /current-run is gone — must not match.
    try std.testing.expect(match("/v1/workspaces/ws_abc/zombies/z_xyz/current-run", .DELETE) == null);
    try std.testing.expect(match("/v1/workspaces/ws_abc/zombies/z_xyz/current-run", .GET) == null);
}

test "retired path: /stop no longer resolves as a zombie action" {
    // /stop was the pre-hygiene path-verb form. With both /stop and
    // /current-run retired in favor of PATCH /zombies/{id} {status:"stopped"},
    // this path must return null.
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/z1/stop", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/z1/stop", .POST) == null);
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
        .workspace_zombie_messages => return error.TestExpectedNotAction,
        else => {},
    };
    const stop_old = match("/v1/workspaces/ws1/zombies/z1:stop", .POST);
    if (stop_old) |r| switch (r) {
        .workspace_zombie_messages => return error.TestExpectedNotAction,
        else => {},
    };
    // /v1/workspaces/ws1:pause used to be the colon-op form (POST). With
    // both the colon-op and the bare PATCH /v1/workspaces/{id} handler
    // removed, it must return null outright — no current matcher accepts
    // the shape.
    try std.testing.expect(match("/v1/workspaces/ws1:pause", .POST) == null);
}

test "webhook action routes: approval / grant-approval / svix / github dispatch per action" {
    // 3-segment /v1/webhooks/{id}/{action} dispatches via matchWebhookAction.
    // 2-segment /v1/webhooks/{id} is HMAC-only receive_webhook (the legacy
    // URL-embedded-secret form was removed in M43).
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
    // "svix" as slot-1 is the Svix route prefix, not a zombie_id.
    // /v1/webhooks/svix/{id} means "svix-signed webhook for zombie {id}".
    const svix = match("/v1/webhooks/svix/zid", .GET) orelse return error.TestExpectedMatch;
    switch (svix) {
        .receive_svix_webhook => {},
        else => return error.TestExpectedEqual,
    }
    const github = match("/v1/webhooks/z1/github", .POST) orelse return error.TestExpectedMatch;
    switch (github) {
        .github_webhook => |id| try std.testing.expectEqualStrings("z1", id),
        else => return error.TestExpectedEqual,
    }
}

test "custom-method subpath: trailing segments after action are rejected" {
    // /v1/webhooks/{id}/approval/extra must not match approval_webhook.
    try std.testing.expect(match("/v1/webhooks/z1/approval/extra", .GET) == null);
    try std.testing.expect(match("/v1/webhooks/z1/grant-approval/extra", .GET) == null);
    try std.testing.expect(match("/v1/webhooks/z1/github/extra", .POST) == null);
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

