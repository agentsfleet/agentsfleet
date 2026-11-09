//! Safe caller-facing sentences for every native failure.

use super::super::{
    DETAIL_BUDGET_EXHAUSTED, DETAIL_CONFIG_UNREADABLE, DETAIL_CONNECTOR_MINT_FAILED,
    DETAIL_CONNECTOR_RECONNECT, DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE,
    DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED, DETAIL_INTEGRATION_NOT_CONNECTED,
    DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND, DETAIL_MESSAGE_LIMIT,
    DETAIL_MESSAGE_NO_CHANNEL, DETAIL_MINT_FAILED, DETAIL_MINT_UNCONFIGURED,
    DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED, DETAIL_RENEWAL_NO_CREDITS,
    DETAIL_STALE_FENCE, DETAIL_VAULT_DATA_INVALID, Error, ErrorKind,
};

impl Error {
    /// The sentence the caller is told.
    ///
    /// A rejection quotes its own detail, because the caller can act on it —
    /// that is the whole reason the kind exists. Every other kind answers a
    /// FIXED sentence declared in `detail.rs`: an internal failure that quotes
    /// its cause is an internal failure leaking
    /// its cause to whoever provoked it, and the cause is in the log where an
    /// operator can read it beside the request id.
    ///
    /// Not an `Option`. Every refusal this plane writes carries a detail, and a
    /// `None` would push the choice of what to say into each handler — which is
    /// how two call sites end up describing one failure differently.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        match self.inner.kind {
            ErrorKind::TaskStorage { .. } | ErrorKind::TaskStorageSigning { .. } | ErrorKind::TaskStorageSigningParameters { .. } | ErrorKind::TaskStorageRequest { .. } | ErrorKind::TaskStorageDeadline { .. } => crate::task::TaskRefusal::StorageUnavailable.detail(),
            ErrorKind::TaskRemote { .. } | ErrorKind::TaskRemoteStatus { .. } | ErrorKind::TaskRemoteDeadline { .. } | ErrorKind::TaskRemoteCooldown { .. } => crate::task::TaskRefusal::EffectUncertain.detail(),
            ErrorKind::TaskMutation { ref source, .. } => source.detail(),
            ErrorKind::DecisionRequest { .. } | ErrorKind::DecisionDeadline { .. } => crate::task::TaskRefusal::DecisionUnavailable.detail(),
            // The one kind whose sentence the CALL SITE chose: a rejection
            // names the field it refused. Every other kind's sentence is
            // decided here or by the crate it delegates to, so no handler can
            // describe one failure two ways.
            ErrorKind::Rejected { detail } => detail,
            ErrorKind::TaskPrecondition { .. } => "the task resource version changed",
            ErrorKind::TaskControlConflict { .. } => crate::task::TaskRefusal::StaleState.detail(),
            ErrorKind::TaskRefused { reason } => reason.detail(),
            ErrorKind::TaskTargetCounter { .. } => crate::task::TaskRefusal::InvalidTarget.detail(),
            ErrorKind::Memory { ref source } => source.detail(),
            ErrorKind::Runner { ref source } => source.detail(),
            ErrorKind::Authoring { ref source } => source.detail(),
            ErrorKind::Admission { ref source } | ErrorKind::TaskControlAdmission { ref source, .. } => source.detail(),
            ErrorKind::Datastore { .. } | ErrorKind::TaskReadTimeout { .. } | ErrorKind::TaskStatementCancelled { .. } => DETAIL_DATABASE_UNAVAILABLE,
            // A corrupt sequence joins the two row faults: all three are the
            // database holding something this daemon cannot use, and a caller
            // is told the same thing because there is nothing it can do about
            // any of them.
            ErrorKind::Context { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Events { .. }
            // The obligation is a PostgreSQL row, so a ledger fault reads as a
            // database error and not a queue one. `afd_outbound::Error` can
            // also carry a QUEUE failure, but not on any path that reaches
            // here: this crate calls only `obligation::owe` and
            // `obligation::receipt`, and neither touches the stream.
            | ErrorKind::Outbound { .. }
            | ErrorKind::SequenceCorrupt => DETAIL_DATABASE_ERROR,
            ErrorKind::Queue { .. } => DETAIL_QUEUE_UNAVAILABLE,
            ErrorKind::Billing { ref source } => source.detail(),
            ErrorKind::Credential { ref source } => source.detail(),
            ErrorKind::Gate { ref source } => source.detail(),
            ErrorKind::Mint { .. } | ErrorKind::Entropy { .. } => DETAIL_REGISTRATION_FAILED,
            ErrorKind::Scrub { .. } => af_core::error::DETAIL_OPERATION_FAILED,
            ErrorKind::VaultDataInvalid => DETAIL_VAULT_DATA_INVALID,
            ErrorKind::ConfigUnreadable { .. } => DETAIL_CONFIG_UNREADABLE,
            ErrorKind::SetupUnavailable => "The fleet needs a reviewed native setup before it can run",
            ErrorKind::StaleFence => DETAIL_STALE_FENCE,
            ErrorKind::LeaseNotFound => DETAIL_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => DETAIL_LEASE_LOST,
            ErrorKind::LeaseCancelled => super::super::DETAIL_LEASE_CANCELLED,
            ErrorKind::LeaseMaxRuntime => DETAIL_LEASE_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => DETAIL_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => DETAIL_BUDGET_EXHAUSTED,
            ErrorKind::MessageNoChannel => DETAIL_MESSAGE_NO_CHANNEL,
            ErrorKind::MessageLimitReached => DETAIL_MESSAGE_LIMIT,
            ErrorKind::IntegrationNotConnected => DETAIL_INTEGRATION_NOT_CONNECTED,
            ErrorKind::MintUnconfigured => DETAIL_MINT_UNCONFIGURED,
            ErrorKind::GithubReconnectRequired => DETAIL_GITHUB_RECONNECT,
            ErrorKind::GithubMintFailed => DETAIL_MINT_FAILED,
            ErrorKind::ConnectorReconnectRequired => DETAIL_CONNECTOR_RECONNECT,
            ErrorKind::ConnectorMintFailed => DETAIL_CONNECTOR_MINT_FAILED,
            ErrorKind::GrantRequired => DETAIL_GRANT_REQUIRED,
        }
    }
}
