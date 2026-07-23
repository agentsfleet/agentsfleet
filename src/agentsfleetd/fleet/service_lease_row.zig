//! The `fleet.runner_leases` row write for the `lease` verb — the durable
//! persistence step of `service.issueLease`, plus the billing-input struct it
//! consumes. Split from `service.zig` to keep that file under the RULE FLL
//! line limit; `service.zig` re-exports `Billed` so the billing helpers keep
//! naming the type. Single consumer: `service.issueLease`.

const clock = @import("common").clock;
const protocol = @import("contract").protocol;
const assign = @import("assign.zig");
const affinity = @import("affinity.zig");
const sql = @import("sql.zig");
const id_format = @import("../types/id_format.zig");
const runner_events = @import("runner_events.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const Hx = @import("../http/handlers/hx.zig").Hx;

/// The lease-row billing fields resolved at issue (fresh) or carried from the
/// prior lease (reclaim). Arena-scoped (see the `service.zig` module note).
pub const Billed = struct {
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
    /// Resolved provider for a FRESH lease, carried from `runBilling` so the key
    /// the lease bills is the exact key it delivers — one vault decryption, no
    /// rotation TOCTOU between billing and delivery. Null on reclaim (no billing
    /// pass); `issueLease` re-resolves. Owned: `issueLease` deinits (secureZero)
    /// after `hx.ok` serializes.
    provider: ?tenant_provider.ResolvedProvider = null,
};

pub fn insertLeaseRow(hx: Hx, runner_id: []const u8, acq: assign.Acquired, billed: Billed, lease_id: []const u8) !void {
    const conn = hx.ctx.pool.acquire() catch return error.DbError;
    defer hx.ctx.pool.release(conn);
    const event_row_id = try id_format.generateRunnerEventId(hx.alloc);
    defer hx.alloc.free(event_row_id);
    const now_ms = clock.nowMillis();
    // Fresh event → reset the per-fleet metering cursor (the slot may carry a
    // prior run's preserved cursor); a reclaim leaves it so the re-leased run
    // meters forward. Fail-closed: a reset error fails issue, never over-charges.
    if (acq.kind == .fresh) affinity.resetCursor(conn, acq.fleet_id, now_ms) catch return error.DbError;
    // The provider name resolved at billing — stored alongside posture/model so
    // the renew credit gate + the report settle can key the rate row by
    // (provider, model) without re-resolving. Fresh leases always carry it.
    const provider_name: []const u8 = if (billed.provider) |p| p.provider else "";
    // metered_* = 0 + last_metered_at_ms = now ($17) seed the incremental-
    // metering cursor at issue (Invariant 9 — never read NULL). A reclaimed
    // re-lease carries the dead holder's cursor forward instead (wired with the
    // /renew Δ-charge), so the new holder meters from where it stopped.
    _ = conn.exec(sql.INSERT_LEASE_WITH_EVENT, .{
        lease_id,
        runner_id,
        acq.fleet_id,
        acq.workspace_id,
        billed.tenant_id,
        acq.event_id,
        acq.actor,
        acq.event_type,
        acq.request_json,
        acq.event_created_at,
        billed.posture,
        provider_name,
        billed.model,
        @as(i64, @intCast(acq.fencing_token)),
        acq.leased_until,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        now_ms,
        event_row_id,
        @tagName(protocol.RunnerEventType.lease_acquired),
        runner_events.META_LEASE_ID,
        runner_events.META_FLEET_ID,
        runner_events.META_AGENTSFLEET_EVENT_ID,
        runner_events.META_KIND,
        @tagName(acq.kind),
    }) catch return error.DbError;
}
