//! The `user.deleted` arm of the Clerk identity-events webhook, split from
//! `identity_events_clerk.zig` for the file-length budget. Unregisters the
//! tenant's upstream schedule timers while their rows still exist, then
//! hard-purges the tenant (`account_teardown`).

const logging = @import("log");

const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");
const account_teardown = @import("../../../state/account_teardown.zig");
const cron_sync = @import("../fleets/cron_sync.zig");
const metrics = @import("../../../observability/metrics_counters.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.auth_identity_events);

pub fn runDelete(hx: Hx, oidc_subject: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        log.warn("delete_pool_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .oidc = oidc_subject, .req_id = hx.req_id });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Upstream schedule timers do not cascade: the provider keeps firing a
    // registration whose rows this purge is about to erase. Unregister first,
    // while the schedule rows still exist. Erasure wins over a provider
    // failure — the counter + log line make a leaked timer reconcilable, but a
    // user's deletion request is never blocked on a third party.
    unregisterTenantSchedules(hx, conn, oidc_subject);

    const purged = account_teardown.purgeByOidcSubject(conn, hx.alloc, oidc_subject) catch |err| {
        log.warn("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .oidc = oidc_subject, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.debug("user_deleted", .{ .oidc = oidc_subject, .purged = purged, .req_id = hx.req_id });
    hx.ok(.ok, .{ .deleted = true });
}

/// Best-effort provider-side unregister for every schedule the tenant's fleets
/// own. `.unconfigured` (no schedule provider in this environment) is silent —
/// nothing was registered upstream. Any other non-ok result is counted and
/// logged per fleet, never propagated: replaying the webhook after the purge
/// cannot retry this (the rows are gone), so the metric is the reconciliation
/// signal.
fn unregisterTenantSchedules(hx: Hx, conn: anytype, oidc_subject: []const u8) void {
    const fleet_ids = account_teardown.fleetIdsByOidcSubject(conn, hx.alloc, oidc_subject) catch |err| {
        metrics.incTeardownUnregisterFailure();
        log.warn("delete_schedule_scan_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .oidc = oidc_subject, .err = @errorName(err), .req_id = hx.req_id });
        return;
    } orelse return;
    defer {
        for (fleet_ids) |id| hx.alloc.free(id);
        hx.alloc.free(fleet_ids);
    }
    for (fleet_ids) |fleet_id| {
        switch (cron_sync.removeAll(hx, fleet_id)) {
            .ok, .skipped => {},
            .unconfigured => {},
            else => {
                metrics.incTeardownUnregisterFailure();
                log.warn("delete_schedule_unregister_failed", .{ .error_code = ec.ERR_SCHEDULE_PROVIDER_UNAVAILABLE, .fleet_id = fleet_id, .req_id = hx.req_id });
            },
        }
    }
}
