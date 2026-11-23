//! Task state and resource tags retain their native kinds.

use super::super::{Error, ErrorKind};

impl Error {
    /// Lifecycle state observed under the lock that refused a task control.
    #[must_use]
    pub const fn durable_task_current_state(&self) -> Option<&'static str> {
        match &self.inner.kind {
            ErrorKind::DurableTaskControlConflict { state }
            | ErrorKind::DurableTaskControlTurn { state, .. }
            | ErrorKind::DurableTaskMutation { state, .. } => Some(state.as_str()),
            ErrorKind::DurableTaskStorageSigning { .. }
            | ErrorKind::DurableTaskStorageSigningParameters { .. }
            | ErrorKind::DurableTaskStorageRequest { .. }
            | ErrorKind::DurableTaskStorage { .. }
            | ErrorKind::DurableTaskRemote { .. }
            | ErrorKind::DurableTaskRemoteStatus { .. }
            | ErrorKind::DurableTaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::DurableTaskReadTimeout { .. }
            | ErrorKind::DurableTaskStatementCancelled { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::DurableTaskDocument { .. }
            | ErrorKind::DurableTaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::DurableTaskCounter { .. }
            | ErrorKind::DurableTaskPrecondition { .. }
            | ErrorKind::DurableTaskRefused { .. }
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
    /// Current resource tag when a conditional task mutation lost its version.
    #[must_use]
    pub fn durable_task_precondition_tag(&self) -> Option<&str> {
        match &self.inner.kind {
            ErrorKind::DurableTaskPrecondition { current_etag } => Some(current_etag),
            ErrorKind::DurableTaskStorageSigning { .. }
            | ErrorKind::DurableTaskStorageSigningParameters { .. }
            | ErrorKind::DurableTaskStorageRequest { .. }
            | ErrorKind::DurableTaskStorage { .. }
            | ErrorKind::DurableTaskRemote { .. }
            | ErrorKind::DurableTaskRemoteStatus { .. }
            | ErrorKind::DurableTaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::DurableTaskReadTimeout { .. }
            | ErrorKind::DurableTaskStatementCancelled { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::DurableTaskDocument { .. }
            | ErrorKind::DurableTaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::DurableTaskCounter { .. }
            | ErrorKind::DurableTaskControlTurn { .. }
            | ErrorKind::DurableTaskMutation { .. }
            | ErrorKind::DurableTaskControlConflict { .. }
            | ErrorKind::DurableTaskRefused { .. }
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
