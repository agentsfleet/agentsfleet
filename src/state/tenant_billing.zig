const std = @import("std");
const pg = @import("pg");
const error_codes = @import("../errors/error_registry.zig");
const store = @import("tenant_billing_store.zig");

const log = std.log.scoped(.state);

pub const FREE_PLAN_INITIAL_BALANCE_CENTS: i64 = 1000;
pub const FREE_PLAN_CENTS_PER_AGENT_SECOND: i64 = 1;
pub const BOOTSTRAP_GRANT_SOURCE = "bootstrap_free_grant";
pub const DEFAULT_FREE_PLAN_TIER = "free";
pub const DEFAULT_FREE_PLAN_SKU = "free_default";

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

pub fn provisionFreeDefault(conn: *pg.Conn, tenant_id: []const u8) !void {
    return provision(conn, tenant_id, DEFAULT_FREE_PLAN_TIER, DEFAULT_FREE_PLAN_SKU, FREE_PLAN_INITIAL_BALANCE_CENTS, BOOTSTRAP_GRANT_SOURCE);
}

pub fn debit(conn: *pg.Conn, tenant_id: []const u8, cents: i64) !DebitResult {
    const r = try store.debit(conn, tenant_id, cents);
    return .{ .balance_cents = r.balance_cents, .updated_at_ms = r.updated_at_ms };
}

/// Caller owns all slice fields (plan_tier, plan_sku, grant_source).
pub fn getBilling(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
) !?Billing {
    var row = (try store.loadByTenant(conn, alloc, tenant_id)) orelse return null;
    return .{
        .plan_tier = row.plan_tier,
        .plan_sku = row.plan_sku,
        .balance_cents = row.balance_cents,
        .grant_source = row.grant_source,
        .updated_at_ms = row.updated_at_ms,
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

pub fn runtimeUsageCostCents(agent_seconds: u64) i64 {
    if (agent_seconds == 0) return 0;
    const seconds = std.math.cast(i64, agent_seconds) orelse return std.math.maxInt(i64);
    return std.math.mul(i64, seconds, FREE_PLAN_CENTS_PER_AGENT_SECOND) catch std.math.maxInt(i64);
}

test {
    _ = @import("tenant_billing_test.zig");
}
