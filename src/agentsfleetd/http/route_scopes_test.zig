//! Tests for the declarative route → required-scope table. FLL-exempt.

const std = @import("std");
const testing = std.testing;
const route_scopes = @import("route_scopes.zig");
const scopes = @import("../auth/scopes.zig");

fn onlyScope(required: []const scopes.Scope) ?scopes.Scope {
    if (required.len != 1) return null;
    return required[0];
}

test "tenant fleet routes map method → capability scope (GET read, write/delete escalate)" {
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(route_scopes.requiredScopes(.{ .workspace_fleets = "ws1" }, .GET)).?);
    try testing.expectEqual(scopes.Scope.fleet_write, onlyScope(route_scopes.requiredScopes(.{ .workspace_fleets = "ws1" }, .POST)).?);

    const fleet = router_fleet();
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(route_scopes.requiredScopes(fleet, .GET)).?);
    try testing.expectEqual(scopes.Scope.fleet_write, onlyScope(route_scopes.requiredScopes(fleet, .PATCH)).?);
    try testing.expectEqual(scopes.Scope.fleet_admin, onlyScope(route_scopes.requiredScopes(fleet, .DELETE)).?);

    const memory: @import("router.zig").Route = .{ .workspace_fleet_memory_item = .{
        .workspace_id = "ws1",
        .fleet_id = "z1",
        .memory_key = "lesson",
    } };
    try testing.expectEqual(scopes.Scope.fleet_write, onlyScope(route_scopes.requiredScopes(memory, .DELETE)).?);
}

test "schedule routes use schedule read/write scopes" {
    const collection: @import("router.zig").Route = .{ .workspace_fleet_schedules = .{ .workspace_id = "ws1", .fleet_id = "z1" } };
    const item: @import("router.zig").Route = .{ .workspace_fleet_schedule = .{ .workspace_id = "ws1", .fleet_id = "z1", .schedule_id = "s1" } };
    const sync: @import("router.zig").Route = .{ .workspace_fleet_schedule_sync = .{ .workspace_id = "ws1", .fleet_id = "z1", .schedule_id = "s1" } };
    try testing.expectEqual(scopes.Scope.schedule_read, onlyScope(route_scopes.requiredScopes(collection, .GET)).?);
    try testing.expectEqual(scopes.Scope.schedule_write, onlyScope(route_scopes.requiredScopes(collection, .POST)).?);
    try testing.expectEqual(scopes.Scope.schedule_read, onlyScope(route_scopes.requiredScopes(item, .GET)).?);
    try testing.expectEqual(scopes.Scope.schedule_write, onlyScope(route_scopes.requiredScopes(item, .PATCH)).?);
    try testing.expectEqual(scopes.Scope.schedule_write, onlyScope(route_scopes.requiredScopes(item, .DELETE)).?);
    try testing.expectEqual(scopes.Scope.schedule_write, onlyScope(route_scopes.requiredScopes(sync, .POST)).?);
}

test "workspace event reads (list + both SSE streams) require fleet:read" {
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(route_scopes.requiredScopes(.{ .workspace_events = "ws1" }, .GET)).?);
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(route_scopes.requiredScopes(.{ .workspace_events_stream = "ws1" }, .GET)).?);
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(route_scopes.requiredScopes(.{ .workspace_fleet_events_stream = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, .GET)).?);
}

test "platform routes map to platform-plane scopes; runner enroll is its own verb" {
    try testing.expectEqual(scopes.Scope.runner_enroll, onlyScope(route_scopes.requiredScopes(.register_runner, .POST)).?);
    try testing.expectEqual(scopes.Scope.runner_read, onlyScope(route_scopes.requiredScopes(.fleet_runners_list, .GET)).?);
    try testing.expectEqual(scopes.Scope.runner_write, onlyScope(route_scopes.requiredScopes(.{ .fleet_runner_patch = "r1" }, .PATCH)).?);
    try testing.expectEqual(scopes.Scope.stream_read, onlyScope(route_scopes.requiredScopes(.fleet_streams_list, .GET)).?);
    try testing.expectEqual(scopes.Scope.platform_key_read, onlyScope(route_scopes.requiredScopes(.admin_platform_keys, .GET)).?);
    try testing.expectEqual(scopes.Scope.platform_key_admin, onlyScope(route_scopes.requiredScopes(.admin_platform_keys, .PUT)).?);
    try testing.expectEqual(scopes.Scope.model_read, onlyScope(route_scopes.requiredScopes(.admin_models, .GET)).?);
    try testing.expectEqual(scopes.Scope.model_admin, onlyScope(route_scopes.requiredScopes(.admin_models, .POST)).?);
}

test "tenant api-key routes escalate read→write→admin by method" {
    try testing.expectEqual(scopes.Scope.apikey_read, onlyScope(route_scopes.requiredScopes(.tenant_api_keys, .GET)).?);
    try testing.expectEqual(scopes.Scope.apikey_write, onlyScope(route_scopes.requiredScopes(.tenant_api_keys, .POST)).?);
    try testing.expectEqual(scopes.Scope.apikey_write, onlyScope(route_scopes.requiredScopes(.{ .tenant_api_key_by_id = "k1" }, .PATCH)).?);
    try testing.expectEqual(scopes.Scope.apikey_admin, onlyScope(route_scopes.requiredScopes(.{ .tenant_api_key_by_id = "k1" }, .DELETE)).?);
}

test "runner self-plane routes all require runner:self" {
    try testing.expectEqual(scopes.Scope.runner_self, onlyScope(route_scopes.requiredScopes(.runner_self, .GET)).?);
    try testing.expectEqual(scopes.Scope.runner_self, onlyScope(route_scopes.requiredScopes(.runner_heartbeat, .POST)).?);
    try testing.expectEqual(scopes.Scope.runner_self, onlyScope(route_scopes.requiredScopes(.runner_lease, .POST)).?);
}

