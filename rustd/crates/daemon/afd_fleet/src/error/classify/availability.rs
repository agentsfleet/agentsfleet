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
    /// it: the runner host's capacity lock is where a lease verb waits.
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.inner.kind {
            ErrorKind::Datastore { .. }
            | ErrorKind::TaskReadTimeout { .. }
            | ErrorKind::TaskStatementCancelled { .. } => true,
            ErrorKind::TaskControlAdmission { ref source, .. } => source.is_datastore_unavailable(),
            ErrorKind::TaskMutation { ref source, .. } => source.is_datastore_unavailable(),
            ErrorKind::Runner { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Authoring { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Memory { ref source } => source.is_datastore_unavailable(),
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
            | ErrorKind::Admission { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskTargetCounter { .. }
            | ErrorKind::TaskControlConflict { .. }
            | ErrorKind::TaskPrecondition { .. }
            | ErrorKind::TaskRefused { .. }
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
            | ErrorKind::GrantRequired => false,
        }
    }
    /// Bounded diagnostic for task-read performance, without query text.
    pub(crate) fn task_timeout_reason(&self) -> Option<&'static str> {
        match self.inner.kind {
            ErrorKind::TaskReadTimeout { .. } => Some("read_deadline"),
            ErrorKind::TaskStatementCancelled { .. } => Some("statement_deadline"),
            ErrorKind::TaskStorageDeadline { .. } => Some("storage_deadline"),
            ErrorKind::TaskRemoteDeadline { .. } => Some("remote_deadline"),
            ErrorKind::TaskRemote { ref source } if source.is_timeout() => Some("remote_deadline"),
            ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteCooldown { .. }
            | ErrorKind::DecisionRequest { .. }
            | ErrorKind::DecisionDeadline { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskTargetCounter { .. }
            | ErrorKind::TaskControlAdmission { .. }
            | ErrorKind::TaskMutation { .. }
            | ErrorKind::TaskControlConflict { .. }
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
    /// A provider cooldown is a native deadline, never an immediate retry hint.
    pub(crate) const fn remote_cooldown(&self) -> Option<jiff::Timestamp> {
        match self.inner.kind {
            ErrorKind::TaskRemoteCooldown { until, .. } => Some(until),
            ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskStorageDeadline { .. }
            | ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteDeadline { .. }
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
    /// Whether the caller sent something this plane will not accept.
    ///
    /// The only kind whose message is written FOR the caller; every other kind
    /// answers a fixed registry sentence and keeps its detail in the log.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::Rejected { .. })
    }
    /// Whether a lease verb was refused because the fleet moved past the
    /// lease. The fence logs its own event for it, so a caller that goes on
    /// without the write logs nothing more.
    #[must_use]
    pub(crate) fn is_fence_refusal(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::StaleFence)
    }
}
