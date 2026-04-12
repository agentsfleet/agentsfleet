//! Typed event structs for the telemetry system.
//! Each struct has a `kind` constant and a `properties()` method
//! that returns a fixed-size array of PostHog properties.

const posthog = @import("posthog");

pub const EventKind = enum {
    agent_completed,
    entitlement_rejected,
    profile_activated,
    billing_lifecycle_event,
    server_started,
    worker_started,
    startup_failed,
    api_error,
    workspace_created,
    workspace_github_connected,
    auth_login_completed,
    auth_rejected,
    run_orphan_recovered,
    zombie_triggered,
    zombie_completed,
};

pub const AgentCompleted = struct {
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    actor: []const u8,
    tokens: u64,
    duration_ms: u64,
    exit_status: []const u8,

    pub const kind: EventKind = .agent_completed;

    pub fn properties(self: @This()) [6]posthog.Property {
        return .{
            .{ .key = "run_id", .value = .{ .string = self.run_id } },
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "actor", .value = .{ .string = self.actor } },
            .{ .key = "tokens", .value = .{ .integer = @intCast(self.tokens) } },
            .{ .key = "duration_ms", .value = .{ .integer = @intCast(self.duration_ms) } },
            .{ .key = "exit_status", .value = .{ .string = self.exit_status } },
        };
    }
};

pub const EntitlementRejected = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    boundary: []const u8,
    reason_code: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .entitlement_rejected;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "boundary", .value = .{ .string = self.boundary } },
            .{ .key = "reason_code", .value = .{ .string = self.reason_code } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const ProfileActivated = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    config_version_id: []const u8,
    run_snapshot_version: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .profile_activated;

    pub fn properties(self: @This()) [5]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "agent_id", .value = .{ .string = self.agent_id } },
            .{ .key = "config_version_id", .value = .{ .string = self.config_version_id } },
            .{ .key = "run_snapshot_version", .value = .{ .string = self.run_snapshot_version } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const BillingLifecycleEvent = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    event_type: []const u8,
    reason: []const u8,
    plan_tier: []const u8,
    billing_status: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .billing_lifecycle_event;

    pub fn properties(self: @This()) [6]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "event_type", .value = .{ .string = self.event_type } },
            .{ .key = "reason", .value = .{ .string = self.reason } },
            .{ .key = "plan_tier", .value = .{ .string = self.plan_tier } },
            .{ .key = "billing_status", .value = .{ .string = self.billing_status } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const ServerStarted = struct {
    port: u16,

    pub const kind: EventKind = .server_started;

    pub fn properties(self: @This()) [1]posthog.Property {
        return .{
            .{ .key = "port", .value = .{ .integer = @intCast(self.port) } },
        };
    }
};

pub const WorkerStarted = struct {
    concurrency: u16,

    pub const kind: EventKind = .worker_started;

    pub fn properties(self: @This()) [1]posthog.Property {
        return .{
            .{ .key = "concurrency", .value = .{ .integer = @intCast(self.concurrency) } },
        };
    }
};

pub const StartupFailed = struct {
    command: []const u8,
    phase: []const u8,
    reason: []const u8,
    error_code: []const u8,

    pub const kind: EventKind = .startup_failed;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = "command", .value = .{ .string = self.command } },
            .{ .key = "phase", .value = .{ .string = self.phase } },
            .{ .key = "reason", .value = .{ .string = self.reason } },
            .{ .key = "error_code", .value = .{ .string = self.error_code } },
        };
    }
};

pub const ApiError = struct {
    distinct_id: []const u8,
    error_code: []const u8,
    message: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .api_error;

    pub fn properties(self: @This()) [3]posthog.Property {
        return .{
            .{ .key = "error_code", .value = .{ .string = self.error_code } },
            .{ .key = "message", .value = .{ .string = self.message } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const ApiErrorWithContext = struct {
    distinct_id: []const u8,
    error_code: []const u8,
    message: []const u8,
    workspace_id: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .api_error;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = "error_code", .value = .{ .string = self.error_code } },
            .{ .key = "message", .value = .{ .string = self.message } },
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const WorkspaceCreated = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    repo_url: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .workspace_created;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "tenant_id", .value = .{ .string = self.tenant_id } },
            .{ .key = "repo_url", .value = .{ .string = self.repo_url } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const WorkspaceGithubConnected = struct {
    workspace_id: []const u8,
    installation_id: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .workspace_github_connected;

    pub fn properties(self: @This()) [3]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "installation_id", .value = .{ .string = self.installation_id } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const AuthLoginCompleted = struct {
    distinct_id: []const u8,
    session_id: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .auth_login_completed;

    pub fn properties(self: @This()) [3]posthog.Property {
        return .{
            .{ .key = "session_id", .value = .{ .string = self.session_id } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
            .{ .key = "distinct_id", .value = .{ .string = self.distinct_id } },
        };
    }
};

pub const AuthRejected = struct {
    reason: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .auth_rejected;

    pub fn properties(self: @This()) [2]posthog.Property {
        return .{
            .{ .key = "reason", .value = .{ .string = self.reason } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

pub const RunOrphanRecovered = struct {
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    staleness_ms: u64,

    pub const kind: EventKind = .run_orphan_recovered;

    pub fn properties(self: @This()) [3]posthog.Property {
        return .{
            .{ .key = "run_id", .value = .{ .string = self.run_id } },
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "staleness_ms", .value = .{ .integer = @intCast(self.staleness_ms) } },
        };
    }
};

pub const ZombieTriggered = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    source: []const u8,

    pub const kind: EventKind = .zombie_triggered;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "zombie_id", .value = .{ .string = self.zombie_id } },
            .{ .key = "event_id", .value = .{ .string = self.event_id } },
            .{ .key = "source", .value = .{ .string = self.source } },
        };
    }
};

pub const ZombieCompleted = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    tokens: u64,
    wall_ms: u64,
    exit_status: []const u8,

    pub const kind: EventKind = .zombie_completed;

    pub fn properties(self: @This()) [6]posthog.Property {
        return .{
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "zombie_id", .value = .{ .string = self.zombie_id } },
            .{ .key = "event_id", .value = .{ .string = self.event_id } },
            .{ .key = "tokens", .value = .{ .integer = @intCast(self.tokens) } },
            .{ .key = "wall_ms", .value = .{ .integer = @intCast(self.wall_ms) } },
            .{ .key = "exit_status", .value = .{ .string = self.exit_status } },
        };
    }
};
