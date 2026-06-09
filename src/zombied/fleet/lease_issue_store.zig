const clock = @import("common").clock;

const hx_mod = @import("../http/handlers/hx.zig");
const protocol = @import("contract").protocol;
const id_format = @import("../types/id_format.zig");
const affinity = @import("affinity.zig");
const assign = @import("assign.zig");
const runner_events = @import("runner_events.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

const Hx = hx_mod.Hx;

/// The lease-row billing fields resolved at issue (fresh) or carried from the
/// prior lease (reclaim). Arena-scoped by the lease handler.
pub const Billed = struct {
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
    /// Resolved provider for a FRESH lease, carried from billing so the key
    /// the lease bills is the exact key it delivers. Null on reclaim.
    provider: ?tenant_provider.ResolvedProvider = null,
};

/// Build and persist the `fleet.runner_leases` row plus its `lease_acquired`
/// event. Fresh leases reset the per-zombie metering cursor; reclaimed leases
/// inherit the dead holder's cursor.
pub fn insertLeaseRow(hx: Hx, runner_id: []const u8, acq: assign.Acquired, billed: Billed, lease_id: []const u8) !void {
    const conn = hx.ctx.pool.acquire() catch return error.DbError;
    defer hx.ctx.pool.release(conn);
    const event_row_id = try id_format.generateRunnerEventId(hx.alloc);
    defer hx.alloc.free(event_row_id);
    const now_ms = clock.nowMillis();
    if (acq.kind == .fresh) affinity.resetCursor(conn, acq.zombie_id, now_ms) catch return error.DbError;
    const provider_name: []const u8 = if (billed.provider) |p| p.provider else "";
    _ = conn.exec(
        \\WITH inserted AS (
        \\  INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id,
        \\   actor, event_type, request_json, event_created_at,
        \\   posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6,
        \\        $7, $8, $9, $10, $11, $12, $13,
        \\        0, 0, 0, $17,
        \\        $14, $15, $16, $17, $17)
        \\  RETURNING id, runner_id, zombie_id, event_id
        \\)
        \\INSERT INTO fleet.runner_events
        \\  (id, runner_id, event_type, occurred_at, metadata, dedup_key, created_at)
        \\SELECT $18::uuid, runner_id, $19, $17,
        \\       jsonb_build_object($20, id::text, $21, zombie_id::text, $22, event_id, $23, $24),
        \\       NULL, $17
        \\FROM inserted
    , .{
        lease_id,
        runner_id,
        acq.zombie_id,
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
        runner_events.META_ZOMBIE_ID,
        runner_events.META_ZOMBIE_EVENT_ID,
        runner_events.META_KIND,
        @tagName(acq.kind),
    }) catch return error.DbError;
}
