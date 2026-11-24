//! Why a run's request to change what its fleet holds was refused.
//!
//! Fieldless, as the task refusal vocabulary is: each is a normal guarded
//! answer the runner turns into `[kind] sentence` for its model, which may
//! correct the call in the same run.

use std::fmt;

use af_core::error_code::{self, ErrorCode};

use crate::error::{Error, ErrorKind};

/// Why an access request or a trigger change was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetRefusal {
    /// The workspace vault holds no secret by the name the run asked for.
    AccessNameNotFound,
    /// The workspace has not connected the integration or provider asked for.
    ProviderNotConnected,
    /// The fleet already holds a trigger of that kind for that source.
    TriggerExists,
    /// The workspace vault holds no signing secret by the name a webhook names.
    TriggerSecretNotFound,
}

impl FleetRefusal {
    /// The fixed public code for this refusal.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::AccessNameNotFound => error_code::FLEET_ACCESS_NAME_NOT_FOUND,
            Self::ProviderNotConnected => error_code::FLEET_PROVIDER_NOT_CONNECTED,
            Self::TriggerExists => error_code::FLEET_TRIGGER_EXISTS,
            Self::TriggerSecretNotFound => error_code::FLEET_TRIGGER_SECRET_NOT_FOUND,
        }
    }

    /// A sentence the run's model reads, naming no secret value.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::AccessNameNotFound => "the workspace holds no secret by that name",
            Self::ProviderNotConnected => "the workspace has not connected that integration",
            Self::TriggerExists => "the fleet already has a trigger for that source",
            Self::TriggerSecretNotFound => "the workspace holds no signing secret by that name",
        }
    }
}

impl fmt::Display for FleetRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.detail())
    }
}

impl From<FleetRefusal> for Error {
    fn from(reason: FleetRefusal) -> Self {
        Self::from(ErrorKind::FleetRefused { reason })
    }
}

#[cfg(test)]
mod tests {
    use super::FleetRefusal;

    #[test]
    fn each_refusal_carries_its_own_code_and_a_sentence() {
        let every = [
            FleetRefusal::AccessNameNotFound,
            FleetRefusal::ProviderNotConnected,
            FleetRefusal::TriggerExists,
            FleetRefusal::TriggerSecretNotFound,
        ];
        let codes: std::collections::BTreeSet<_> = every
            .iter()
            .map(|refusal| refusal.code().as_str())
            .collect();

        assert_eq!(codes.len(), every.len(), "{codes:?}");
        for refusal in every {
            let error = crate::Error::from(refusal);
            assert_eq!(error.code(), refusal.code());
            assert_eq!(error.detail(), refusal.detail());
            assert!(!error.is_datastore_unavailable());
        }
    }
}
