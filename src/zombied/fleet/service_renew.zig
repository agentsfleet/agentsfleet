//! zombied-side runner control-plane orchestration — the `renew` verb.
//!
//! `POST /v1/runners/me/leases/{lease_id}/renew` lets a runner that is actively
//! executing push its kill deadline forward so a legitimate long run is never
//! reclaimed mid-flight. Orchestration around the atomic dual-row extend in
//! `renewal.zig`:
//!
//!   1. Load the lease scoped to the presenting runner (ownership = runner_id).
//!      No row → 404 lease_not_found; not `active` → 409 lease_lost.
//!   2. Credit gate: refuse (402) if the tenant balance can no longer cover the
//!      run — reuses the same `balanceCoversEstimate` the lease path uses.
//!   3. `renewal.renew` — atomic, fenced, capped extension of BOTH the lease row
//!      and the affinity slot. renewed → 200; max_runtime → 410-class 409 (010);
//!      lost (reclaimed/fenced between load and extend) → 409 (011).
//!   4. On success, bump `fleet.runners.last_seen_at` — the runner is single-
//!      threaded and does not heartbeat during a long execution, so the renewal
//!      doubles as its liveness signal (else §2 lapse-detection would reassign a
//!      live runner's own lease). Best-effort.
//!
//! Allocator: per-request arena (`hx.alloc`).

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const protocol = @import("contract").protocol;
const renewal = @import("renewal.zig");
const metering = @import("../zombie/metering.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_renew);

/// The lease fields renewal needs before the atomic extend: tenant + metering
/// context for the credit gate, and the status to reject a non-active lease
/// early. Arena-dup'd off the row.
const Lease = struct {
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
    status: []const u8,
};

/// POST /v1/runners/me/leases/{lease_id}/renew — extend a live lease's deadline.
pub fn renew(hx: Hx, req: *httpz.Request, lease_id: []const u8) void {
    _ = req; // empty body — the lease_id path param + Bearer identity are the input.
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };

    const lease = loadLease(hx, runner_id, lease_id) orelse {
        hx.fail(ec.ERR_RUN_LEASE_NOT_FOUND, "No lease matches this lease_id for the runner");
        return;
    };
    if (!std.mem.eql(u8, lease.status, protocol.RUNNER_LEASE_STATUS_ACTIVE)) {
        hx.fail(ec.ERR_RUN_LEASE_LOST, "Lease is no longer active; it was reclaimed or already reported");
        return;
    }
    if (!creditsCover(hx, lease)) {
        log.info("renew_no_credits", .{ .error_code = ec.ERR_RUN_LEASE_RENEWAL_NO_CREDITS, .runner_id = runner_id, .lease_id = lease_id });
        hx.fail(ec.ERR_RUN_LEASE_RENEWAL_NO_CREDITS, "Tenant balance can no longer fund this run; not renewed");
        return;
    }

    completeRenew(hx, runner_id, lease_id);
}

/// Run the atomic extend and map its verdict to the wire. Split out to keep
/// `renew` within the method-length budget.
fn completeRenew(hx: Hx, runner_id: []const u8, lease_id: []const u8) void {
    const outcome = runRenew(hx, lease_id, runner_id) catch {
        @import("../http/handlers/common.zig").internalDbError(hx.res, hx.req_id);
        return;
    };
    switch (outcome) {
        .renewed => |until| {
            bumpLastSeen(hx, runner_id);
            log.info("lease_renewed", .{ .runner_id = runner_id, .lease_id = lease_id, .lease_expires_at = until });
            hx.ok(.ok, protocol.RenewResponse{ .lease_expires_at = until });
        },
        .max_runtime => |cap| {
            log.info("renew_max_runtime", .{ .error_code = ec.ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME, .runner_id = runner_id, .lease_id = lease_id, .hard_cap = cap });
            hx.fail(ec.ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME, "Lease reached the hard max runtime; not renewed");
        },
        .lost => {
            log.info("renew_lost", .{ .error_code = ec.ERR_RUN_LEASE_LOST, .runner_id = runner_id, .lease_id = lease_id });
            hx.fail(ec.ERR_RUN_LEASE_LOST, "Lease was reassigned before this renewal; terminate the child");
        },
    }
}

fn runRenew(hx: Hx, lease_id: []const u8, runner_id: []const u8) !renewal.RenewOutcome {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    return renewal.renew(conn, lease_id, runner_id, std.time.milliTimestamp());
}

/// The tenant balance gate — reuse the exact check the lease path applies, so
/// renewal and issue share one credit policy. Policy is resolved once at
/// startup and carried on the request context (not re-read from the env here).
fn creditsCover(hx: Hx, lease: Lease) bool {
    return metering.balanceCoversEstimate(hx.ctx.pool, hx.alloc, lease.tenant_id, parsePosture(lease.posture), lease.model, hx.ctx.balance_policy);
}

fn loadLease(hx: Hx, runner_id: []const u8, lease_id: []const u8) ?Lease {
    return loadLeaseInner(hx, runner_id, lease_id) catch |err| {
        log.warn("renew_lease_load_failed", .{ .lease_id = lease_id, .err = @errorName(err) });
        return null;
    };
}

fn loadLeaseInner(hx: Hx, runner_id: []const u8, lease_id: []const u8) !?Lease {
    const conn = try hx.ctx.pool.acquire();
    defer hx.ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT tenant_id::text, posture, model, status
        \\FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid
    , .{ lease_id, runner_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return Lease{
        .tenant_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0)),
        .posture = try hx.alloc.dupe(u8, try row.get([]const u8, 1)),
        .model = try hx.alloc.dupe(u8, try row.get([]const u8, 2)),
        .status = try hx.alloc.dupe(u8, try row.get([]const u8, 3)),
    };
}

/// A renewing runner is provably alive — bump its liveness bookmark so §2's
/// heartbeat-lapse scan does not reassign the lease it is actively running.
/// Best-effort: a DB blip must not fail an already-succeeded renewal.
fn bumpLastSeen(hx: Hx, runner_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid
    , .{ runner_id, now_ms }) catch |err| {
        log.warn("renew_last_seen_bump_failed", .{ .runner_id = runner_id, .err = @errorName(err) });
    };
}

/// Map the stored posture label back to `Mode` for the balance gate (mirrors
/// service_report); keyed on the enum's own `label()` (RULE UFS), unknown → platform.
fn parsePosture(label: []const u8) tenant_provider.Mode {
    if (std.mem.eql(u8, label, tenant_provider.Mode.self_managed.label())) return .self_managed;
    return .platform;
}
