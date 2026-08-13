// Tenant + admin billing route matchers — split out of route_matchers.zig to
// keep that file within the 350-line limit (RULE FLL). Operates on the same
// canonical `Path` view; re-exported from route_matchers.zig so call sites stay
// unchanged.

const matchers = @import("route_matchers.zig");
const Path = matchers.Path;

const SEG_TENANTS = "tenants";
const SEG_ME = "me";
const SEG_ADMIN = "admin";
const SEG_MODELS = "models";
const SEG_FLEET_LIBRARIES = "fleet-libraries";
const SEG_PLATFORM_KEYS = "platform-keys";
const SEG_API_KEYS = "api-keys";

/// `/admin/{collection}/{param}` — the shared three-segment admin item shape.
fn matchAdminCollectionItem(p: Path, collection: []const u8) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, SEG_ADMIN) or !p.eq(1, collection)) return null;
    return p.param(2);
}

// ── /admin/platform-keys/{provider} ────────────────────────────────────────

pub fn matchAdminPlatformKey(p: Path) ?[]const u8 {
    return matchAdminCollectionItem(p, SEG_PLATFORM_KEYS);
}

// ── /admin/models/{id} ────────────────────────────────────────────────────
// id (uuidv7) keys the row — model_id can contain '/', so it cannot be a path
// segment. The bare /admin/models collection is exact-matched in router.match().

pub fn matchAdminModel(p: Path) ?[]const u8 {
    return matchAdminCollectionItem(p, SEG_MODELS);
}

// ── /admin/fleet-libraries/{id} ────────────────────────────────────────────
// The catalog id is the bundle's SKILL.md frontmatter name (a slug), not a UUID.

pub fn matchAdminFleetLibrary(p: Path) ?[]const u8 {
    return matchAdminCollectionItem(p, SEG_FLEET_LIBRARIES);
}

// ── /api-keys/{id} ─────────────────────────────────────────────────────────

pub fn matchTenantApiKeyById(p: Path) ?[]const u8 {
    return matchers.matchCollectionItem(p, SEG_API_KEYS);
}

// ── /tenants/me/models/{id} ─────────────────────────────────────────────────
// The bare /tenants/me/models collection is exact-matched in router.match().

pub fn matchTenantModelEntryById(p: Path) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, SEG_TENANTS) or !p.eq(1, SEG_ME) or !p.eq(2, SEG_MODELS)) return null;
    return p.param(3);
}
