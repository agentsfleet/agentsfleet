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

/// Three staged steps, each holding a pool connection only for its own
/// database work.
///
/// The provider round trips in step two run with NO connection held by this
/// request. The previous shape kept one open across them while `cron_sync`
/// reached for a second: with the default four-slot pool and its two-second
/// acquire timeout, four concurrent deletions could occupy every slot, make
/// each nested acquire time out, and then purge anyway — every upstream timer
/// surviving, every row that named it gone. No step now holds two slots, so
/// deletions queue on the pool instead of deadlocking against each other.
pub fn runDelete(hx: Hx, oidc_subject: []const u8) void {
    // Step 1 — enumerate, on a connection released before we leave.
    const fleet_ids = enumerateTenantFleets(hx, oidc_subject);
    defer if (fleet_ids) |ids| {
        for (ids) |id| hx.alloc.free(id);
        hx.alloc.free(ids);
    };

    // Step 2 — upstream schedule timers do not cascade: the provider keeps
    // firing a registration whose rows this purge is about to erase.
    // Unregister first, while the schedule rows still exist. Erasure wins over
    // a provider failure — the counter and log lines make a leaked timer
    // reconcilable, but a user's deletion request is never blocked on a third
    // party.
    if (fleet_ids) |ids| unregisterTenantSchedules(hx, ids);

    // Step 3 — purge, on its own connection.
    const conn = hx.ctx.pool.acquire() catch {
        log.warn("delete_pool_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .oidc = oidc_subject, .req_id = hx.req_id });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const result = account_teardown.purgeByOidcSubject(conn, hx.alloc, oidc_subject, fleet_ids orelse &.{}) catch |err| {
        log.warn("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .oidc = oidc_subject, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    // Only when the enumeration itself succeeded. A failed enumeration already
    // counted itself, and every fleet would be unenumerated by definition —
    // reporting it again would bill one incident to the metric twice.
    if (fleet_ids != null) reportPurgeRace(hx, oidc_subject, result);

    log.debug("user_deleted", .{ .oidc = oidc_subject, .purged = result.purged, .req_id = hx.req_id });
    hx.ok(.ok, .{ .deleted = true });
}

/// Fleet ids owned by the subject's tenant, read while their rows still exist.
/// Null means there is nothing to unregister — an unknown subject, an already
/// purged one, or a read this process could not complete. A failure here is
/// counted and swallowed rather than propagated: erasure is never blocked on
/// the unregister pass succeeding.
fn enumerateTenantFleets(hx: Hx, oidc_subject: []const u8) ?[][]const u8 {
    const conn = hx.ctx.pool.acquire() catch {
        metrics.incTeardownUnregisterFailure();
        log.warn("delete_schedule_scan_pool_unavailable", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .oidc = oidc_subject, .req_id = hx.req_id });
        return null;
    };
    defer hx.ctx.pool.release(conn);
    return account_teardown.fleetIdsByOidcSubject(conn, hx.alloc, oidc_subject) catch |err| {
        metrics.incTeardownUnregisterFailure();
        log.warn("delete_schedule_scan_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .oidc = oidc_subject, .err = @errorName(err), .req_id = hx.req_id });
        return null;
    };
}

/// A fleet that appeared between the enumeration and the purge was erased
/// without its upstream timer being retired — an already-authorized concurrent
/// request can still create one while the deletion is in flight. Closing that
/// window needs a tenant-level deleting marker every write path honours, which
/// is a security boundary this workstream does not open; naming the leak is
/// what makes it reconcilable in the meantime.
///
/// The purge answers by identity (fleets it erased that the enumeration never
/// named), so a create that a concurrent delete offsets cannot hide inside an
/// unchanged count.
fn reportPurgeRace(hx: Hx, oidc_subject: []const u8, result: account_teardown.PurgeResult) void {
    if (result.unenumerated_fleets == 0) return;
    metrics.incTeardownUnregisterFailure();
    log.warn("delete_schedule_purge_race", .{
        .error_code = ec.ERR_SCHEDULE_PROVIDER_UNAVAILABLE,
        .oidc = oidc_subject,
        .unenumerated_fleets = result.unenumerated_fleets,
        .req_id = hx.req_id,
    });
}

/// Best-effort provider-side unregister for every schedule the tenant's fleets
/// own. Every fleet is attempted regardless of what the previous one returned,
/// and no result is propagated: replaying the webhook after the purge cannot
/// retry this (the rows are gone), so the metric plus `cron_sync`'s
/// per-schedule log lines are the reconciliation signal.
fn unregisterTenantSchedules(hx: Hx, fleet_ids: []const []const u8) void {
    for (fleet_ids) |fleet_id| {
        switch (cron_sync.removeAll(hx, fleet_id)) {
            // `.skipped` is the only genuine no-op: the fleet owned no
            // schedule rows, so there is nothing upstream to retire.
            .ok, .skipped => {},
            // NOT silence. `removeAll` answers `.skipped` for an empty list
            // before it ever resolves credentials, so `.unconfigured` here
            // means schedules existed and none were retired — this process
            // simply had no credentials to do it with. Credentials go absent
            // after a startup vault or database fault, which made a transient
            // restart error turn every subsequent account deletion into an
            // invisible upstream leak. It is counted like any other failure;
            // the separate log line keeps "we had no key" distinguishable from
            // "the provider refused".
            .unconfigured => {
                metrics.incTeardownUnregisterFailure();
                log.warn("delete_schedule_unregister_unconfigured", .{ .error_code = ec.ERR_SCHEDULE_NOT_CONFIGURED, .fleet_id = fleet_id, .req_id = hx.req_id });
            },
            else => {
                metrics.incTeardownUnregisterFailure();
                log.warn("delete_schedule_unregister_failed", .{ .error_code = ec.ERR_SCHEDULE_PROVIDER_UNAVAILABLE, .fleet_id = fleet_id, .req_id = hx.req_id });
            },
        }
    }
}
