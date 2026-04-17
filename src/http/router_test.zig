// Tests for http/router.zig — split out to keep router.zig under 400 lines.

const std = @import("std");
const router = @import("router.zig");
const matchers = @import("route_matchers.zig");
const Route = router.Route;
const match = router.match;

test "match resolves workspace billing routes" {
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/billing/events").?) {
            .apply_workspace_billing_event => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/scoring/config").?) {
            .set_workspace_scoring_config => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/billing/scale").?) {
            .upgrade_workspace_to_scale => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match rejects multi-segment workspace suffix routes" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/billing/events") == null);
    try std.testing.expect(match("/v1/workspaces//billing/events") == null);
}

test "match rejects /v1/agents paths after agent_profiles removal (M17_001)" {
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11") == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/scores") == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/improvement-report") == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals") == null);
    try std.testing.expect(match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/harness/changes/c1:revert") == null);
    try std.testing.expect(match("/v1/agents/") == null);
    try std.testing.expect(match("/v1/agents/foo/bar/scores") == null);
}

test "match resolves auth routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions").?);
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/complete").?) {
            .complete_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
}

// M26_001: memory recall/list are matched regardless of HTTP method — method
// enforcement happens in route_table_invoke.zig. This test pins the path match
// so a regression that drops the routes can be caught without spinning up a
// server. Spec §2.1 — GET /v1/memory/recall resolves to .memory_recall.
test "match resolves memory recall/list/store/forget routes" {
    try std.testing.expectEqualDeep(Route.memory_store, match("/v1/memory/store").?);
    try std.testing.expectEqualDeep(Route.memory_recall, match("/v1/memory/recall").?);
    try std.testing.expectEqualDeep(Route.memory_list, match("/v1/memory/list").?);
    try std.testing.expectEqualDeep(Route.memory_forget, match("/v1/memory/forget").?);
    // Query-string suffixes are stripped by the httpz layer before match() runs;
    // path-only match is what we pin here.
    try std.testing.expect(match("/v1/memory/recall/") == null); // trailing slash is NOT accepted
    try std.testing.expect(match("/v1/memory/unknown") == null);
}

// M10_001: /v1/runs/* routes removed — get_run, retry_run, replay_run,
// stream_run, cancel_run variants deleted from Route union.
test "M10_001: run paths no longer match any route" {
    try std.testing.expect(match("/v1/runs/run_1") == null);
    try std.testing.expect(match("/v1/runs/run_1:retry") == null);
    try std.testing.expect(match("/v1/runs/run_1:replay") == null);
    try std.testing.expect(match("/v1/runs/run_1:stream") == null);
    try std.testing.expect(match("/v1/runs/run_1:cancel") == null);
}

// ── M16_004 route tests ───────────────────────────────────────────────────────

test "match resolves admin platform key routes (M16_004)" {
    try std.testing.expectEqualDeep(Route.admin_platform_keys, match("/v1/admin/platform-keys").?);
    try std.testing.expectEqualStrings(
        "anthropic",
        switch (match("/v1/admin/platform-keys/anthropic").?) {
            .delete_admin_platform_key => |provider| provider,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/admin/platform-keys/a/b") == null);
    try std.testing.expect(match("/v1/admin/platform-keys/") == null);
}

// ── M18_003 agent relay route tests ──────────────────────────────────────────

test "match resolves spec template route (M18_003)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/template").?) {
            .spec_template => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match resolves spec preview route (M18_003)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/preview").?) {
            .spec_preview => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match rejects multi-segment workspace in spec routes (M18_003)" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/spec/template") == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/spec/preview") == null);
}

test "match resolves workspace LLM credential route (M16_004)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm").?) {
            .workspace_llm_credential => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/credentials/llm") == null);
}

// M10_001: matchRunAction tests, M16_002 run action tests, M17_001 cancel tests
// all removed — Route variants (retry_run, replay_run, stream_run, cancel_run,
// get_run) deleted. Run path null-match covered in "run paths no longer match" above.

// ── M1_001 webhook route tests ────────────────────────────────────────────

test "M1_001: webhook routes resolve and reject correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    // Webhook tests for matchWebhookRoute are in route_matchers.zig.
    // Test via match() integration:
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .receive_webhook => |r| r.zombie_id,
        else => return error.TestExpectedEqual,
    });
}

// ── M2_001 zombie CRUD route tests ────────────────────────────────────

test "M24_001: workspace-scoped zombie collection resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies").?) {
        .workspace_zombies => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M24_001: flat /v1/zombies/ is removed (pre-v2.0 bare 404 per RULE EP4)" {
    try std.testing.expect(match("/v1/zombies/") == null);
}

test "M24_001: workspace-scoped zombie DELETE resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const r = match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?;
    switch (r) {
        .delete_workspace_zombie => |route| {
            try std.testing.expectEqualStrings(ws_id, route.workspace_id);
            try std.testing.expectEqualStrings(zid, route.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
}

test "M24_001: workspace-scoped activity route resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/activity").?) {
        .workspace_zombie_activity => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
    // flat /v1/zombies/activity removed
    try std.testing.expect(match("/v1/zombies/activity") == null);
}

test "M24_001: workspace-scoped credentials route resolves" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials").?) {
        .workspace_credentials => |id| id,
        else => return error.TestExpectedEqual,
    });
    try std.testing.expect(match("/v1/zombies/credentials") == null);
}

test "M24_001: /credentials/llm still distinct from /credentials" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(ws_id, switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm").?) {
        .workspace_llm_credential => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M24_001: flat /v1/zombies/{id} DELETE path is removed (bare 404 per RULE EP4)" {
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11") == null);
}

test "M2_001: zombie routes reject invalid paths" {
    try std.testing.expect(match("/v1/zombies/a/b") == null);
    try std.testing.expect(match("/v1/zombies") == null);
}

// ── M4_001 approval gate route tests ────────────────────────────────────

test "M4_001: approval webhook route resolves correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11:approval") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .approval_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M4_001: approval route does not interfere with regular webhook" {
    const route = match("/v1/webhooks/z1") orelse return error.TestExpectedMatch;
    switch (route) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "M4_001: approval route resolves before webhook route" {
    // :approval suffix is matched before the generic webhook route
    const route = match("/v1/webhooks/z1:approval") orelse return error.TestExpectedMatch;
    switch (route) {
        .approval_webhook => {},
        else => return error.TestExpectedEqual,
    }
    // Without :approval suffix, goes to receive_webhook
    const route2 = match("/v1/webhooks/z1") orelse return error.TestExpectedMatch;
    switch (route2) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}
