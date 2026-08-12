const std = @import("std");
const pg = @import("pg");
const store = @import("tenant_billing_store.zig");
const tenant_provider = @import("tenant_provider.zig");
const logging = @import("log");

const log = logging.scoped(.state);

/// Canonical nanos-per-USD conversion factor. 1 USD = 1_000_000_000 nanos
/// (1 nano = 1/1,000,000,000 USD). Mirrors `NANOS_PER_USD` in
/// `ui/packages/app/lib/types.ts` and `cli/src/constants/billing.js`.
pub const NANOS_PER_USD: i64 = 1_000_000_000;

/// $5 starter grant in nanos.
pub const STARTER_CREDIT_NANOS: i64 = 5 * NANOS_PER_USD;
const BOOTSTRAP_GRANT_SOURCE = "bootstrap_starter_grant";

// Credit-pool cost model — expressed in nanos. Event receipts are free under
// both postures. Active agent runtime is metered per second at the single
// RUN_NANOS_PER_SEC rate (identical for both postures); platform posture adds
// the per-token model cost on top, self-managed leaves tokens to the user's
// own provider bill.
pub const Posture = tenant_provider.Mode;

/// Receive-side per-event drain. Zero under both postures.
pub const EVENT_NANOS: i64 = 0;

/// Run-time rate: $0.0001/sec = 100K nanos per active second (≈ $0.36/hr),
/// charged identically under both postures. Runtime is metered by the second
/// as the agent works — not estimated once at lease issue — so a slice's run
/// fee is `runFee(elapsed_ms)`. The per-token model cost (platform posture
/// only) is added on top.
pub const RUN_NANOS_PER_SEC: i64 = 100_000;

/// Conservative estimate floors used by the gate-time stage-cost projection
/// (the runner doesn't know real token counts yet). The actual cost is
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

/// Caller owns the grant_source slice. Surface for both
/// `GET /v1/tenants/me/billing` and `agentsfleet doctor --json`.
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
    _ = @import("tenant_billing_rates.zig");
}
