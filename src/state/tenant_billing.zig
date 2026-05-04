const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/error_registry.zig");
const store = @import("tenant_billing_store.zig");
const tenant_provider = @import("tenant_provider.zig");
const model_rate_cache = @import("model_rate_cache.zig");

const log = std.log.scoped(.state);

/// One-time grant inserted at tenant creation. Funds the first ~166 platform-
/// managed events (3¢ each) or ~500 BYOK events (1¢ each).
pub const STARTER_GRANT_CENTS: i64 = 500;
const BOOTSTRAP_GRANT_SOURCE = "bootstrap_starter_grant";
const STARTER_GRANT_PLAN_TIER = "free";
const STARTER_GRANT_PLAN_SKU = "free_default";

// Credit-pool cost model. No plan-tier branching in the cost functions —
// plans only show up at credit-grant time as different starting balances.
pub const Posture = tenant_provider.Mode;

/// Receive-side per-event drain. Charged once per event after the balance
/// gate passes. BYOK is free at receive (the user's own provider account
/// pays for the LLM call); platform-managed pays a flat 1¢ overhead.
pub const RECEIVE_PLATFORM_CENTS: i64 = 1;
pub const RECEIVE_BYOK_CENTS: i64 = 0;

/// Stage-side flat overhead. Charged once per event before the executor
/// runs, on top of the model-rate-based token charge under platform-managed.
/// BYOK pays this flat charge only — token cost is on the user's account.
pub const STAGE_OVERHEAD_PLATFORM_CENTS: i64 = 1;
pub const STAGE_OVERHEAD_BYOK_CENTS: i64 = 1;

/// Conservative estimate floors used by the gate-time stage-cost projection
/// (the executor doesn't know real token counts yet). The actual cost is
/// charged at this floor; v3 may add reconciliation against StageResult.
pub const ESTIMATE_FLOOR_INPUT_TOKENS: u32 = 100;
pub const ESTIMATE_FLOOR_OUTPUT_TOKENS: u32 = 100;

pub const PlanTier = enum {
    free,
    scale,

    pub fn label(self: PlanTier) []const u8 {
        return switch (self) {
            .free => "free",
            .scale => "scale",
        };
    }

    pub fn parse(raw: []const u8) PlanTier {
        if (std.ascii.eqlIgnoreCase(raw, "scale")) return .scale;
        return .free;
    }
};

pub const Billing = struct {
    plan_tier: []const u8,
    plan_sku: []const u8,
    balance_cents: i64,
    grant_source: []const u8,
    updated_at_ms: i64,
    exhausted_at_ms: ?i64,
};

pub const DebitResult = struct { balance_cents: i64, updated_at_ms: i64 };

const billing_error_table = [_]error_codes.ErrorMapping{
    .{ .err = error.CreditExhausted, .code = error_codes.ERR_CREDIT_EXHAUSTED, .message = "Free plan balance exhausted. Upgrade to Scale to continue." },
};
comptime {
    error_codes.validateErrorTable(&billing_error_table);
}

pub fn errorCode(err: anyerror) ?[]const u8 {
    inline for (billing_error_table) |entry| {
        if (err == entry.err) return entry.code;
    }
    return null;
}

pub fn errorMessage(err: anyerror) ?[]const u8 {
    inline for (billing_error_table) |entry| {
        if (err == entry.err) return entry.message;
    }
    return null;
}

pub fn provision(
    conn: *pg.Conn,
    tenant_id: []const u8,
    plan_tier: []const u8,
    plan_sku: []const u8,
    balance_cents: i64,
    grant_source: []const u8,
) !void {
    try store.insertIfAbsent(conn, tenant_id, plan_tier, plan_sku, balance_cents, grant_source);
    log.info("tenant_billing.provisioned tenant_id={s} balance_cents={d} source={s}", .{ tenant_id, balance_cents, grant_source });
}

/// Insert the one-time $5 starter grant for a new tenant. Called from the
/// tenant-create transaction in signup_bootstrap. Idempotent via the
/// underlying ON CONFLICT DO NOTHING.
pub fn insertStarterGrant(conn: *pg.Conn, tenant_id: []const u8) !void {
    return provision(conn, tenant_id, STARTER_GRANT_PLAN_TIER, STARTER_GRANT_PLAN_SKU, STARTER_GRANT_CENTS, BOOTSTRAP_GRANT_SOURCE);
}

/// Receive-side per-event charge. Posture-only; no token math.
pub fn computeReceiveCharge(posture: Posture) i64 {
    return switch (posture) {
        .platform => RECEIVE_PLATFORM_CENTS,
        .byok => RECEIVE_BYOK_CENTS,
    };
}

/// Stage-side per-event charge. Under platform-managed posture this is the
/// flat overhead plus per-token cost looked up from model_rate_cache; under
/// BYOK it's the flat overhead alone (token cost lands on the user's own
/// provider bill). Panics under platform if `model` is missing from the
/// cache — that condition should have been rejected upstream by the
/// tenant-provider PUT validator and the install-skill frontmatter check.
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
            const in_cents = @divTrunc(rate.input_cents_per_mtok * @as(i64, input_tokens), 1_000_000);
            const out_cents = @divTrunc(rate.output_cents_per_mtok * @as(i64, output_tokens), 1_000_000);
            break :blk STAGE_OVERHEAD_PLATFORM_CENTS + in_cents + out_cents;
        },
        .byok => STAGE_OVERHEAD_BYOK_CENTS,
    };
}

pub fn debit(conn: *pg.Conn, tenant_id: []const u8, cents: i64) !DebitResult {
    const r = try store.debit(conn, tenant_id, cents);
    return .{ .balance_cents = r.balance_cents, .updated_at_ms = r.updated_at_ms };
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

/// Caller owns all slice fields (plan_tier, plan_sku, grant_source).
pub fn getBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?Billing {
    const row = (try store.loadByTenant(conn, alloc, tenant_id)) orelse return null;
    return .{
        .plan_tier = row.plan_tier,
        .plan_sku = row.plan_sku,
        .balance_cents = row.balance_cents,
        .grant_source = row.grant_source,
        .updated_at_ms = row.updated_at_ms,
        .exhausted_at_ms = row.exhausted_at_ms,
    };
}

pub fn getPlanTier(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !PlanTier {
    var row = (try store.loadByTenant(conn, alloc, tenant_id)) orelse return .free;
    defer row.deinit(alloc);
    return PlanTier.parse(row.plan_tier);
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
