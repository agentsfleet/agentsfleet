//! Bounded decision failure observations.

use super::super::{Error, ErrorKind};

impl Error {
    pub(crate) const fn decision_failure_reason(&self) -> Option<&'static str> {
        match self.inner.kind {
            ErrorKind::DecisionRequest { .. } => Some("request_bounds"),
            ErrorKind::DecisionDeadline { .. } => Some("decision_deadline"),
            ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskStorageDeadline { .. }
            | ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteCooldown { .. }
            | ErrorKind::TaskReadTimeout { .. }
            | ErrorKind::TaskStatementCancelled { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskTargetCounter { .. }
            | ErrorKind::TaskControlTurn { .. }
            | ErrorKind::TaskMutation { .. }
            | ErrorKind::TaskControlConflict { .. }
            | ErrorKind::TaskPrecondition { .. }
            | ErrorKind::TaskRefused { .. }
            | ErrorKind::TurnCheckpointRefused { .. }
            | ErrorKind::TurnCheckpointFailure { .. }
            | ErrorKind::Context { .. }
            | ErrorKind::Setup { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Runner { .. }
            | ErrorKind::Gate { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Billing { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Events { .. }
            | ErrorKind::Outbound { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::ConfigUnreadable { .. }
            | ErrorKind::SetupUnavailable
            | ErrorKind::VaultDataInvalid
            | ErrorKind::SequenceCorrupt
            | ErrorKind::PostureCorrupt
            | ErrorKind::StaleFence
            | ErrorKind::RunNotFound
            | ErrorKind::RunLost
            | ErrorKind::RunCancelled
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            | ErrorKind::MessageNoChannel
            | ErrorKind::Scrub { .. }
            | ErrorKind::IntegrationNotConnected
            | ErrorKind::MintUnconfigured
            | ErrorKind::GithubReconnectRequired
            | ErrorKind::GithubMintFailed
            | ErrorKind::ConnectorReconnectRequired
            | ErrorKind::ConnectorMintFailed
            | ErrorKind::GrantRequired
            | ErrorKind::Memory { .. } => None,
        }
    }
}
