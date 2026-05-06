// Tenant + provider resolution and approval gate dispatch for the worker
// write path. Split out of event_loop_writepath.zig to keep both files
// under the 350-line cap.

const std = @import("std");
const Allocator = std.mem.Allocator;

const redis_zombie = @import("../queue/redis_zombie.zig");
const event_loop_gate = @import("event_loop_gate.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const logging = @import("log");

const types = @import("event_loop_types.zig");
const ZombieSession = types.ZombieSession;
const EventLoopConfig = types.EventLoopConfig;

const rows = @import("event_loop_writepath_rows.zig");

const log = logging.scoped(.zombie_event_loop);

pub const ResolveOutcome = union(enum) {
    /// Caller takes ownership of both fields and must free tenant_id with
    /// alloc.free and call resolved.deinit(alloc) before returning from run.
    resolved: struct { tenant_id: []u8, resolved: tenant_provider.ResolvedProvider },
    /// User-fixable BYOK error: dead-letter the event and return.
    dead_letter: []const u8, // failure_label
    /// Operator-side or transient error: sleep + retry path.
    transient_err: void,
};

pub fn resolveTenantAndProvider(
    alloc: Allocator,
    cfg: EventLoopConfig,
    session: *ZombieSession,
    event_id: []const u8,
) ResolveOutcome {
    const conn = cfg.pool.acquire() catch |err| {
        log.warn("resolve_acquire_fail", .{ .zombie_id = session.zombie_id, .err = @errorName(err) });
        return .{ .transient_err = {} };
    };
    defer cfg.pool.release(conn);

    const tenant_id = tenant_billing.resolveTenantFromWorkspace(conn, alloc, session.workspace_id) catch |err| {
        log.err("tenant_lookup_fail", .{ .zombie_id = session.zombie_id, .workspace_id = session.workspace_id, .err = @errorName(err) });
        return .{ .transient_err = {} };
    };
    errdefer alloc.free(tenant_id);

    const resolved = tenant_provider.resolveActiveProvider(alloc, conn, tenant_id) catch |err| switch (err) {
        tenant_provider.ResolveError.CredentialMissing => {
            log.warn("byok_credential_missing", .{ .zombie_id = session.zombie_id, .tenant_id = tenant_id, .event_id = event_id });
            alloc.free(tenant_id);
            return .{ .dead_letter = rows.LABEL_PROVIDER_CREDENTIAL_MISSING };
        },
        tenant_provider.ResolveError.CredentialDataMalformed => {
            log.warn("byok_credential_malformed", .{ .zombie_id = session.zombie_id, .tenant_id = tenant_id, .event_id = event_id });
            alloc.free(tenant_id);
            return .{ .dead_letter = rows.LABEL_PROVIDER_CREDENTIAL_MALFORMED };
        },
        else => {
            log.err("resolve_provider_fail", .{ .zombie_id = session.zombie_id, .tenant_id = tenant_id, .err = @errorName(err) });
            alloc.free(tenant_id);
            return .{ .transient_err = {} };
        },
    };
    return .{ .resolved = .{ .tenant_id = tenant_id, .resolved = resolved } };
}

pub const ApprovalOutcome = union(enum) {
    passed: void,
    blocked: []const u8, // failure_label
};

pub fn checkApprovalOnly(
    alloc: Allocator,
    cfg: EventLoopConfig,
    session: *ZombieSession,
    event: *const redis_zombie.ZombieEvent,
) ApprovalOutcome {
    const gate = event_loop_gate.checkApprovalGate(alloc, session, event, cfg.pool, cfg.redis);
    return switch (gate) {
        .passed => .{ .passed = {} },
        .blocked => |reason| .{ .blocked = switch (reason) {
            .approval_denied => rows.LABEL_APPROVAL_DENIED,
            .timeout => rows.LABEL_APPROVAL_TIMEOUT,
            .unavailable => rows.LABEL_APPROVAL_UNAVAILABLE,
        } },
        .auto_killed => |trigger| .{ .blocked = switch (trigger) {
            .anomaly => rows.LABEL_AUTO_KILL_ANOMALY,
            .policy => rows.LABEL_AUTO_KILL_POLICY,
        } },
    };
}
