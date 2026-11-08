//! Destination scope for HTTP tools, independent of sandbox networking.

use serde::{Deserialize, Serialize};

/// Which public HTTPS destinations HTTP tools may reach.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpEgress {
    /// Reach only additional allowed domains and the optional package registry preset.
    #[default]
    Restricted,
    /// Reach public destinations, subject to address, credential and request restrictions.
    Public,
}

/// Package registry hosts shared by the HTTP preset and the runner's default registry baseline.
/// Each is exact; subdomains and registry redirects require their own admission.
pub const PACKAGE_MANAGER_DOMAINS: [&str; 8] = [
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "static.crates.io",
    "crates.io",
    "index.crates.io",
    "proxy.golang.org",
    "sum.golang.org",
];
