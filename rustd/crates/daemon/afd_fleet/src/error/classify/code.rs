//! One registry code for every native failure.

use af_core::error_code::{self, ErrorCode};

use super::super::{Error, ErrorKind};

impl Error {
    /// The registry code this failure answers with.
    ///
    /// Exhaustive, so a new kind fails the build until it is given one — the
    /// same device `afd_auth::Error::code` uses. The arms group by family, and
    /// each family's reason is written here rather than between the arms, so
    /// the table reads as one table:
    ///
    /// - **Delegated** — billing, runner, authoring, credential,
    ///   gate, events, outbound and memory each already decide which of their
    ///   failures is an outage and which is their own fault. A second copy of
    ///   that mapping here is the drift this crate's module header warns about.
    /// - **This process failing** — a clock that cannot name an instant, a
    ///   host that cannot draw random bytes, a queue that will not answer, a
    ///   mask that cannot be built, and a producer writing an entry this daemon
    ///   cannot execute are not the caller's request being wrong. An earlier
    ///   draft answered `Mint` with `UUIDV7_INVALID_ID_SHAPE`, a 400 that told
    ///   an operator their enrolment was malformed while the fault was here. A
    ///   stored config this daemon cannot read joins them: the finer code an
    ///   operator would want does not exist in a registry this family does not
    ///   own, and the parser's own error survives in the source chain.
    /// - **Vault body** — a body that will not parse answers its own code,
    ///   because its SHAPE is a fact the operator who stored it can act on.
    /// - **Lease lifecycle** — six refusals, six codes. They are one fact (this
    ///   runner may not do this to this lease) observed at six moments, and the
    ///   runner acts differently on every one.
    /// - **Messages** — no thread, or the run has spoken its fill. Neither is a
    ///   fault, and the model acts on each differently.
    /// - **Corrupt sequence** — the same code a column that will not parse
    ///   answers: this IS that, found by range rather than by shape.
    /// - **Mint** — GitHub keeps two codes because an App installation has its
    ///   own reconnect semantics, and a runner meeting UZ-GH-001 knows a HUMAN
    ///   must reinstall the App. The refresh connectors share one code with two
    ///   sentences, so a Zoho failure never routes a runner to GitHub's
    ///   reconnect.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::TaskStorage { .. }
            | ErrorKind::TaskStorageSigning { .. }
            | ErrorKind::TaskStorageSigningParameters { .. }
            | ErrorKind::TaskStorageRequest { .. }
            | ErrorKind::TaskStorageDeadline { .. } => error_code::TASK_STORAGE_UNAVAILABLE,
            ErrorKind::TaskRemote { .. }
            | ErrorKind::TaskRemoteStatus { .. }
            | ErrorKind::TaskRemoteDeadline { .. }
            | ErrorKind::TaskRemoteCooldown { .. } => error_code::TASK_EFFECT_UNCERTAIN,
            ErrorKind::DecisionRequest { .. } | ErrorKind::DecisionDeadline { .. } => {
                error_code::TASK_DECISION_UNAVAILABLE
            }
            ErrorKind::Datastore { .. }
            | ErrorKind::TaskReadTimeout { .. }
            | ErrorKind::TaskStatementCancelled { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Billing { ref source } => source.code(),
            ErrorKind::Runner { ref source } => source.code(),
            ErrorKind::Authoring { ref source } => source.code(),
            ErrorKind::Admission { ref source }
            | ErrorKind::TaskControlAdmission { ref source, .. } => source.code(),
            ErrorKind::Context { ref source } => source.code(),
            ErrorKind::Credential { ref source } => source.code(),
            ErrorKind::Gate { ref source } => source.code(),
            ErrorKind::Events { ref source } => source.code(),
            ErrorKind::Outbound { ref source } => source.code(),
            ErrorKind::Memory { ref source } => source.code(),
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::UsageCount { .. }
            | ErrorKind::TaskCounter { .. }
            | ErrorKind::TaskState { .. }
            | ErrorKind::TaskDocument { .. }
            | ErrorKind::SequenceCorrupt => error_code::INTERNAL_DB_QUERY,
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            ErrorKind::TaskPrecondition { .. } => error_code::TASK_PRECONDITION_FAILED,
            ErrorKind::TaskMutation { ref source, .. } => source.code(),
            ErrorKind::TaskControlConflict { .. } => error_code::TASK_STALE_STATE,
            ErrorKind::TaskRefused { reason } => reason.code(),
            ErrorKind::TaskTargetCounter { .. } => error_code::TASK_INVALID_TARGET,
            ErrorKind::SetupUnavailable => error_code::AUTHORING_POLICY,
            ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::ConfigUnreadable { .. }
            | ErrorKind::Scrub { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::VaultDataInvalid => error_code::VAULT_DATA_INVALID,
            ErrorKind::StaleFence => error_code::RUN_STALE_FENCING_TOKEN,
            ErrorKind::LeaseNotFound => error_code::RUN_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => error_code::RUN_LEASE_LOST,
            ErrorKind::LeaseCancelled => error_code::RUN_LEASE_CANCELLED,
            ErrorKind::LeaseMaxRuntime => error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => error_code::RUN_BUDGET_EXCEEDED,
            ErrorKind::MessageNoChannel => error_code::MESSAGE_NO_CHANNEL,
            ErrorKind::MessageLimitReached => error_code::MESSAGE_LIMIT_REACHED,
            ErrorKind::IntegrationNotConnected => error_code::CRED_INTEGRATION_NOT_CONNECTED,
            ErrorKind::MintUnconfigured => error_code::CRED_BROKER_NOT_CONFIGURED,
            ErrorKind::GithubReconnectRequired => error_code::GH_RECONNECT_REQUIRED,
            ErrorKind::GithubMintFailed => error_code::GH_MINT_FAILED,
            ErrorKind::ConnectorReconnectRequired | ErrorKind::ConnectorMintFailed => {
                error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED
            }
            ErrorKind::GrantRequired => error_code::GRANT_NOT_FOUND,
        }
    }
}
