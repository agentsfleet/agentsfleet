//! The HTTP seam the onboarding checklist acts through.

use af_core::id::Uuid7;
use afd_tenant::Result as TenantResult;
use afd_tenant::onboarding::{Onboarding, Signals};

/// Everything the onboarding route acts through.
pub trait WorkspaceOnboarding: Send + Sync + std::fmt::Debug + 'static {
    /// The five derivable onboarding signals for this workspace.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn signals(
        &self,
        workspace: &Uuid7,
        tenant: &Uuid7,
    ) -> impl Future<Output = TenantResult<Signals>> + Send;
}

/// The production store answers it directly.
impl WorkspaceOnboarding for Onboarding {
    fn signals(
        &self,
        workspace: &Uuid7,
        tenant: &Uuid7,
    ) -> impl Future<Output = TenantResult<Signals>> + Send {
        Self::signals(self, workspace, tenant)
    }
}
