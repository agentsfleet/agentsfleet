const std = @import("std");
const builtin = @import("builtin");
const posthog = @import("posthog");
const logging = @import("log");

const log = logging.scoped(.telemetry);

// ── Utility ─────────────────────────────────────────────────────────

const S_SYSTEM = "system";
const DISTINCT_ID_FIELD = "distinct_id";

pub fn distinctIdOrSystem(raw: []const u8) []const u8 {
    if (raw.len == 0) return S_SYSTEM;
    return raw;
}

// ── Event types + structs (from telemetry_events.zig) ───────────────

const events = @import("telemetry_events.zig");

pub const EventKind = events.EventKind;

pub const RecordedEvent = struct {
    const Self = @This();

    kind: EventKind,
    distinct_id_buf: [64]u8 = .{0} ** 64,
    distinct_id_len: u8 = 0,
    workspace_id_buf: [64]u8 = .{0} ** 64,
    workspace_id_len: u8 = 0,

    pub fn distinctId(self: *const Self) []const u8 {
        return self.distinct_id_buf[0..self.distinct_id_len];
    }

    pub fn workspaceId(self: *const Self) []const u8 {
        return self.workspace_id_buf[0..self.workspace_id_len];
    }

    pub fn initFromSlices(kind: EventKind, did: []const u8, wid: []const u8) RecordedEvent {
        var r = RecordedEvent{ .kind = kind };
        const did_len = @min(did.len, 64);
        const wid_len = @min(wid.len, 64);
        @memcpy(r.distinct_id_buf[0..did_len], did[0..did_len]);
        r.distinct_id_len = @intCast(did_len);
        @memcpy(r.workspace_id_buf[0..wid_len], wid[0..wid_len]);
        r.workspace_id_len = @intCast(wid_len);
        return r;
    }
};
pub const EntitlementRejected = events.EntitlementRejected;
pub const ServerStarted = events.ServerStarted;
pub const WorkerStarted = events.WorkerStarted;
pub const StartupFailed = events.StartupFailed;
pub const ApiError = events.ApiError;
pub const ApiErrorWithContext = events.ApiErrorWithContext;
pub const WorkspaceCreated = events.WorkspaceCreated;
pub const AuthLoginCompleted = events.AuthLoginCompleted;
pub const AuthRejected = events.AuthRejected;
pub const FleetTriggered = events.FleetTriggered;
pub const FleetCompleted = events.FleetCompleted;
pub const SignupBootstrapped = events.SignupBootstrapped;

// ── Backends ────────────────────────────────────────────────────────

pub const ProdBackend = struct {
    const Self = @This();

    client: ?*posthog.PostHogClient,

    pub fn capture(self: *Self, comptime E: type, event: E) void {
        const ph = self.client orelse return;
        const props = event.properties();
        const did = if (@hasField(E, DISTINCT_ID_FIELD))
            distinctIdOrSystem(event.distinct_id)
        else
            S_SYSTEM;
        ph.capture(.{
            .distinct_id = did,
            .event = @tagName(E.kind),
            .properties = &props,
        }) catch |err| {
            log.warn("posthog.capture_failed", .{ .event = @tagName(E.kind), .err = @errorName(err) });
        };
    }
};

pub const TestBackend = struct {
    // Thread-local so a harness's server/worker threads (which emit telemetry
    // through this same backend in test builds) can't pollute the recorded
    // state a unit test captures and asserts on its own thread. Every assertion
    // here reads what the asserting thread itself recorded.
    threadlocal var ring: [64]?RecordedEvent = [_]?RecordedEvent{null} ** 64;
    threadlocal var count: usize = 0;

    // The ring above is deliberately threadlocal, which makes it blind to an
    // integration test: that drives the real handler on an httpz worker thread
    // and asserts from the test thread. This per-kind tally is the cross-thread
    // view — counts only, so it can stay allocation-free and lock-free.
    const KIND_COUNT = @typeInfo(EventKind).@"enum".fields.len;
    var global_counts = [_]std.atomic.Value(u32){std.atomic.Value(u32).init(0)} ** KIND_COUNT;

    pub fn capture(_: *TestBackend, comptime E: type, event: E) void {
        const did = if (@hasField(E, DISTINCT_ID_FIELD)) event.distinct_id else S_SYSTEM;
        const wid = if (@hasField(E, "workspace_id")) event.workspace_id else "";
        ring[count % 64] = RecordedEvent.initFromSlices(E.kind, did, wid);
        count += 1;
        _ = global_counts[@intFromEnum(E.kind)].fetchAdd(1, .acq_rel);
    }

    pub fn reset() void {
        ring = [_]?RecordedEvent{null} ** 64;
        count = 0;
    }

    /// Captures of one kind recorded by ANY thread since `resetGlobal`.
    pub fn globalCount(kind: EventKind) u32 {
        // safe because: the paired fetchAdd releases, so an acquire load here
        // observes every capture that happened-before it on the worker thread.
        return global_counts[@intFromEnum(kind)].load(.acquire);
    }

    /// Clear the cross-thread tally. Call before driving the path under test.
    pub fn resetGlobal() void {
        for (&global_counts) |*value| value.store(0, .release);
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
    const Self = @This();

    backend: Backend,

    pub fn capture(self: *Self, comptime E: type, event: E) void {
        self.backend.capture(E, event);
    }

    /// Production init — wraps a PostHog client (nullable for graceful degradation).
    /// `Backend` is comptime-selected: TestBackend (no `.client` field) in test
    /// builds, ProdBackend otherwise. A test that drives the real `serve.run`
    /// reaches this through `initTelemetry`, so in a test build it must construct
    /// the empty test backend instead of a `.client` literal TestBackend lacks.
    /// Production (`!is_test`) is unchanged.
    pub fn initProd(client: ?*posthog.PostHogClient) Telemetry {
        if (builtin.is_test) return .{ .backend = .{} };
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
