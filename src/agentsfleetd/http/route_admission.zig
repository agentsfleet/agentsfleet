const std = @import("std");
const router = @import("router.zig");

/// Shed behaviour at dispatch. Exhaustive on purpose: a new Route variant
/// fails compilation until its class is chosen.
pub const RouteClass = enum { ops, stream, api };

/// Total over Route. ops = never shed; stream uses the dedicated
/// Server-Sent Events limit; api uses the in-flight request ceiling.
pub fn classFor(route: router.Route) RouteClass {
    return switch (route) {
        .healthz, .readyz, .metrics => .ops,
        .workspace_fleet_events_stream, .workspace_events_stream => .stream,
        .model_library,
        .create_auth_session,
        .poll_auth_session,
        .approve_auth_session,
        .verify_auth_session,
        .delete_auth_session,
        .delete_all_auth_sessions,
        .create_workspace,
        .get_tenant_billing,
        .get_tenant_billing_charges,
        .get_tenant_metering_periods,
        .list_tenant_workspaces,
        .tenant_provider,
        .tenant_model_entries,
        .tenant_model_entry_by_id,
        .fleet_bundles,
        .admin_fleet_library,
        .admin_fleet_library_by_id,
        .workspace_fleet_library,
        .receive_webhook,
        .receive_svix_webhook,
        .auth_identity_event_clerk,
        .approval_webhook,
        .grant_approval_webhook,
        .github_webhook,
        .app_ingress,
        .qstash_schedule_ingress,
        .admin_platform_keys,
        .delete_admin_platform_key,
        .admin_models,
        .admin_model_by_id,
        .workspace_fleets,
        .patch_workspace_fleet,
        .workspace_fleet_schedules,
        .workspace_fleet_schedule,
        .workspace_fleet_schedule_sync,
        .workspace_secrets,
        .workspace_secret,
        .workspace_fleet_messages,
        .workspace_fleet_events,
        .workspace_events,
        .workspace_onboarding,
        .workspace_preferences,
        .workspace_preference,
        .workspace_approvals,
        .workspace_approval_detail,
        .workspace_approval_resolve,
        .workspace_fleet_memories,
        .workspace_fleet_memory_item,
        .request_integration_grant,
        .list_integration_grants,
        .revoke_integration_grant,
        .connector_connect,
        .connector_status,
        .connector_catalog,
        .connector_callback,
        .slack_events,
        .fleet_keys,
        .delete_fleet_key,
        .tenant_api_keys,
        .tenant_api_key_by_id,
        .register_runner,
        .fleet_runners_list,
        .fleet_runner_patch,
        .fleet_runner_events,
        .fleet_streams_list,
        .runner_self,
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_credentials_mint,
        .runner_activity,
        .runner_renew,
        .runner_memory_hydrate,
        .runner_memory_capture,
        .runner_bundle,
        => .api,
    };
}

test "classFor: ops probes never shed, the SSE tail is stream, the rest api" {
    try std.testing.expectEqual(RouteClass.ops, classFor(.healthz));
    try std.testing.expectEqual(RouteClass.ops, classFor(.readyz));
    try std.testing.expectEqual(RouteClass.ops, classFor(.metrics));
    try std.testing.expectEqual(RouteClass.stream, classFor(.{ .workspace_fleet_events_stream = .{ .workspace_id = "ws1", .fleet_id = "z1" } }));
    try std.testing.expectEqual(RouteClass.stream, classFor(.{ .workspace_events_stream = "ws1" }));
    try std.testing.expectEqual(RouteClass.api, classFor(.model_library));
    try std.testing.expectEqual(RouteClass.api, classFor(.create_workspace));
    try std.testing.expectEqual(RouteClass.api, classFor(.runner_lease));
    try std.testing.expectEqual(RouteClass.api, classFor(.{ .receive_webhook = "z1" }));
}
