//! Existing native facilities shared by task calls and ordinary fleet runs.

use std::sync::Arc;

use afd_billing::biller::Biller;
use afd_credential::provider::Providers;

use crate::run::RunService;

/// Handles only; the task lifecycle owns behavior and introduces no dispatcher.
#[derive(Debug, Clone)]
pub struct DurableTaskDependencies {
    /// Validated limits for new acceptances; saved task allowances remain authoritative.
    pub policy: super::policy::DurableTaskPolicy,
    /// Deployment-owned Secrets workspace, never selected by fleet/model input.
    pub admin_workspace: Option<af_core::id::Uuid7>,
    /// The same payer funds check as fleet runs; logging a line charges it.
    pub biller: Arc<dyn Biller>,
    /// Fresh platform-admin provider-secret resolution.
    pub providers: Providers,
    /// Native branch publication using the same credential plane and guarded client.
    pub github: Option<super::Github>,
    /// Private platform-owned checkpoint storage, absent until activation is configured.
    pub storage: Option<super::storage::R2CheckpointStore>,
}

impl From<&RunService> for DurableTaskDependencies {
    fn from(plane: &RunService) -> Self {
        Self {
            policy: super::policy::DurableTaskPolicy::default(),
            admin_workspace: None,
            biller: Arc::clone(&plane.biller),
            providers: plane.providers.clone(),
            github: None,
            storage: None,
        }
    }
}
