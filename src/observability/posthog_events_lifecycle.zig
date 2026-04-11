//! Workspace and auth lifecycle PostHog events — extracted from
//! posthog_events.zig for the 350-line limit (M10_002).

const posthog = @import("posthog");
const posthog_events = @import("posthog_events.zig");
const distinctIdOrSystem = posthog_events.distinctIdOrSystem;

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
