const std = @import("std");
const posthog = @import("posthog");
const obs_log = std.log.scoped(.posthog);

pub fn distinctIdOrSystem(raw: []const u8) []const u8 {
    if (raw.len == 0) return "system";
    return raw;
}

pub fn trackRunStarted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    mode: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "spec_id", .value = .{ .string = spec_id } },
            .{ .key = "mode", .value = .{ .string = mode } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_started",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunRetried(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    attempt: u32,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "attempt", .value = .{ .integer = @intCast(attempt) } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_retried",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunCompleted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    verdict: []const u8,
    duration_ms: u64,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "verdict", .value = .{ .string = verdict } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_completed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackRunFailed(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    reason: []const u8,
    duration_ms: u64,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "reason", .value = .{ .string = reason } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_failed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackAgentCompleted(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    actor: []const u8,
    tokens: u64,
    duration_ms: u64,
    exit_status: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "actor", .value = .{ .string = actor } },
            .{ .key = "tokens", .value = .{ .integer = @intCast(tokens) } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(duration_ms) } },
            .{ .key = "exit_status", .value = .{ .string = exit_status } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "agent_completed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackEntitlementRejected(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    boundary: []const u8,
    reason_code: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "boundary", .value = .{ .string = boundary } },
            .{ .key = "reason_code", .value = .{ .string = reason_code } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "entitlement_rejected",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackProfileActivated(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    config_version_id: []const u8,
    run_snapshot_version: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "agent_id", .value = .{ .string = agent_id } },
            .{ .key = "config_version_id", .value = .{ .string = config_version_id } },
            .{ .key = "run_snapshot_version", .value = .{ .string = run_snapshot_version } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "profile_activated",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackBillingLifecycleEvent(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    event_type: []const u8,
    reason: []const u8,
    plan_tier: []const u8,
    billing_status: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "event_type", .value = .{ .string = event_type } },
            .{ .key = "reason", .value = .{ .string = reason } },
            .{ .key = "plan_tier", .value = .{ .string = plan_tier } },
            .{ .key = "billing_status", .value = .{ .string = billing_status } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "billing_lifecycle_event",
            .properties = &props,
        }) catch {};
    }
}

// ---------------------------------------------------------------------------
// Startup lifecycle events
// ---------------------------------------------------------------------------

pub fn trackServerStarted(
    client: ?*posthog.PostHogClient,
    port: u16,
    worker_concurrency: u16,
) void {
    if (client) |ph| {
        const props = serverStartedProps(port, worker_concurrency);
        ph.capture(.{
            .distinct_id = "system",
            .event = "server_started",
            .properties = &props,
        }) catch {};
    }
}

pub fn serverStartedProps(port: u16, worker_concurrency: u16) [2]posthog.Property {
    return .{
        .{ .key = "port", .value = .{ .integer = @intCast(port) } },
        .{ .key = "worker_concurrency", .value = .{ .integer = @intCast(worker_concurrency) } },
    };
}

pub fn trackWorkerStarted(
    client: ?*posthog.PostHogClient,
    concurrency: u16,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "concurrency", .value = .{ .integer = @intCast(concurrency) } },
        };
        ph.capture(.{
            .distinct_id = "system",
            .event = "worker_started",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackStartupFailed(
    client: ?*posthog.PostHogClient,
    command: []const u8,
    phase: []const u8,
    reason: []const u8,
    error_code: []const u8,
) void {
    if (client) |ph| {
        const props = startupFailedProps(command, phase, reason, error_code);
        ph.capture(.{
            .distinct_id = "system",
            .event = "startup_failed",
            .properties = &props,
        }) catch {};
    }
}

pub fn startupFailedProps(
    command: []const u8,
    phase: []const u8,
    reason: []const u8,
    error_code: []const u8,
) [4]posthog.Property {
    return .{
        .{ .key = "command", .value = .{ .string = command } },
        .{ .key = "phase", .value = .{ .string = phase } },
        .{ .key = "reason", .value = .{ .string = reason } },
        .{ .key = "error_code", .value = .{ .string = error_code } },
    };
}

// ---------------------------------------------------------------------------
// General API error tracking
// ---------------------------------------------------------------------------

pub fn trackApiError(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    error_code: []const u8,
    message: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "error_code", .value = .{ .string = error_code } },
            .{ .key = "message", .value = .{ .string = message } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinctIdOrSystem(distinct_id),
            .event = "api_error",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackApiErrorWithContext(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    error_code: []const u8,
    message: []const u8,
    workspace_id: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "error_code", .value = .{ .string = error_code } },
            .{ .key = "message", .value = .{ .string = message } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinctIdOrSystem(distinct_id),
            .event = "api_error",
            .properties = &props,
        }) catch {};
    }
}

// ---------------------------------------------------------------------------
// Workspace lifecycle events
// ---------------------------------------------------------------------------

pub fn trackWorkspaceCreated(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    repo_url: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "tenant_id", .value = .{ .string = tenant_id } },
            .{ .key = "repo_url", .value = .{ .string = repo_url } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = distinctIdOrSystem(distinct_id),
            .event = "workspace_created",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackWorkspaceGithubConnected(
    client: ?*posthog.PostHogClient,
    workspace_id: []const u8,
    installation_id: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "installation_id", .value = .{ .string = installation_id } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = "system",
            .event = "workspace_github_connected",
            .properties = &props,
        }) catch {};
    }
}

// ---------------------------------------------------------------------------
// Auth lifecycle events
// ---------------------------------------------------------------------------

pub fn trackAuthLoginCompleted(
    client: ?*posthog.PostHogClient,
    session_id: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "session_id", .value = .{ .string = session_id } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = "system",
            .event = "auth_login_completed",
            .properties = &props,
        }) catch {};
    }
}

pub fn trackAuthRejected(
    client: ?*posthog.PostHogClient,
    reason: []const u8,
    request_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "reason", .value = .{ .string = reason } },
            .{ .key = "request_id", .value = .{ .string = request_id } },
        };
        ph.capture(.{
            .distinct_id = "system",
            .event = "auth_rejected",
            .properties = &props,
        }) catch {};
    }
}

// ---------------------------------------------------------------------------
// Orphan recovery events (M14_001)
// ---------------------------------------------------------------------------

pub fn trackRunOrphanRecovered(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    staleness_ms: u64,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
            .{ .key = "staleness_ms", .value = .{ .integer = @intCast(staleness_ms) } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_orphan_recovered",
            .properties = &props,
        }) catch |err| {
            obs_log.warn("posthog.capture_fail event=run_orphan_recovered run_id={s} err={s}", .{
                run_id, @errorName(err),
            });
        };
    }
}

/// Emitted when an orphan run's workspace has no active agent profile.
/// The run is still transitioned to BLOCKED. Useful for detecting
/// workspaces that crash before profile creation.
pub fn trackRunOrphanNoAgentProfile(
    client: ?*posthog.PostHogClient,
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
) void {
    if (client) |ph| {
        const props = [_]posthog.Property{
            .{ .key = "run_id", .value = .{ .string = run_id } },
            .{ .key = "workspace_id", .value = .{ .string = workspace_id } },
        };
        ph.capture(.{
            .distinct_id = distinct_id,
            .event = "run_orphan_no_agent_profile",
            .properties = &props,
        }) catch |err| {
            obs_log.warn("posthog.capture_fail event=run_orphan_no_agent_profile run_id={s} err={s}", .{
                run_id, @errorName(err),
            });
        };
    }
}

// Tests live in posthog_events_test.zig
comptime {
    _ = @import("posthog_events_test.zig");
}
