// Tests for http/router.zig — split out to keep router.zig under 400 lines.

const std = @import("std");
const router = @import("router.zig");
const matchers = @import("route_matchers.zig");
const Route = router.Route;
const match = router.match;
const matchRunAction = matchers.matchRunAction;
const matchZombieId = matchers.matchZombieId;

test "match resolves workspace billing and harness routes" {
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
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/harness/compile").?) {
            .compile_harness => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match rejects multi-segment workspace suffix routes" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/billing/events") == null);
    try std.testing.expect(match("/v1/workspaces//billing/events") == null);
}

test "match resolves agent profile and scores routes" {
    const agent_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11").?) {
            .get_agent => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/scores").?) {
            .get_agent_scores => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/improvement-report").?) {
            .get_agent_improvement_report => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals").?) {
            .list_agent_proposals => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    const approve = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21:approve").?) {
        .approve_agent_proposal => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, approve.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", approve.proposal_id);
    const veto = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21:veto").?) {
        .veto_agent_proposal => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, veto.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", veto.proposal_id);
    const revert = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/harness/changes/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31:revert").?) {
        .revert_agent_harness_change => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, revert.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31", revert.change_id);
    try std.testing.expect(match("/v1/agents/") == null);
    try std.testing.expect(match("/v1/agents/foo/bar/scores") == null);
    try std.testing.expect(match("/v1/agents/foo/proposals/bar/baz:approve") == null);
    try std.testing.expect(match("/v1/agents/foo/harness/changes/bar/baz:revert") == null);
}

test "match resolves auth and run routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions").?);
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/complete").?) {
            .complete_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1").?) {
            .get_run => |run_id| run_id,
            else => return error.TestExpectedEqual,
        },
    );
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

// ── M16_002 matchRunAction tests ──────────────────────────────────────────────

test "matchRunAction resolves :retry, :replay, :stream with single-segment run_id" {
    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:retry", ":retry").?);
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:replay", ":replay").?);
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:stream", ":stream").?);
    try std.testing.expect(matchRunAction("/v1/runs/foo/bar:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/runs//bar:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/runs/:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/workspaces/ws1:retry", ":retry") == null);
}

test "match uses matchRunAction — run action routes resolve correctly" {
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:retry").?) {
            .retry_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:replay").?) {
            .replay_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:stream").?) {
            .stream_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

// ── M17_001 router tests ──────────────────────────────────────────────────

test "M17: match resolves cancel_run route and extracts run_id" {
    const run_id = "0195b4ba-8d3a-7f13-8abc-cc0000000001";
    const route = match("/v1/runs/0195b4ba-8d3a-7f13-8abc-cc0000000001:cancel") orelse
        return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(run_id, switch (route) {
        .cancel_run => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M17: match cancel_run accepts short run_id" {
    const route = match("/v1/runs/run-42:cancel") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("run-42", switch (route) {
        .cancel_run => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M17: match rejects cancel_run with empty run_id" {
    // M2_002 fixed: `:cancel` with empty inner is now rejected (null).
    try std.testing.expect(match("/v1/runs/:cancel") == null);
}

test "M17: wrong suffix does not match cancel_run" {
    // M2_002: run_ids with ':' are rejected by get_run to prevent false matches.
    try std.testing.expect(match("/v1/runs/run-1:cancelX") == null);
    try std.testing.expect(match("/v1/runs/run-1:CANCEL") == null);
    try std.testing.expect(match("/v1/runs/run-1/cancel") == null);
}

test "M17: cancel route does not interfere with retry and replay" {
    const retry_route = match("/v1/runs/run-1:retry") orelse return error.TestExpectedMatch;
    switch (retry_route) {
        .retry_run => {},
        else => return error.TestExpectedEqual,
    }
    const replay_route = match("/v1/runs/run-1:replay") orelse return error.TestExpectedMatch;
    switch (replay_route) {
        .replay_run => {},
        else => return error.TestExpectedEqual,
    }
}

test "M17: bare run path resolves to get_run not cancel_run" {
    const route = match("/v1/runs/run-99") orelse return error.TestExpectedMatch;
    switch (route) {
        .get_run => |id| try std.testing.expectEqualStrings("run-99", id),
        else => return error.TestExpectedEqual,
    }
}

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

test "M2_001: zombie CRUD routes resolve correctly" {
    try std.testing.expectEqualDeep(Route.list_or_create_zombies, match("/v1/zombies/").?);
    try std.testing.expectEqualDeep(Route.zombie_activity, match("/v1/zombies/activity").?);
    try std.testing.expectEqualDeep(Route.zombie_credentials, match("/v1/zombies/credentials").?);
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(zombie_id, switch (match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?) {
        .delete_zombie => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M2_001: zombie routes reject invalid paths" {
    try std.testing.expect(match("/v1/zombies/a/b") == null);
    try std.testing.expectEqualDeep(Route.list_or_create_zombies, match("/v1/zombies/").?);
    try std.testing.expect(match("/v1/zombies") == null);
}

test "M2_001: matchZombieId excludes sub-paths" {
    try std.testing.expect(matchZombieId("/v1/zombies/activity") == null);
    try std.testing.expect(matchZombieId("/v1/zombies/credentials") == null);
    try std.testing.expectEqualStrings("z1", matchZombieId("/v1/zombies/z1").?);
}
