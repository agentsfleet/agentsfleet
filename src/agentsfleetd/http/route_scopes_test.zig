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

test "connector routes: generic trio gates write/read; callback + events are signature/state-authed (no scope)" {
    const connect_route: @import("router.zig").Route = .{ .connector_connect = .{ .workspace_id = "ws1", .provider = "slack" } };
    const status_route: @import("router.zig").Route = .{ .connector_status = .{ .workspace_id = "ws1", .provider = "github" } };
    try testing.expectEqual(scopes.Scope.connector_write, onlyScope(route_scopes.requiredScopes(connect_route, .POST)).?);
    try testing.expectEqual(scopes.Scope.connector_read, onlyScope(route_scopes.requiredScopes(status_route, .GET)).?);
    // The catalog is a read of the registry + workspace state — connector:read.
    try testing.expectEqual(scopes.Scope.connector_read, onlyScope(route_scopes.requiredScopes(.{ .connector_catalog = "ws1" }, .GET)).?);
    // Bearer-less by design: the callback trusts the signed single-use state,
    // the events ingress trusts the Slack v0 signature — neither carries a scope.
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.{ .connector_callback = "slack" }, .GET).len);
    try testing.expectEqual(@as(usize, 0), route_scopes.requiredScopes(.slack_events, .POST).len);
}

fn router_fleet() @import("router.zig").Route {
    return .{ .patch_workspace_fleet = .{ .workspace_id = "ws1", .fleet_id = "z1" } };
}
