//! Decision recovery and bounded failure observations.

use super::super::{Error, ErrorKind};

impl Error {
    pub(crate) fn is_decision_lock_busy(&self) -> bool {
        let ErrorKind::TaskStatementCancelled { source } = &self.inner.kind else {
            return false;
        };
        source
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref()
            == Some("55P03")
    }
    pub(crate) fn is_decision_recovery_outage(&self) -> bool {
        self.is_datastore_unavailable()
            || matches!(&self.inner.kind,
            ErrorKind::Query { source, .. } if afd_db::error::is_connection_lost(source))
    }
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
            | ErrorKind::TaskRemoteDeadline { .. }
            | ErrorKind::TaskRemoteCooldown { .. }
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
}
