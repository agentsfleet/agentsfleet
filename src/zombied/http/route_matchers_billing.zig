// Tenant + admin billing route matchers — split out of route_matchers.zig to
// keep that file within the 350-line limit (RULE FLL). Operates on the same
// canonical `Path` view; re-exported from route_matchers.zig so call sites stay
// unchanged.

const Path = @import("route_matchers.zig").Path;

// ── /admin/platform-keys/{provider} ────────────────────────────────────────

pub fn matchAdminPlatformKey(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, "admin") or !p.eq(1, "platform-keys")) return null;
    return p.param(2);
}

// ── /api-keys/{id} ─────────────────────────────────────────────────────────

pub fn matchTenantApiKeyById(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, "api-keys")) return null;
    return p.param(1);
}

// ── /tenants/me/billing/charges/{event_id}/metering-periods ────────────────
// The per-renewal slice breakdown behind one charge; returns the event_id.

pub fn matchTenantMeteringPeriods(p: Path) ?[]const u8 {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, "tenants") or !p.eq(1, "me") or !p.eq(2, "billing") or
        !p.eq(3, "charges") or !p.eq(5, "metering-periods")) return null;
    return p.param(4);
}
