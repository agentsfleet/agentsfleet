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

    const limit = parseLimit(qs) catch |err| {
        const msg = switch (err) {
            error.LimitNotNumeric => "limit must be a positive integer",
            error.LimitOutOfRange => "limit must be between 1 and 200",
        };
        hx.fail(ec.ERR_INVALID_REQUEST, msg);
        return;
    };

    // `?cursor=` (empty value) is treated as "no cursor" rather than
    // bouncing the caller with a 400 — empty cursor isn't malformed, it's
    // just the first page expressed verbosely.
    const cursor: ?[]const u8 = if (qs.get("cursor")) |c|
        (if (c.len == 0) null else c)
    else
        null;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // `listTelemetryForTenant` returns the union of cursor_mod.parseCursor's
    // error set + the pg driver's anyerror surface, hence the open `!`. Only
    // `error.InvalidCursor` is a 400; everything else (PG, alloc) is a 500.
    // The local helpers like `parseLimit` use a closed set because their
    // surface is tiny and operator-discrimination matters; this call sits
    // on top of pg + cursor_mod, so widening to anyerror is intentional.
    const rows = telemetry_store.listTelemetryForTenant(conn, hx.alloc, tenant_id, limit, cursor) catch |err| {
        if (err == error.InvalidCursor) {
            hx.fail(ec.ERR_INVALID_REQUEST, "invalid cursor");
            return;
        }
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    // A full page → emit a next_cursor pointing at the last row so the
    // caller can ask for the next slice. Short page → null (no more rows).
    //
    // `hx.alloc` is the per-request arena (see hx.zig); it's freed when
    // the response is serialized, so the rows slice + the cursor token
    // both ride that lifetime — no manual deinit / dupe needed.
    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0) blk: {
        break :blk telemetry_store.makeCursor(hx.alloc, rows[rows.len - 1]) catch {
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
    } else null;

    hx.ok(.ok, .{ .items = rows, .next_cursor = next_cursor });
}

const ParseLimitError = error{ LimitNotNumeric, LimitOutOfRange };

fn parseLimit(qs: anytype) ParseLimitError!u32 {
    const raw = qs.get("limit") orelse return USAGE_LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.LimitNotNumeric;
    if (n == 0 or n > USAGE_LIMIT_MAX) return error.LimitOutOfRange;
    return n;
}

// ── Tests ──────────────────────────────────────────────────────────────────

const FakeQuery = struct {
    value: ?[]const u8,
    pub fn get(self: FakeQuery, key: []const u8) ?[]const u8 {
        _ = key;
        return self.value;
    }
};

test "parseLimit: missing limit returns the default" {
    const qs = FakeQuery{ .value = null };
    try std.testing.expectEqual(USAGE_LIMIT_DEFAULT, try parseLimit(qs));
}

test "parseLimit: numeric in-range value passes through" {
    const qs = FakeQuery{ .value = "10" };
    try std.testing.expectEqual(@as(u32, 10), try parseLimit(qs));
}

test "parseLimit: zero rejected as LimitOutOfRange" {
    const qs = FakeQuery{ .value = "0" };
    try std.testing.expectError(error.LimitOutOfRange, parseLimit(qs));
}

test "parseLimit: above max rejected as LimitOutOfRange" {
    const qs = FakeQuery{ .value = "201" };
    try std.testing.expectError(error.LimitOutOfRange, parseLimit(qs));
}

test "parseLimit: non-numeric rejected as LimitNotNumeric" {
    const qs = FakeQuery{ .value = "lots" };
    try std.testing.expectError(error.LimitNotNumeric, parseLimit(qs));
}

test "parseLimit: negative input rejected as LimitNotNumeric (u32 parse rejects sign)" {
    const qs = FakeQuery{ .value = "-1" };
    try std.testing.expectError(error.LimitNotNumeric, parseLimit(qs));
}

test "parseLimit: max boundary is accepted" {
    const qs = FakeQuery{ .value = "200" };
    try std.testing.expectEqual(@as(u32, 200), try parseLimit(qs));
}
