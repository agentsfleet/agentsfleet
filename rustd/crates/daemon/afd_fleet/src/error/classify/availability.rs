//! Availability, deadlines and retry classification.

use super::super::{Error, ErrorKind};

impl Error {
    /// Whether the datastore could not be reached, or gave up waiting past the
    /// API login's lock or statement bound (`afd_db::login`).
    ///
    /// The question the runner plane turns on: a caller answering this `true`
    /// must report a transport failure, never an authentication or a validation
    /// one, because the runner client counts rejections toward a
    /// self-termination ceiling and resets that counter on transport failures
    /// (RULE ECL, and `docs/AUTH.md` §Runner token). A wrapped plane's error
    /// decides for itself whether a reached bound is one, and this table asks
    /// it: the runner host's capacity lock is where a run verb waits.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.inner.kind {
            ErrorKind::Datastore { .. }
            | ErrorKind::DurableTaskReadTimeout { .. }
            | ErrorKind::DurableTaskStatementCancelled { .. } => true,
            ErrorKind::DurableTaskControlTurn { ref source, .. } => {
                source.is_datastore_unavailable()
            }
            ErrorKind::Runner { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Setup { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Memory { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Approval { ref source } => source.is_datastore_unavailable(),
            ErrorKind::DurableTaskStorageSigning { .. }
            | ErrorKind::DurableTaskStorageSigningParameters { .. }
            | ErrorKind::DurableTaskStorageRequest { .. }
            | ErrorKind::DurableTaskStorage { .. }
            | ErrorKind::DurableTaskRemote { .. }
            | ErrorKind::DurableTaskRemoteStatus { .. }
            | ErrorKind::DurableTaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::DurableTaskDocument { .. }
            | ErrorKind::DurableTaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::DurableTaskCounter { .. }
            | ErrorKind::DurableTaskControlConflict { .. }
            | ErrorKind::DurableTaskPrecondition { .. }
            | ErrorKind::DurableTaskRefused { .. }
            | ErrorKind::FleetRefused { .. }
            | ErrorKind::TurnCheckpointRefused { .. }
            | ErrorKind::TurnCheckpointFailure { .. }
            | ErrorKind::Context { .. }
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
            | ErrorKind::FleetRowUnreadable
            | ErrorKind::VaultDataInvalid
            | ErrorKind::SequenceCorrupt
            | ErrorKind::PostureCorrupt
            | ErrorKind::StaleFence
            | ErrorKind::RunNotFound
            | ErrorKind::RunLost
            | ErrorKind::RunCancelled
            | ErrorKind::RenewalNoCredits
            | ErrorKind::MessageNoChannel
            | ErrorKind::Scrub { .. }
            | ErrorKind::IntegrationNotConnected
            | ErrorKind::MintUnconfigured
            | ErrorKind::GithubReconnectRequired
            | ErrorKind::GithubMintFailed
            | ErrorKind::ConnectorReconnectRequired
            | ErrorKind::ConnectorMintFailed
            | ErrorKind::GrantRequired => false,
        }
    }
    /// Bounded diagnostic for task-read performance, without query text.
    pub(crate) fn durable_task_timeout_reason(&self) -> Option<&'static str> {
        match self.inner.kind {
            ErrorKind::DurableTaskReadTimeout { .. } => Some("read_deadline"),
            ErrorKind::DurableTaskStatementCancelled { .. } => Some("statement_deadline"),
            ErrorKind::DurableTaskRemote { ref source } if source.is_timeout() => {
                Some("remote_deadline")
            }
            ErrorKind::DurableTaskStorageSigning { .. }
            | ErrorKind::DurableTaskStorageSigningParameters { .. }
            | ErrorKind::DurableTaskStorageRequest { .. }
            | ErrorKind::DurableTaskStorage { .. }
            | ErrorKind::DurableTaskRemote { .. }
            | ErrorKind::DurableTaskRemoteStatus { .. }
            | ErrorKind::DurableTaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::TurnWrite { .. }
            | ErrorKind::DurableTaskDocument { .. }
            | ErrorKind::DurableTaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::DurableTaskCounter { .. }
            | ErrorKind::DurableTaskControlTurn { .. }
            | ErrorKind::DurableTaskControlConflict { .. }
            | ErrorKind::DurableTaskPrecondition { .. }
            | ErrorKind::DurableTaskRefused { .. }
            | ErrorKind::FleetRefused { .. }
            | ErrorKind::TurnCheckpointRefused { .. }
            | ErrorKind::TurnCheckpointFailure { .. }
            | ErrorKind::Context { .. }
            | ErrorKind::Setup { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Runner { .. }
            | ErrorKind::Gate { .. }
            | ErrorKind::Approval { .. }
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
            | ErrorKind::FleetRowUnreadable
            | ErrorKind::VaultDataInvalid
            | ErrorKind::SequenceCorrupt
            | ErrorKind::PostureCorrupt
            | ErrorKind::StaleFence
            | ErrorKind::RunNotFound
            | ErrorKind::RunLost
            | ErrorKind::RunCancelled
            | ErrorKind::RenewalNoCredits
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
    /// A provider cooldown is a native deadline, never an immediate retry hint.
    pub(crate) const fn remote_cooldown(&self) -> Option<jiff::Timestamp> {
        match self.inner.kind {
            ErrorKind::DurableTaskRemoteCooldown { until, .. } => Some(until),
            ErrorKind::DurableTaskStorageSigning { .. }
            | ErrorKind::DurableTaskStorageSigningParameters { .. }
            | ErrorKind::DurableTaskStorageRequest { .. }
            | ErrorKind::DurableTaskStorage { .. }
            | ErrorKind::DurableTaskRemote { .. }
            | ErrorKind::DurableTaskRemoteStatus { .. }
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
            | ErrorKind::DurableTaskControlConflict { .. }
            | ErrorKind::DurableTaskPrecondition { .. }
            | ErrorKind::DurableTaskRefused { .. }
            | ErrorKind::FleetRefused { .. }
            | ErrorKind::TurnCheckpointRefused { .. }
            | ErrorKind::TurnCheckpointFailure { .. }
            | ErrorKind::Context { .. }
            | ErrorKind::Setup { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Runner { .. }
            | ErrorKind::Gate { .. }
            | ErrorKind::Approval { .. }
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
            | ErrorKind::FleetRowUnreadable
            | ErrorKind::VaultDataInvalid
            | ErrorKind::SequenceCorrupt
            | ErrorKind::PostureCorrupt
            | ErrorKind::StaleFence
            | ErrorKind::RunNotFound
            | ErrorKind::RunLost
            | ErrorKind::RunCancelled
            | ErrorKind::RenewalNoCredits
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
    /// Whether the caller sent something this plane will not accept.
    ///
    /// The only kind whose message is written FOR the caller; every other kind
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Rejected { .. })
    }
    /// Whether a run verb was refused because the fleet moved past the
    /// run. The fence logs its own event for it, so a caller that goes on
    /// without the write logs nothing more.
    #[must_use]
    pub(crate) fn is_fence_refusal(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::StaleFence)
    }
}
