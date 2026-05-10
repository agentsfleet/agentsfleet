const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/error_registry.zig");
const store = @import("tenant_billing_store.zig");
const tenant_provider = @import("tenant_provider.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const logging = @import("log");

const log = logging.scoped(.state);

/// $5 starter grant in nanos (1 nano = 1/1,000,000,000 USD; 5 USD = 5e9 nanos).
pub const STARTER_CREDIT_NANOS: i64 = 5_000_000_000;
const BOOTSTRAP_GRANT_SOURCE = "bootstrap_starter_grant";

// Credit-pool cost model — M66 traction rates expressed in nanos.
// Events are free both postures; stages cost $0.001 platform / $0.0001
// self-managed. The 10× gradient between postures is the on-ramp signal:
// platform mode is convenient, self-managed is cheap to scale.
pub const Posture = tenant_provider.Mode;

/// Receive-side per-event drain. M66: zero, both postures.
pub const EVENT_NANOS: i64 = 0;

/// Stage-side platform fee, $0.001 = 1M nanos. Charged once per stage
/// execution before the executor runs under platform posture; the per-token
/// model cost (also in nanos) is added on top.
pub const STAGE_PLATFORM_NANOS: i64 = 1_000_000;

/// Stage-side self-managed fee, $0.0001 = 100K nanos. The user's own provider
/// account pays the token cost directly; we only charge the flat overhead.
pub const STAGE_SELF_MANAGED_NANOS: i64 = 100_000;

/// Conservative estimate floors used by the gate-time stage-cost projection
/// (the executor doesn't know real token counts yet). The actual cost is
/// charged at this floor; v3 may add reconciliation against StageResult.
pub const ESTIMATE_FLOOR_INPUT_TOKENS: u32 = 100;
pub const ESTIMATE_FLOOR_OUTPUT_TOKENS: u32 = 100;

pub const Billing = struct {
    balance_nanos: i64,
    grant_source: []const u8,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64,
};

pub const DebitResult = struct { balance_nanos: i64, updated_at_ms: i64 };

pub fn provision(
    conn: *pg.Conn,
    tenant_id: []const u8,
    balance_nanos: i64,
    grant_source: []const u8,
) !void {
    try store.insertIfAbsent(conn, tenant_id, balance_nanos, grant_source);
    log.info("tenant_billing_provisioned", .{ .tenant_id = tenant_id, .balance_nanos = balance_nanos, .source = grant_source });
}

/// Insert the one-time $5 starter grant for a new tenant. Called from the
/// tenant-create transaction in signup_bootstrap. Idempotent via the
/// underlying ON CONFLICT DO NOTHING.
pub fn insertStarterGrant(conn: *pg.Conn, tenant_id: []const u8) !void {
    return provision(conn, tenant_id, STARTER_CREDIT_NANOS, BOOTSTRAP_GRANT_SOURCE);
}

/// Receive-side per-event charge. M66: zero both postures.
pub fn computeReceiveCharge(posture: Posture) i64 {
    _ = posture;
    return EVENT_NANOS;
}

/// Stage-side per-event charge. Under platform posture this is the flat
/// platform stage fee plus per-token cost looked up from model_rate_cache
/// (both in nanos); under self_managed it's the flat self-managed fee
/// alone (token cost lands on the user's own provider bill). Panics under
/// platform if `model` is missing from the cache — that condition should
/// have been rejected upstream by the tenant-provider PUT validator and
/// the install-skill frontmatter check.
pub fn computeStageCharge(
    posture: Posture,
    model: []const u8,
    input_tokens: u32,
    output_tokens: u32,
) i64 {
    return switch (posture) {
        .platform => blk: {
            const rate = model_rate_cache.lookup_model_rate(model) orelse
                std.debug.panic("compute_stage_charge: model '{s}' not in cached caps catalogue", .{model});
            const in_nanos = @divTrunc(rate.input_nanos_per_mtok * @as(i64, input_tokens), 1_000_000);
            const out_nanos = @divTrunc(rate.output_nanos_per_mtok * @as(i64, output_tokens), 1_000_000);
            break :blk STAGE_PLATFORM_NANOS + in_nanos + out_nanos;
        },
        .self_managed => STAGE_SELF_MANAGED_NANOS,
    };
}

pub fn debit(conn: *pg.Conn, tenant_id: []const u8, nanos: i64) !DebitResult {
    const r = try store.debit(conn, tenant_id, nanos);
    return .{ .balance_nanos = r.balance_nanos, .updated_at_ms = r.updated_at_ms };
}

/// Atomically stamp `balance_exhausted_at` on the first CreditExhausted debit.
/// Returns true if this call transitioned the row (first exhaust), false if
/// the row was already marked. Callers use the return value to gate the
/// one-shot `balance_exhausted_first_debit` activity event.
pub fn markExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return store.markExhausted(conn, tenant_id);
}

/// Clear `balance_exhausted_at` when a tenant is replenished outside the
/// regular `debit` path (admin manual credit, Stripe top-up when wired,
/// etc.). `debit` already clears on a successful deduction, so callers
/// that debit after top-up do not need this — but paths that add credit
/// without a matching debit (refunds, grants, admin SQL) MUST call it
/// or the `stop` gate stays permanently closed.
pub fn clearExhausted(conn: *pg.Conn, tenant_id: []const u8) !bool {
    return store.clearExhausted(conn, tenant_id);
}

/// Caller owns the grant_source slice.
pub fn getBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?Billing {
    const row = (try store.loadByTenant(conn, alloc, tenant_id)) orelse return null;
    return .{
        .balance_nanos = row.balance_nanos,
        .grant_source = row.grant_source,
        .updated_at_ms = row.updated_at_ms,
        .exhausted_at_ms = row.exhausted_at_ms,
    };
}

pub fn resolveTenantFromWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) ![]u8 {
    return store.resolveTenantFromWorkspace(conn, alloc, workspace_id);
}

test {
    _ = @import("tenant_billing_test.zig");
}
