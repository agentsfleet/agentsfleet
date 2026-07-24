//! Typed event structs for the telemetry system.
//! Each struct has a `kind` constant and a `properties()` method
//! that returns a fixed-size array of PostHog properties.

const std = @import("std");
const posthog = @import("posthog");

const S_REASON = "reason";
const S_ERROR_CODE = "error_code";
const S_REQUEST_ID = "request_id";
const S_WORKSPACE_ID = "workspace_id";
const S_MESSAGE = "message";
const S_TENANT_ID = "tenant_id";
const S_FLEET_ID = "fleet_id";
const S_EVENT_ID = "event_id";
const S_INSERT_ID = "$insert_id";
const HASH_SEPARATOR = [_]u8{0};

pub const EventKind = enum {
    entitlement_rejected,
    server_started,
    worker_started,
    startup_failed,
    api_error,
    workspace_created,
    auth_login_completed,
    auth_rejected,
    fleet_triggered,
    fleet_completed,
    signup_bootstrapped,
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
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = "boundary", .value = .{ .string = self.boundary } },
            .{ .key = "reason_code", .value = .{ .string = self.reason_code } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
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
            .{ .key = S_REASON, .value = .{ .string = self.reason } },
            .{ .key = S_ERROR_CODE, .value = .{ .string = self.error_code } },
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
            .{ .key = S_ERROR_CODE, .value = .{ .string = self.error_code } },
            .{ .key = S_MESSAGE, .value = .{ .string = self.message } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
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
            .{ .key = S_ERROR_CODE, .value = .{ .string = self.error_code } },
            .{ .key = S_MESSAGE, .value = .{ .string = self.message } },
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
        };
    }
};

pub const WorkspaceCreated = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    tenant_id: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .workspace_created;

    pub fn properties(self: @This()) [3]posthog.Property {
        return .{
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = S_TENANT_ID, .value = .{ .string = self.tenant_id } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
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
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
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
            .{ .key = S_REASON, .value = .{ .string = self.reason } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
        };
    }
};

pub const FleetTriggered = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    source: []const u8,

    pub const kind: EventKind = .fleet_triggered;

    pub fn properties(self: @This()) [4]posthog.Property {
        return .{
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = S_FLEET_ID, .value = .{ .string = self.fleet_id } },
            .{ .key = S_EVENT_ID, .value = .{ .string = self.event_id } },
            .{ .key = "source", .value = .{ .string = self.source } },
        };
    }
};

pub const FleetCompleted = struct {
    distinct_id: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    tokens: u64,
    wall_ms: u64,
    exit_status: []const u8,
    /// ms to first token. 0 if the runner did not report.
    time_to_first_token_ms: u64 = 0,
    insert_id: [64]u8,

    pub const kind: EventKind = .fleet_completed;

    pub const SettledFacts = struct {
        distinct_id: []const u8,
        workspace_id: []const u8,
        fleet_id: []const u8,
        event_id: []const u8,
        tokens: u64,
        wall_ms: u64,
        exit_status: []const u8,
        time_to_first_token_ms: u64,
    };

    /// Build the event and its deterministic Secure Hash Algorithm 256-bit
    /// (SHA-256) insertion Identifier (ID) from settled fleet and event IDs.
    pub fn init(facts: SettledFacts) @This() {
        var hasher = std.crypto.hash.sha2.Sha256.init(.{});
        hasher.update(facts.fleet_id);
        hasher.update(&HASH_SEPARATOR);
        hasher.update(facts.event_id);
        var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
        hasher.final(&digest);
        return .{
            .distinct_id = facts.distinct_id,
            .workspace_id = facts.workspace_id,
            .fleet_id = facts.fleet_id,
            .event_id = facts.event_id,
            .tokens = facts.tokens,
            .wall_ms = facts.wall_ms,
            .exit_status = facts.exit_status,
            .time_to_first_token_ms = facts.time_to_first_token_ms,
            .insert_id = std.fmt.bytesToHex(digest, .lower),
        };
    }

    pub fn properties(self: *const @This()) [8]posthog.Property {
        return .{
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = S_FLEET_ID, .value = .{ .string = self.fleet_id } },
            .{ .key = S_EVENT_ID, .value = .{ .string = self.event_id } },
            .{ .key = "tokens", .value = .{ .integer = saturatingI64(self.tokens) } },
            .{ .key = "wall_ms", .value = .{ .integer = saturatingI64(self.wall_ms) } },
            .{ .key = "exit_status", .value = .{ .string = self.exit_status } },
            .{ .key = "time_to_first_token_ms", .value = .{ .integer = saturatingI64(self.time_to_first_token_ms) } },
            .{ .key = S_INSERT_ID, .value = .{ .string = &self.insert_id } },
        };
    }
};

fn saturatingI64(value: u64) i64 {
    return std.math.cast(i64, value) orelse std.math.maxInt(i64);
}

/// Clerk signup bootstrapped a personal account (or confirmed replay of an
/// existing one). distinct_id is the OIDC subject so PostHog funnels stitch
/// across replayed webhooks. email_domain is included (not the full email)
/// for cohort analysis without storing PII in the event payload.
pub const SignupBootstrapped = struct {
    distinct_id: []const u8,
    tenant_id: []const u8,
    workspace_id: []const u8,
    workspace_name: []const u8,
    email_domain: []const u8,
    created: bool,
    request_id: []const u8,

    pub const kind: EventKind = .signup_bootstrapped;

    pub fn properties(self: @This()) [6]posthog.Property {
        return .{
            .{ .key = S_TENANT_ID, .value = .{ .string = self.tenant_id } },
            .{ .key = S_WORKSPACE_ID, .value = .{ .string = self.workspace_id } },
            .{ .key = "workspace_name", .value = .{ .string = self.workspace_name } },
            .{ .key = "email_domain", .value = .{ .string = self.email_domain } },
            .{ .key = "created", .value = .{ .boolean = self.created } },
            .{ .key = S_REQUEST_ID, .value = .{ .string = self.request_id } },
        };
    }
};
