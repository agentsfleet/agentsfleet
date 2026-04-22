//! GET /v1/tenants/me/billing — tenant-scoped billing snapshot.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const tenant_billing = @import("../../state/tenant_billing.zig");

const Hx = hx_mod.Hx;

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
