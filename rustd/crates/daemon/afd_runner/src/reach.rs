//! How far a caller reaches into runners: every runner, or one tenant's.

use af_core::id::Uuid7;

/// The runners a caller may see and change.
///
/// Bound into each runner statement: `Platform` binds NULL and reaches every
/// runner, `Tenant` binds its id and reaches only the runners it registered. A
/// tenant's reach always carries its id, so a missing tenant cannot widen into a
/// full listing the way an optional query filter would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// A platform admin: every runner.
    Platform,
    /// One tenant: only its own runners.
    Tenant(Uuid7),
}

impl Reach {
    /// The tenant this reach binds, or `None` for the platform.
    #[must_use]
    pub const fn tenant(&self) -> Option<&Uuid7> {
        match self {
            Self::Platform => None,
            Self::Tenant(tenant) => Some(tenant),
        }
    }
}
