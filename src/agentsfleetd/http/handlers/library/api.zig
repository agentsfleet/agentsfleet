const onboard = @import("onboard.zig");
const catalog = @import("catalog.zig");
const catalog_patch = @import("catalog_patch.zig");

pub const innerPlatformOnboard = onboard.innerPlatformOnboard;
pub const innerTenantOnboard = onboard.innerTenantOnboard;
pub const innerGallery = @import("gallery.zig").innerGallery;

// The operator catalog (M128) — the read, curate, publish, and delete arms of
// the same resource `onboard.zig` writes.
pub const innerAdminCatalogList = catalog.innerAdminCatalogList;
// The write lives in its own module: M130 widened it past catalog.zig's length
// cap (RULE FLL), and it is the arm that carries the bundle-invalidation guard.
pub const innerAdminCatalogPatch = catalog_patch.innerAdminCatalogPatch;
pub const innerAdminCatalogDelete = catalog.innerAdminCatalogDelete;
