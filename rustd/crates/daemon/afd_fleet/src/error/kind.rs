//! Private causal failures; the parent generates the one public error shell.

use super::Error;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("a saved usage count cannot be represented as a nonnegative metric")]
    UsageCount {
        #[source]
        source: std::num::TryFromIntError,
    },

    #[error("private checkpoint upload authorization could not be signed")]
    TaskStorageSigning {
        #[source]
        source: aws_sigv4::http_request::SigningError,
    },
    #[error("private checkpoint upload signing parameters are unavailable")]
    TaskStorageSigningParameters {
        #[source]
        source: aws_sigv4::sign::v4::signing_params::BuildError,
    },
    #[error("private checkpoint upload request could not be constructed")]
    TaskStorageRequest {
        #[source]
        source: http::Error,
    },

    #[error("private checkpoint storage could not establish object custody")]
    TaskStorage {
        #[source]
        source: object_store::Error,
    },
    #[error("the checkpoint storage operation exceeded its absolute deadline")]
    TaskStorageDeadline {
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("the native GitHub request did not produce a confirmed receipt")]
    TaskRemote {
        #[source]
        source: reqwest::Error,
    },
    #[error("the native GitHub request returned status {status}")]
    TaskRemoteStatus { status: u16 },
    #[error("the native GitHub read exceeded its absolute deadline")]
    TaskRemoteDeadline {
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("GitHub requested a cooldown after status {status}")]
    TaskRemoteCooldown { status: u16, until: jiff::Timestamp },
    #[error("the complete decision request cannot be represented within its reviewed bounds")]
    DecisionRequest {
        #[source]
        source: garde::Report,
    },
    #[error("the native decision operation exceeded its deadline")]
    DecisionDeadline {
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("task progress could not be read before its deadline")]
    TaskReadTimeout {
        #[source]
        source: tokio::time::error::Elapsed,
    },
    #[error("the database cancelled an overdue task progress read")]
    TaskStatementCancelled {
        #[source]
        source: sqlx::Error,
    },
    #[error("task continuation admission failed")]
    Admission {
        #[source]
        source: afd_admission::Error,
    },
    #[error("a stored task document is malformed")]
    TaskDocument {
        #[source]
        source: af_core::json::Unreadable,
    },
    #[error("a stored task lifecycle state is unknown")]
    TaskState {
        #[source]
        source: serde::de::value::Error,
    },
    #[error("task counter {column} cannot be represented")]
    TaskCounter {
        column: &'static str,
        #[source]
        source: std::num::TryFromIntError,
    },
    #[error("native task target field {column} cannot be represented")]
    TaskTargetCounter {
        column: &'static str,
        #[source]
        source: std::num::TryFromIntError,
    },
    #[error("the task continuation could not be admitted")]
    TaskControlAdmission {
        state: af_wire::task::TaskState,
        #[source]
        source: afd_admission::Error,
    },
    #[error("the task mutation was refused")]
    TaskMutation {
        state: af_wire::task::TaskState,
        #[source]
        source: Box<Error>,
    },
    #[error("the task generation or lifecycle state changed")]
    TaskControlConflict { state: af_wire::task::TaskState },
    #[error("the task resource version changed")]
    TaskPrecondition { current_etag: String },
    #[error("{reason}")]
    TaskRefused { reason: crate::task::TaskRefusal },
    #[error("the private working context could not be accessed")]
    Context {
        #[source]
        source: af_context::Error,
    },
    #[error("private authoring admission could not complete")]
    Authoring {
        #[source]
        source: afd_authoring::Error,
    },
    #[error("the datastore backing the runner plane would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("the runner host could not establish lease capacity")]
    Runner {
        #[source]
        source: afd_runner::Error,
    },

    #[error("the gate plane could not answer for the runner plane")]
    Gate {
        #[source]
        source: afd_gate::Error,
    },

    #[error("the credential plane could not answer for the runner plane")]
    Credential {
        #[source]
        source: afd_credential::Error,
    },

    #[error("the queue backing the runner plane would not answer")]
    Queue {
        #[source]
        source: afd_dragonfly::Error,
    },

    #[error("the billing store could not answer for the runner plane")]
    Billing {
        #[source]
        source: afd_billing::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("a {table} row holds a value this daemon cannot read: {column}")]
    RowMalformed {
        table: &'static str,
        column: &'static str,
        #[source]
        source: af_core::error::Error,
    },

    #[error("the event store could not answer for the runner plane")]
    Events {
        #[source]
        source: afd_events::Error,
    },

    /// The delivery ledger would not answer.
    ///
    /// `afd_outbound` owns `fleet.obligations`, and the report path commits
    /// an obligation through it inside its own transaction — so a failure there
    /// is a failure of THIS transaction and has to reach the caller as one.
    /// Distinct from [`Self::Datastore`] because the obligation is a PostgreSQL
    /// row, not a queue entry, and the two fail for different reasons.
    #[error("the delivery ledger could not answer for the runner plane")]
    Outbound {
        #[source]
        source: afd_outbound::Error,
    },

    #[error("{detail}")]
    Rejected { detail: &'static str },

    #[error("an identifier could not be minted from the current instant")]
    Mint {
        #[from]
        source: af_core::error::Error,
    },

    #[error("could not draw the entropy a credential is minted from")]
    Entropy {
        #[source]
        source: af_crypto::error::Error,
    },

    #[error("the fleet's stored configuration cannot be read")]
    ConfigUnreadable {
        #[from]
        source: af_fleet_runtime::Error,
    },

    #[error("the fleet needs a reviewed native setup before it can run")]
    SetupUnavailable,

    #[error("a stored credential body is not the JSON object the tool bridge addresses")]
    VaultDataInvalid,

    #[error("a stored native sequence is outside its permitted range")]
    SequenceCorrupt,

    #[error("the reporting holder's fence is behind the fleet's live sequence")]
    StaleFence,

    #[error("no lease with that id belongs to the presenting runner")]
    LeaseNotFound,

    #[error("the lease is no longer this runner's to renew")]
    LeaseLost,

    #[error("the fleet was killed and this lease must stop")]
    LeaseCancelled,

    #[error("the lease reached the hard ceiling on one run's wall time")]
    LeaseMaxRuntime,

    #[error("the tenant's credit pool cannot fund another slice of this run")]
    RenewalNoCredits,

    #[error("the fleet reached a spend ceiling its own author declared")]
    BudgetExhausted,

    #[error("the leased event recorded no thread a message can go to")]
    MessageNoChannel,

    #[error("the lease already posted as many messages as one run may")]
    MessageLimitReached,

    #[error("the fleet's secret values could not be gathered into a mask")]
    Scrub {
        #[source]
        source: af_secrets::Error,
    },

    #[error("this workspace has connected no integration under that name")]
    IntegrationNotConnected,

    #[error("this deployment holds no platform credential for that connector")]
    MintUnconfigured,

    #[error("the GitHub App installation this handle names is gone")]
    GithubReconnectRequired,

    #[error("GitHub returned no installation token this daemon would hand over")]
    GithubMintFailed,

    #[error("the connector's stored authorization is no longer redeemable")]
    ConnectorReconnectRequired,

    #[error("the connector's token exchange produced no credential")]
    ConnectorMintFailed,

    #[error("the fleet holds no approved grant for that integration")]
    GrantRequired,

    #[error("the fleet's memory could not be read or written")]
    Memory {
        #[source]
        source: afd_memory::Error,
    },
}
