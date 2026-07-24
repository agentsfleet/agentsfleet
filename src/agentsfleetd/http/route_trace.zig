//! Bounded admission policy for HTTP request spans.

const std = @import("std");
const router = @import("router.zig");

const TRACE_SAMPLE_SEED: u64 = 0x6d_313339_74726163;
const SAMPLE_DENOMINATOR: u64 = 100;
const EPOCH_MASK: u64 = 0xffff_ffff;
const RUNNER_REJECTION_LIMIT: u32 = 4;
const SERVER_ERROR_LIMIT: u32 = 4;
const SAMPLED_SUCCESS_LIMIT: u32 = 2;

pub const SuppressionReason = enum {
    noisy_route,
    runner_rejection_budget,
    server_error_budget,
    sampled_success_budget,
    sample_miss,
};

pub const Decision = union(enum) {
    emit,
    suppress: SuppressionReason,
};

const RouteTraits = struct {
    runner: bool = false,
    noisy_success: bool = false,
};

var runner_rejections = std.atomic.Value(u64).init(0);
var server_errors = std.atomic.Value(u64).init(0);
var sampled_successes = std.atomic.Value(u64).init(0);

fn classify(route: router.Route) RouteTraits {
    return switch (route) {
        .healthz,
        .readyz,
        .metrics,
        => .{ .noisy_success = true },
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_activity,
        .runner_renew,
        => .{ .runner = true, .noisy_success = true },
        .register_runner,
        .runner_self,
        .runner_credentials_mint,
        .runner_memory_hydrate,
        .runner_memory_capture,
        .runner_bundle,
        => .{ .runner = true },
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
        .workspace_fleet_events_stream,
        .workspace_events,
        .workspace_events_stream,
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
        .fleet_runners_list,
        .fleet_runner_patch,
        .fleet_runner_events,
        .fleet_streams_list,
        => .{},
    };
}

fn admit(window: *std.atomic.Value(u64), now_second: u64, limit: u32) bool {
    const epoch = now_second & EPOCH_MASK;
    while (true) {
        const old = window.load(.acquire);
        const old_epoch = old >> 32;
        const old_count: u32 = @intCast(old & EPOCH_MASK);
        if (old_epoch > epoch) return false;
        const next = if (old_epoch < epoch)
            (epoch << 32) | 1
        else if (old_count < limit)
            (epoch << 32) | (@as(u64, old_count) + 1)
        else
            return false;
        // safe because: the packed epoch and count are one atomic admission state;
        // a failed compare-and-swap retries without exposing a partial reset.
        if (window.cmpxchgWeak(old, next, .acq_rel, .acquire) == null) return true;
    }
}

fn isSampled(span_id: []const u8) bool {
    return std.hash.Wyhash.hash(TRACE_SAMPLE_SEED, span_id) % SAMPLE_DENOMINATOR == 0;
}

/// Derive the exported epoch end from one wall-clock read plus boot-clock
/// elapsed time. A regressed test clock clamps to zero elapsed; addition
/// saturates instead of wrapping an OpenTelemetry timestamp.
pub fn endEpochNanos(wall_start_ns: u64, boot_start_ns: i96, boot_end_ns: i96) u64 {
    if (boot_end_ns <= boot_start_ns) return wall_start_ns;
    const elapsed = std.math.cast(u64, boot_end_ns - boot_start_ns) orelse std.math.maxInt(u64);
    return wall_start_ns +| elapsed;
}

pub fn decide(route: router.Route, status: u16, span_id: []const u8, monotonic_second: u64) Decision {
    const traits = classify(route);
    if (status >= 500) {
        if (admit(&server_errors, monotonic_second, SERVER_ERROR_LIMIT)) return .emit;
        return .{ .suppress = .server_error_budget };
    }
    if (status >= 400 and traits.runner) {
        if (admit(&runner_rejections, monotonic_second, RUNNER_REJECTION_LIMIT)) return .emit;
        return .{ .suppress = .runner_rejection_budget };
    }
    if (status < 400 and traits.noisy_success) return .{ .suppress = .noisy_route };
    if (!isSampled(span_id)) return .{ .suppress = .sample_miss };
    if (admit(&sampled_successes, monotonic_second, SAMPLED_SUCCESS_LIMIT)) return .emit;
    return .{ .suppress = .sampled_success_budget };
}

pub fn resetForTest() void {
    runner_rejections.store(0, .release);
    server_errors.store(0, .release);
    sampled_successes.store(0, .release);
}

test {
    _ = @import("route_trace_test.zig");
}
