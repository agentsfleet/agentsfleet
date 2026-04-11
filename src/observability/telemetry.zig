const std = @import("std");
const builtin = @import("builtin");
const posthog = @import("posthog");

const obs_log = std.log.scoped(.telemetry);

// ── Utility ─────────────────────────────────────────────────────────

pub fn distinctIdOrSystem(raw: []const u8) []const u8 {
    if (raw.len == 0) return "system";
    return raw;
}

// ── Event types ─────────────────────────────────────────────────────

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
    run_orphan_no_agent_profile,
};

pub const RecordedEvent = struct {
    kind: EventKind,
    distinct_id: []const u8,
    workspace_id: []const u8,
};

// ── Event structs (re-exported from telemetry_events.zig) ───────────

const events = @import("telemetry_events.zig");
pub const AgentCompleted = events.AgentCompleted;
pub const EntitlementRejected = events.EntitlementRejected;
pub const ProfileActivated = events.ProfileActivated;
pub const BillingLifecycleEvent = events.BillingLifecycleEvent;
pub const ServerStarted = events.ServerStarted;
pub const WorkerStarted = events.WorkerStarted;
pub const StartupFailed = events.StartupFailed;
pub const ApiError = events.ApiError;
pub const ApiErrorWithContext = events.ApiErrorWithContext;
pub const WorkspaceCreated = events.WorkspaceCreated;
pub const WorkspaceGithubConnected = events.WorkspaceGithubConnected;
pub const AuthLoginCompleted = events.AuthLoginCompleted;
pub const AuthRejected = events.AuthRejected;
pub const RunOrphanRecovered = events.RunOrphanRecovered;
pub const RunOrphanNoAgentProfile = events.RunOrphanNoAgentProfile;

// ── Backends ────────────────────────────────────────────────────────

pub const ProdBackend = struct {
    client: ?*posthog.PostHogClient,

    pub fn capture(self: *ProdBackend, comptime E: type, event: E) void {
        const ph = self.client orelse return;
        const props = event.properties();
        const did = if (@hasField(E, "distinct_id"))
            distinctIdOrSystem(event.distinct_id)
        else
            "system";
        ph.capture(.{
            .distinct_id = did,
            .event = @tagName(E.kind),
            .properties = &props,
        }) catch |err| {
            obs_log.warn("posthog.capture_fail event={s} err={s}", .{ @tagName(E.kind), @errorName(err) });
        };
    }
};

pub const TestBackend = struct {
    var ring: [64]?RecordedEvent = [_]?RecordedEvent{null} ** 64;
    var count: usize = 0;

    pub fn capture(_: *TestBackend, comptime E: type, event: E) void {
        ring[count % 64] = .{
            .kind = E.kind,
            .distinct_id = if (@hasField(E, "distinct_id")) event.distinct_id else "system",
            .workspace_id = if (@hasField(E, "workspace_id")) event.workspace_id else "",
        };
        count += 1;
    }

    pub fn reset() void {
        ring = [_]?RecordedEvent{null} ** 64;
        count = 0;
    }

    pub fn lastEvent() ?RecordedEvent {
        if (count == 0) return null;
        return ring[(count - 1) % 64];
    }

    pub fn assertLastEventIs(expected: EventKind) !void {
        const last = lastEvent() orelse return error.NoEventsRecorded;
        try std.testing.expectEqual(expected, last.kind);
    }

    pub fn assertCount(expected: usize) !void {
        try std.testing.expectEqual(expected, count);
    }
};

// ── Telemetry (comptime-selected) ───────────────────────────────────

pub const Backend = if (builtin.is_test) TestBackend else ProdBackend;

pub const Telemetry = struct {
    backend: Backend,

    pub fn capture(self: *Telemetry, comptime E: type, event: E) void {
        self.backend.capture(E, event);
    }

    /// Production init — wraps a PostHog client (nullable for graceful degradation).
    pub fn initProd(client: ?*posthog.PostHogClient) Telemetry {
        return .{ .backend = .{ .client = client } };
    }

    /// Test init — uses TestBackend (no PostHog dependency).
    pub fn initTest() Telemetry {
        TestBackend.reset();
        return .{ .backend = .{} };
    }
};

comptime {
    _ = @import("telemetry_test.zig");
}
