const onboard = @import("onboard.zig");
const catalog = @import("catalog.zig");

pub const innerPlatformOnboard = onboard.innerPlatformOnboard;
pub const innerTenantOnboard = onboard.innerTenantOnboard;
pub const innerGallery = @import("gallery.zig").innerGallery;

// The operator catalog (M128) — the read, curate, publish, and delete arms of
// the same resource `onboard.zig` writes.
pub const innerAdminCatalogList = catalog.innerAdminCatalogList;
pub const innerAdminCatalogPatch = catalog.innerAdminCatalogPatch;
pub const innerAdminCatalogDelete = catalog.innerAdminCatalogDelete;
