// Tests for route_matchers.zig — kept in a sibling file so the production
// module stays under the file-length budget.

const std = @import("std");
const matchers = @import("route_matchers.zig");

const matchWorkspaceCredential = matchers.matchWorkspaceCredential;
const matchWorkspaceAgentDelete = matchers.matchWorkspaceAgentDelete;
const matchWorkspaceZombieGrant = matchers.matchWorkspaceZombieGrant;
const matchWorkspaceZombie = matchers.matchWorkspaceZombie;
const matchWorkspaceZombieAction = matchers.matchWorkspaceZombieAction;
const matchZombieTelemetry = matchers.matchZombieTelemetry;
const matchWebhookRoute = matchers.matchWebhookRoute;
const matchWorkspaceApprovalResolve = matchers.matchWorkspaceApprovalResolve;
const matchWorkspaceApprovalGate = matchers.matchWorkspaceApprovalGate;
const ApprovalResolveDecision = matchers.ApprovalResolveDecision;

test "matchWorkspaceCredential: workspace_id and credential_name" {
    const r = matchWorkspaceCredential("/v1/workspaces/ws1/credentials/fly").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("fly", r.credential_name);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials/") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces//credentials/fly") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials/llm") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials") == null);
}

test "matchWorkspaceAgentDelete: workspace_id and agent_id" {
    const r = matchWorkspaceAgentDelete("/v1/workspaces/ws1/agent-keys/ag1").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("ag1", r.agent_id);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/ws1/agent-keys/") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces//agent-keys/ag1") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/a/b/agent-keys/ag1") == null);
}

test "matchWorkspaceZombieGrant: ws_id, zombie_id, grant_id" {
    const r = matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/integration-grants/g1").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.zombie_id);
    try std.testing.expectEqualStrings("g1", r.grant_id);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/integration-grants/") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces//zombies/z1/integration-grants/g1") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies//integration-grants/g1") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/x/integration-grants/g1") == null);
}

test "matchWorkspaceZombie: workspace_id and zombie_id extracted" {
    const r = matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/z_1").?;
    try std.testing.expectEqualStrings("ws_1", r.workspace_id);
    try std.testing.expectEqualStrings("z_1", r.zombie_id);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces//zombies/z_1") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/a/b/zombies/z_1") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/z_1/extra") == null);
}

test "matchWorkspaceZombieAction: /steer extracts ws_id + zombie_id" {
    const r = matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/z1/steer", "/steer").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.zombie_id);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies//steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces//zombies/z1/steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/a/b/steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/a/b/zombies/z1/steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/z1/other-action", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/zombies/z1/steer", "/steer") == null);
}

test "matchZombieTelemetry: extracts workspace_id and zombie_id" {
    const ws = "ws_abc";
    const zid = "z_123";
    const r = matchZombieTelemetry("/v1/workspaces/ws_abc/zombies/z_123/telemetry").?;
    try std.testing.expectEqualStrings(ws, r.workspace_id);
    try std.testing.expectEqualStrings(zid, r.zombie_id);
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/extra/zombies/z_123/telemetry") == null);
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/zombies/z_123") == null);
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/zombies//telemetry") == null);
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces//zombies/z_123/telemetry") == null);
}

test "matchWebhookRoute: id only and id+secret" {
    const id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const r1 = matchWebhookRoute("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?;
    try std.testing.expectEqualStrings(id, r1.zombie_id);
    try std.testing.expect(r1.secret == null);
    const r2 = matchWebhookRoute("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/kR7x2mN").?;
    try std.testing.expectEqualStrings(id, r2.zombie_id);
    try std.testing.expectEqualStrings("kR7x2mN", r2.secret.?);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks/") == null);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks") == null);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks/a/b/c") == null);
}

test "matchWorkspaceApprovalResolve: approve and deny" {
    const r = matchWorkspaceApprovalResolve("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999:approve").?;
    try std.testing.expectEqualStrings("ws_1", r.workspace_id);
    try std.testing.expectEqualStrings("01999999-9999-7999-9999-999999999999", r.gate_id);
    try std.testing.expectEqual(ApprovalResolveDecision.approve, r.decision);
    const d = matchWorkspaceApprovalResolve("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999:deny").?;
    try std.testing.expectEqual(ApprovalResolveDecision.deny, d.decision);
}

test "matchWorkspaceApprovalResolve: rejects malformed paths" {
    try std.testing.expect(matchWorkspaceApprovalResolve("/v1/workspaces/ws_1/approvals/abc") == null);
    try std.testing.expect(matchWorkspaceApprovalResolve("/v1/workspaces/ws_1/approvals/abc:other") == null);
    try std.testing.expect(matchWorkspaceApprovalResolve("/v1/workspaces/ws_1/approvals/abc/x:approve") == null);
}

test "matchWorkspaceApprovalGate: bare gate id" {
    const r = matchWorkspaceApprovalGate("/v1/workspaces/ws_1/approvals/01999999-9999-7999-9999-999999999999").?;
    try std.testing.expectEqualStrings("01999999-9999-7999-9999-999999999999", r.gate_id);
    try std.testing.expect(matchWorkspaceApprovalGate("/v1/workspaces/ws_1/approvals/abc:approve") == null);
}
