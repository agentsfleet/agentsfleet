// Tenant + admin billing route matchers — split out of route_matchers.zig to
// keep that file within the 350-line limit (RULE FLL). Operates on the same
// canonical `Path` view; re-exported from route_matchers.zig so call sites stay
// unchanged.

const Path = @import("route_matchers.zig").Path;

const SEG_TENANTS = "tenants";
const SEG_ME = "me";
const SEG_ADMIN = "admin";
const SEG_BILLING = "billing";
const SEG_CHARGES = "charges";
const SEG_TELEMETRY = "telemetry";
const SEG_MODELS = "models";

// ── /admin/platform-keys/{provider} ────────────────────────────────────────

pub fn matchAdminPlatformKey(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, SEG_ADMIN) or !p.eq(1, "platform-keys")) return null;
    return p.param(2);
}

// ── /admin/models/{uid} ────────────────────────────────────────────────────
// uid (uuidv7) keys the row — model_id can contain '/', so it cannot be a path
// segment. The bare /admin/models collection is exact-matched in router.match().

pub fn matchAdminModel(p: Path) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, SEG_ADMIN) or !p.eq(1, SEG_MODELS)) return null;
    return p.param(2);
}

// ── /api-keys/{id} ─────────────────────────────────────────────────────────

pub fn matchTenantApiKeyById(p: Path) ?[]const u8 {
    if (p.segs.len != 2) return null;
    if (!p.eq(0, "api-keys")) return null;
    return p.param(1);
}

// ── /tenants/me/models/{id} ─────────────────────────────────────────────────
// The bare /tenants/me/models collection is exact-matched in router.match().

pub fn matchTenantModelEntryById(p: Path) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, SEG_TENANTS) or !p.eq(1, SEG_ME) or !p.eq(2, SEG_MODELS)) return null;
    return p.param(3);
}

// ── /tenants/me/billing/charges/{event_id}/telemetry ───────────────────────
// The per-renewal slice breakdown behind one charge; returns the event_id.

pub fn matchTenantMeteringPeriods(p: Path) ?[]const u8 {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, SEG_TENANTS) or !p.eq(1, SEG_ME) or !p.eq(2, SEG_BILLING) or
        !p.eq(3, SEG_CHARGES) or !p.eq(5, SEG_TELEMETRY)) return null;
    return p.param(4);
}