test "no-auth and self-service routes carry no capability scope (authenticated-only/none)" {
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.healthz, .GET).len);
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.{ .receive_webhook = "z1" }, .POST).len);
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.qstash_schedule_ingress, .POST).len);
    // Self-session management authenticates but needs no capability scope.
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.delete_all_auth_sessions, .DELETE).len);
}

test "connector routes: generic trio gates write/read; callback methods split relay from completion" {
    const connect_route: @import("router.zig").Route = .{ .connector_connect = .{ .workspace_id = "ws1", .provider = "slack" } };
    const status_route: @import("router.zig").Route = .{ .connector_status = .{ .workspace_id = "ws1", .provider = "github" } };
    try testing.expectEqual(scopes.Scope.connector_write, onlyScope(route_scopes.requiredScopes(connect_route, .POST)).?);
    try testing.expectEqual(scopes.Scope.connector_read, onlyScope(route_scopes.requiredScopes(status_route, .GET)).?);
    // The catalog is a read of the registry + workspace state — connector:read.
    try testing.expectEqual(scopes.Scope.connector_read, onlyScope(route_scopes.requiredScopes(.{ .connector_catalog = "ws1" }, .GET)).?);
    // The provider-facing GET relay is Bearer-less. The dashboard POST carries
    // connector:write and the same-identity state check. Slack events trust
    // their own v0 signature.
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.{ .connector_callback = "slack" }, .GET).len);
    try testing.expectEqual(scopes.Scope.connector_write, onlyScope(route_scopes.requiredScopes(.{ .connector_complete = "slack" }, .POST)).?);
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.slack_events, .POST).len);
}

test "test_no_machine_approval_callers" {
    const resolve: @import("router.zig").Route = .{ .workspace_approval_resolve = .{
        .workspace_id = "ws1",
        .gate_id = "gate1",
        .decision = .approve,
    } };
    const required = route_scopes.requiredScopes(resolve, .POST);
    try testing.expectEqual(scopes.Scope.approval_resolve, onlyScope(required).?);

    // A signup owner reaches it. There is no longer a separate
    // machine grant to contrast against: an `agt_t` key resolves whatever the
    // provider holds for the person who minted it, so what reaches this gate is
    // decided per person at the provider, not per credential class here.
    const owner = scopes.parseClaim(scopes.SIGNUP_OWNER_CLAIM);
    try testing.expect(scopes.satisfiesAny(owner, required));

    // The runner credential is self-plane only and reaches no tenant route. It
    // is the one class still resolved in code, having no identity to ask about.
    try testing.expect(!scopes.satisfiesAny(scopes.RUNNER_SCOPES, required));

    // Viewing the inbox is the lower rung of the same ladder, reached by the
    // closure rather than granted separately.
    const inbox = route_scopes.requiredScopes(.{ .workspace_approvals = "ws1" }, .GET);
    try testing.expectEqual(scopes.Scope.approval_read, onlyScope(inbox).?);
    try testing.expect(scopes.satisfiesAny(owner, inbox));
    try testing.expect(!scopes.satisfiesAny(scopes.RUNNER_SCOPES, inbox));
}

fn router_fleet() @import("router.zig").Route {
    return .{ .patch_workspace_fleet = .{ .workspace_id = "ws1", .fleet_id = "z1" } };
}

test "test_fleet_write_can_blank_gate_policy" {
    // Dimension 3.6 — a KNOWN bypass, asserted so that closing it is
    // regression-tested rather than assumed. This test is expected to CHANGE
    // when the fleet:message split lands; it is not expected to be deleted.
    //
    // Waking a fleet and reconfiguring one are ONE scope today. `gates` lives in
    // `config_json`, and PATCH accepts `config_json` — so any credential able to
    // send the wake message is also able to rewrite the repairer's gate policy to
    // empty, after which `approval_gate` falls through to `.auto_approve` and no
    // human is ever asked. That bypass needs no approval of its own, so §1's
    // removal of `approval_resolve` does not close it, and no narrowing of WHICH
    // tenant scopes are granted can: the capability the investigator would need
    // and the capability that breaks the design are the same capability.
    //
    // This is why machine wakes stay un-privileged: any credential that can
    // wake a fleet can also blank its gate policy through the same scope.
    // Splitting `fleet:message` out of `fleet:write` is the fix.
    const wake = route_scopes.requiredScopes(.{ .workspace_fleet_messages = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, .POST);
    const reconfigure = route_scopes.requiredScopes(router_fleet(), .PATCH);

    try testing.expectEqual(scopes.Scope.fleet_write, onlyScope(wake).?);
    try testing.expectEqual(scopes.Scope.fleet_write, onlyScope(reconfigure).?);

    // The thread READ on the same route is a read: a fleet:read holder can see
    // the conversation without holding the wake-the-fleet capability.
    const thread_read = route_scopes.requiredScopes(.{ .workspace_fleet_messages = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, .GET);
    try testing.expectEqual(scopes.Scope.fleet_read, onlyScope(thread_read).?);

    // The bypass, stated as the identity it actually is: one scope opens both
    // doors, so holding the wake implies holding the rewrite.
    try testing.expectEqual(onlyScope(wake).?, onlyScope(reconfigure).?);

    // And an ordinary tenant person holds it, so any credential resolving to
    // that person — their terminal or a key they minted — opens both doors.
    // That is why this gap survives §1.
    const owner = scopes.parseClaim(scopes.SIGNUP_OWNER_CLAIM);
    try testing.expect(scopes.satisfiesAny(owner, wake));
    try testing.expect(scopes.satisfiesAny(owner, reconfigure));
}
