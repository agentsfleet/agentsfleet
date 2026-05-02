//! Tenant-scoped billing endpoints.
//!
//!   GET /v1/tenants/me/billing         — plan + balance snapshot.
//!   GET /v1/tenants/me/billing/charges — newest-first credit-pool charges
//!                                        (one row per (event_id, charge_type);
//!                                        limit-only paging). Backs the
//!                                        Settings → Billing dashboard's
//!                                        Usage tab and `zombiectl billing show`.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const tenant_billing = @import("../../state/tenant_billing.zig");
const telemetry_store = @import("../../state/zombie_telemetry_store.zig");

const Hx = hx_mod.Hx;

const USAGE_LIMIT_DEFAULT: u32 = 50;
const USAGE_LIMIT_MAX: u32 = 200;

pub fn innerGetTenantBilling(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const billing_opt = tenant_billing.getBilling(conn, hx.alloc, tenant_id) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    const billing = billing_opt orelse {
        hx.fail(ec.ERR_INTERNAL_OPERATION_FAILED, "Tenant billing row missing — bootstrap invariant violated");
        return;
    };
    defer hx.alloc.free(@constCast(billing.plan_tier));
    defer hx.alloc.free(@constCast(billing.plan_sku));
    defer hx.alloc.free(@constCast(billing.grant_source));

    hx.ok(.ok, .{
        .plan_tier = billing.plan_tier,
        .plan_sku = billing.plan_sku,
        .balance_cents = billing.balance_cents,
        .updated_at = billing.updated_at_ms,
        .is_exhausted = billing.exhausted_at_ms != null,
        .exhausted_at = billing.exhausted_at_ms,
    });
}

pub fn innerGetTenantBillingCharges(hx: Hx, req: *httpz.Request) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };

    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = telemetry_store.listTelemetryForTenant(conn, hx.alloc, tenant_id, limit) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .items = rows });
}

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return USAGE_LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > USAGE_LIMIT_MAX) return error.InvalidLimit;
    return n;
}
