//! Whose runners a caller acts on: every runner, as the platform, or one tenant's own.

use af_core::id::Uuid7;

use crate::store::Runners;

/// Who a runner belongs to, and so how far a caller reaches. Built only by
/// [`Runners::platform`] and [`Runners::tenant`], so no caller names the platform
/// by leaving a tenant out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner<'a> {
    /// The platform: registers platform runners and reaches every runner.
    Platform,
    /// One tenant: registers and reaches only its own runners.
    Tenant(&'a Uuid7),
}

impl<'a> Owner<'a> {
    /// The statement's platform flag. A tenant binds false, so its missing tenant
    /// would match no row rather than every row.
    pub(crate) const fn platform(self) -> bool {
        matches!(self, Self::Platform)
    }

    /// The statement's tenant, `None` for the platform.
    pub(crate) const fn tenant(self) -> Option<&'a Uuid7> {
        match self {
            Self::Platform => None,
            Self::Tenant(tenant) => Some(tenant),
        }
    }
}

/// The runner store narrowed to one caller's runners. Each method binds the
/// narrowing into the statement that acts, so ownership and the change are one
/// read, and a runner outside it answers not-found.
#[derive(Clone, Copy)]
pub struct RunnerScope<'a> {
    pub(crate) runners: &'a Runners,
    pub(crate) owner: Owner<'a>,
}

impl std::fmt::Debug for RunnerScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerScope")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

impl Runners {
    /// Every runner, as a platform admin sees them.
    #[must_use]
    pub const fn platform(&self) -> RunnerScope<'_> {
        RunnerScope {
            runners: self,
            owner: Owner::Platform,
        }
    }

    /// Only `tenant`'s own runners.
    #[must_use]
    pub const fn tenant<'a>(&'a self, tenant: &'a Uuid7) -> RunnerScope<'a> {
        RunnerScope {
            runners: self,
            owner: Owner::Tenant(tenant),
        }
    }
}
