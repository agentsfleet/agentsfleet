//! Task state and resource tags retain their native kinds.

use super::super::{Error, ErrorKind};

impl Error {
    /// Lifecycle state observed under the lock that refused a task control.
    #[must_use]
    pub const fn task_current_state(&self) -> Option<&'static str> {
        match &self.inner.kind {
            ErrorKind::TaskControlConflict { state }
            | ErrorKind::TaskControlAdmission { state, .. }
            | ErrorKind::TaskMutation { state, .. } => Some(state.as_str()),
            ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskStorageDeadline { .. }
            | ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteDeadline { .. }
            | ErrorKind::TaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::TaskReadTimeout { .. }
            | ErrorKind::TaskStatementCancelled { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskTargetCounter { .. }
            | ErrorKind::TaskPrecondition { .. }
            | ErrorKind::TaskRefused { .. }
            | ErrorKind::Context { .. }
            | ErrorKind::Authoring { .. }
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
            | ErrorKind::StaleFence
            | ErrorKind::LeaseNotFound
            | ErrorKind::LeaseLost
            | ErrorKind::LeaseCancelled
            | ErrorKind::LeaseMaxRuntime
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            | ErrorKind::MessageNoChannel
            | ErrorKind::MessageLimitReached
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
    pub fn task_precondition_tag(&self) -> Option<&str> {
        match &self.inner.kind {
            ErrorKind::TaskPrecondition { current_etag } => Some(current_etag),
            ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskStorageDeadline { .. }
            | ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteDeadline { .. }
            | ErrorKind::TaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::TaskReadTimeout { .. }
            | ErrorKind::TaskStatementCancelled { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskTargetCounter { .. }
            | ErrorKind::TaskControlAdmission { .. }
            | ErrorKind::TaskMutation { .. }
            | ErrorKind::TaskControlConflict { .. }
            | ErrorKind::TaskRefused { .. }
            | ErrorKind::Context { .. }
            | ErrorKind::Authoring { .. }
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
            | ErrorKind::StaleFence
            | ErrorKind::LeaseNotFound
            | ErrorKind::LeaseLost
            | ErrorKind::LeaseCancelled
            | ErrorKind::LeaseMaxRuntime
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            | ErrorKind::MessageNoChannel
            | ErrorKind::MessageLimitReached
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
