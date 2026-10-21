//! What a failure MEANS to a caller, as three exhaustive tables.
//!
//! Split from [`super`] at the seam [`super::detail`] already names: that
//! module is what we SAY, this one is what we DECIDE, and the enum next door is
//! what happened. Every function here is a total match over
//! `ErrorKind`, so a new kind fails the build until someone
//! has answered all three questions about it — which code a client matches on,
//! which sentence it reads, and whether the work can ever run again.
//!
//! That totality is the whole reason they are tables rather than `if` ladders.
//! A ladder restates the pairing at every call site and classifies permanence
//! by the ORDER of its arms, so both facts live wherever someone last wrote
//! them down.

use afd_core::error_code::{self, ErrorCode};

use super::{
    DETAIL_BUDGET_EXHAUSTED, DETAIL_CONFIG_UNREADABLE, DETAIL_CONNECTOR_MINT_FAILED,
    DETAIL_CONNECTOR_RECONNECT, DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE,
    DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED, DETAIL_INTEGRATION_NOT_CONNECTED,
    DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND, DETAIL_MESSAGE_LIMIT,
    DETAIL_MESSAGE_NO_CHANNEL, DETAIL_MINT_FAILED, DETAIL_MINT_UNCONFIGURED,
    DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED, DETAIL_RENEWAL_NO_CREDITS,
    DETAIL_STALE_FENCE, DETAIL_VAULT_DATA_INVALID, Error, ErrorKind,
};

impl Error {
    /// Whether the datastore could not be reached at all.
    ///
    /// The question the runner plane turns on: a caller answering this `true`
    /// must report a transport failure, never an authentication or a validation
    /// one, because the runner client counts rejections toward a
    /// self-termination ceiling and resets that counter on transport failures
    /// (RULE ECL, and `docs/AUTH.md` §Runner token).
    #[must_use]
    pub fn is_datastore_unavailable(&self) -> bool {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => true,
            ErrorKind::Runner { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Authoring { ref source } => source.is_datastore_unavailable(),
            ErrorKind::Memory { ref source } => source.is_datastore_unavailable(),
            _ => false,
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
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            ErrorKind::Billing { ref source } => source.code(),
            ErrorKind::Runner { ref source } => source.code(),
            ErrorKind::Authoring { ref source } => source.code(),
            ErrorKind::Credential { ref source } => source.code(),
            ErrorKind::Gate { ref source } => source.code(),
            ErrorKind::Events { ref source } => source.code(),
            ErrorKind::Outbound { ref source } => source.code(),
            ErrorKind::Memory { ref source } => source.code(),
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            ErrorKind::SetupUnavailable => error_code::AUTHORING_POLICY,
            ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::ConfigUnreadable { .. }
            | ErrorKind::Scrub { .. } => error_code::INTERNAL_OPERATION_FAILED,
            ErrorKind::VaultDataInvalid => error_code::VAULT_DATA_INVALID,
            ErrorKind::StaleFence => error_code::RUN_STALE_FENCING_TOKEN,
            ErrorKind::LeaseNotFound => error_code::RUN_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => error_code::RUN_LEASE_LOST,
            ErrorKind::LeaseMaxRuntime => error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => error_code::RUN_BUDGET_EXCEEDED,
            ErrorKind::MessageNoChannel => error_code::MESSAGE_NO_CHANNEL,
            ErrorKind::MessageLimitReached => error_code::MESSAGE_LIMIT_REACHED,
            ErrorKind::SequenceCorrupt => error_code::INTERNAL_DB_QUERY,
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
            // The one kind whose sentence the CALL SITE chose: a rejection
            // names the field it refused. Every other kind's sentence is
            // decided here or by the crate it delegates to, so no handler can
            // describe one failure two ways.
            ErrorKind::Rejected { detail } => detail,
            ErrorKind::Memory { ref source } => source.detail(),
            ErrorKind::Runner { ref source } => source.detail(),
            ErrorKind::Authoring { ref source } => source.detail(),
            ErrorKind::Datastore { .. } => DETAIL_DATABASE_UNAVAILABLE,
            // A corrupt sequence joins the two row faults: all three are the
            // database holding something this daemon cannot use, and a caller
            // is told the same thing because there is nothing it can do about
            // any of them.
            ErrorKind::Query { .. }
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
            ErrorKind::Scrub { .. } => afd_core::error::DETAIL_OPERATION_FAILED,
            ErrorKind::VaultDataInvalid => DETAIL_VAULT_DATA_INVALID,
            ErrorKind::ConfigUnreadable { .. } => DETAIL_CONFIG_UNREADABLE,
            ErrorKind::SetupUnavailable => "The fleet needs a reviewed native setup before it can run",
            ErrorKind::StaleFence => DETAIL_STALE_FENCE,
            ErrorKind::LeaseNotFound => DETAIL_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => DETAIL_LEASE_LOST,
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
