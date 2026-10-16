//! Shared-memory permissions remain independent from immutable fleet setup.

use serde::{Deserialize, Serialize};

/// Presence-based memory permission changes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryAccessRequest {
    /// Read shared workspace memory.
    #[serde(default)]
    pub read: Option<bool>,
    /// Publish workspace memory.
    #[serde(default)]
    pub publish: Option<bool>,
}

/// Current shared-memory permissions.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryAccess {
    /// Read permission.
    pub read: bool,
    /// Publish permission.
    pub publish: bool,
}
