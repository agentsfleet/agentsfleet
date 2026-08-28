//! What a failure MEANS to a caller, as three exhaustive tables.
//!
//! Split from [`super`] at the seam [`super::detail`] already names: that
//! module is what we SAY, this one is what we DECIDE, and the enum next door is
//! what happened. Every function here is a total match over
//! [`ErrorKind`](super::ErrorKind), so a new kind fails the build until someone
//! has answered all three questions about it — which code a client matches on,
//! which sentence it reads, and whether the work can ever run again.
//!
//! That totality is the whole reason they are tables rather than `if` ladders.
//! The Zig restates the pairing at every `hx.fail` call site and classifies
//! permanence by the ORDER of arms in one `switch`, so both facts live wherever
//! someone last wrote them down.

use afd_core::error_code::{self, ErrorCode};

use super::{
    DETAIL_BINDING_DRIFT, DETAIL_BUDGET_EXHAUSTED, DETAIL_BUNDLE_FETCH_FAILED,
    DETAIL_BUNDLE_NOT_FOUND, DETAIL_BUNDLE_STORAGE_UNAVAILABLE, DETAIL_CONFIG_UNREADABLE,
    DETAIL_CONNECTOR_MINT_FAILED, DETAIL_CONNECTOR_RECONNECT,
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_EVENT_MALFORMED,
    DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED, DETAIL_INTEGRATION_NOT_CONNECTED,
    DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND,
    DETAIL_MEMORY_AGENTSFLEET_NOT_FOUND, DETAIL_MEMORY_ENTRY_NOT_FOUND, DETAIL_MINT_FAILED,
    DETAIL_MINT_UNCONFIGURED, DETAIL_QUEUE_UNAVAILABLE,
    DETAIL_REGISTRATION_FAILED, DETAIL_RENEWAL_NO_CREDITS, DETAIL_STALE_FENCE,
    DETAIL_VAULT_DATA_INVALID, DETAIL_WRITE_SPEND_EXHAUSTED, DETAIL_WRITE_UNAPPROVED, Error,
    ErrorKind,
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
        matches!(self.inner.kind, ErrorKind::Datastore { .. })
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
    /// same device `afd_auth::Error::code` uses, applied to the pairing the Zig
    /// handlers restate at every `hx.fail` call site.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.inner.kind {
            ErrorKind::Datastore { .. } => error_code::INTERNAL_DB_UNAVAILABLE,
            // Delegated, not restated: the billing crate already decides which
            // of its failures are an outage and which are a statement fault,
            // and a second copy of that mapping here is the drift this crate's
            // own module header warns about.
            ErrorKind::Billing { ref source } => source.code(),
            // Delegated for the reason Billing is: the credential plane already
            // decides which of its failures is an outage and which is a fault.
            ErrorKind::Credential { ref source } => source.code(),
            ErrorKind::Gate { ref source } => source.code(),
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::Rejected { .. } => error_code::INVALID_REQUEST,
            // A daemon whose clock cannot name an instant, and a host that
            // cannot draw random bytes, are both THIS process failing — not the
            // caller's request being wrong. An earlier draft answered `Mint`
            // with `UUIDV7_INVALID_ID_SHAPE`, which is a 400: it told an
            // operator their enrolment was malformed while the fault was here.
            // The queue joins these rather than getting a code of its own: the
            // Zig assign path logs `ERR_INTERNAL_OPERATION_FAILED` for every
            // Redis failure it meets, and a new code would fire the ERROR
            // REGISTRY gate over a registry this family does not own.
            // Every provider-resolution failure answers the code
            // `service_billing.zig` logs for the whole family
            // (`ERR_INTERNAL_OPERATION_FAILED`), and the vault join them
            // because that is what `crypto_store.zig` logs when an envelope
            // will not open. The finer `UZ-PROVIDER-*` codes exist in the Zig
            // registry and belong to the TENANT plane's handler, which is
            // M178's — declaring them here for a path that cannot emit them
            // would be an unreferenced code that looks like coverage.
            // `Envelope` is a producer writing an entry this daemon cannot
            // execute — not the asking runner's fault, so it answers as an
            // internal failure rather than a 4xx telling a healthy runner to
            // stop asking.
            ErrorKind::Envelope { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::Queue { .. }
            // A stored config this daemon cannot read joins the family for the
            // registry reason the queue does: the finer code an operator would
            // want does not exist in the Zig registry, and minting one here
            // would fire the ERROR REGISTRY gate over a registry this family
            // does not own. The parser's own error says which rule the
            // document broke, and it survives in the source chain.
            | ErrorKind::ConfigUnreadable { .. } => error_code::INTERNAL_OPERATION_FAILED,
            // Two vault failures, two codes, matching the two the Zig logs:
            // `crypto_store.decrypt_failed` answers the internal code above
            // because which check failed is an oracle, while `vault.zig`'s
            // parse failure answers this one because the body's SHAPE is a
            // fact the operator who stored it can act on.
            ErrorKind::VaultDataInvalid => error_code::VAULT_DATA_INVALID,
            // The one provider-family failure with a code of its own, because
            // it is the one an operator can ACT on: the fleet named a
            // credential and nobody stored it. `secrets_resolve.zig` logs the
            // same code, and the entry already exists in the Zig registry.
            // The six lease-lifecycle refusals, each with its own registry
            // code. None of them is an internal failure and none is a bad
            // request: they are all one fact — this runner may not do this to
            // this lease — observed at six different moments, and the runner
            // acts differently on every one. That is why they do not share a
            // code the way the provider family above does.
            ErrorKind::StaleFence => error_code::RUN_STALE_FENCING_TOKEN,
            ErrorKind::LeaseNotFound => error_code::RUN_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => error_code::RUN_LEASE_LOST,
            ErrorKind::LeaseMaxRuntime => error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => error_code::RUN_BUDGET_EXCEEDED,
            // A corrupt row, so the same code a column that will not parse
            // answers — this IS that, found by range rather than by shape.
            ErrorKind::SequenceCorrupt => error_code::INTERNAL_DB_QUERY,
            // A 404, and it is the one 404 on this plane that is not a
            // refusal: a skill-only bundle stores no snapshot, so this is what
            // the ordinary case looks like from the wire.
            ErrorKind::BundleMissing => error_code::FLEET_BUNDLE_NOT_FOUND,
            // Three ways for a snapshot to be unservable, one code, because
            // the RUNNER acts identically on all three — it re-polls. The Zig
            // answers `ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE` for the first two
            // and has no third; what separates them for an operator is
            // `detail` below and the source chain in the log.
            ErrorKind::BundleUnconfigured
            | ErrorKind::BundleStorage { .. }
            | ErrorKind::BundleOversized { .. } => error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
            // The mint family. A workspace that connected nothing and a handle
            // naming an unknown connector are one code, for the reason their
            // shared sentence gives.
            ErrorKind::IntegrationNotConnected => error_code::CRED_INTEGRATION_NOT_CONNECTED,
            ErrorKind::MintUnconfigured => error_code::CRED_BROKER_NOT_CONFIGURED,
            // GitHub keeps two codes where the refresh connectors share one,
            // and the asymmetry is the Zig's: an App installation has its own
            // reconnect semantics, and a runner that meets UZ-GH-001 knows a
            // HUMAN must reinstall an App rather than that a token exchange
            // failed.
            ErrorKind::GithubReconnectRequired => error_code::GH_RECONNECT_REQUIRED,
            ErrorKind::GithubMintFailed => error_code::GH_MINT_FAILED,
            // One code, two sentences: a Zoho failure must never route a runner
            // to GitHub's reconnect, and which of the two it was is what the
            // detail says.
            ErrorKind::ConnectorReconnectRequired | ErrorKind::ConnectorMintFailed => {
                error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED
            }
            ErrorKind::GrantRequired => error_code::GRANT_NOT_FOUND,
            // Three refusals, three codes, because the remedies differ: wait
            // for a human, re-raise the card against the reach the fleet now
            // declares, or answer a new approval.
            ErrorKind::WriteUnapproved => error_code::REPAIR_WRITE_UNAPPROVED,
            ErrorKind::BindingDrift => error_code::REPAIR_BINDING_DRIFT,
            ErrorKind::WriteSpendExhausted => error_code::REPAIR_SPEND_EXHAUSTED,
            // The memory operator surface's three, each with its own code
            // because the remedies are three different things: name a fleet
            // this workspace holds, come back when the store is up, or check
            // the key. The unavailable one is deliberately NOT the
            // `INTERNAL_DB_QUERY` a refused statement answers everywhere else
            // — memory is the datastore this product degrades around, and a
            // 503 is what tells a client the fleet is still running.
            ErrorKind::MemoryFleetNotFound => error_code::MEM_AGENTSFLEET_NOT_FOUND,
            ErrorKind::MemoryUnavailable { .. } => error_code::MEM_UNAVAILABLE,
            ErrorKind::MemoryEntryNotFound => error_code::MEM_ENTRY_NOT_FOUND,
            // The login family. Each field answers its own code because the
            // command line renders a different prompt for each — a bad key is
            // the client's own bug, a bad code is the person's typing.
        }
    }

    /// The sentence the caller is told.
    ///
    /// A rejection quotes its own detail, because the caller can act on it —
    /// that is the whole reason the kind exists. Every other kind answers a
    /// FIXED sentence, byte-identical to the one `problem_response.zig` writes:
    /// an internal failure that quotes its cause is an internal failure leaking
    /// its cause to whoever provoked it, and the cause is in the log where an
    /// operator can read it beside the request id.
    ///
    /// Not an `Option`. Every refusal this plane writes carries a detail, and a
    /// `None` would push the choice of what to say into each handler — which is
    /// how two call sites end up describing one failure differently.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        match self.inner.kind {
            ErrorKind::Rejected { detail } => detail,
            ErrorKind::Datastore { .. } => DETAIL_DATABASE_UNAVAILABLE,
            // A corrupt sequence joins the two row faults: all three are the
            // database holding something this daemon cannot use, and a caller
            // is told the same thing because there is nothing it can do about
            // any of them.
            ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::SequenceCorrupt => DETAIL_DATABASE_ERROR,
            ErrorKind::Queue { .. } => DETAIL_QUEUE_UNAVAILABLE,
            ErrorKind::Billing { ref source } => source.detail(),
            ErrorKind::Credential { ref source } => source.detail(),
            ErrorKind::Gate { ref source } => source.detail(),
            ErrorKind::Envelope { .. } => DETAIL_EVENT_MALFORMED,
            ErrorKind::Mint { .. } | ErrorKind::Entropy { .. } => DETAIL_REGISTRATION_FAILED,
            ErrorKind::VaultDataInvalid => DETAIL_VAULT_DATA_INVALID,
            ErrorKind::ConfigUnreadable { .. } => DETAIL_CONFIG_UNREADABLE,
            ErrorKind::StaleFence => DETAIL_STALE_FENCE,
            ErrorKind::LeaseNotFound => DETAIL_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => DETAIL_LEASE_LOST,
            ErrorKind::LeaseMaxRuntime => DETAIL_LEASE_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => DETAIL_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => DETAIL_BUDGET_EXHAUSTED,
            ErrorKind::BundleMissing => DETAIL_BUNDLE_NOT_FOUND,
            ErrorKind::BundleUnconfigured => DETAIL_BUNDLE_STORAGE_UNAVAILABLE,
            // An oversized object joins the store failure rather than getting a
            // sentence of its own. The size is a fact about what an operator
            // stored, and telling the asking runner would say more about this
            // deployment's contents than a bundle fetch should.
            ErrorKind::BundleStorage { .. } | ErrorKind::BundleOversized { .. } => {
                DETAIL_BUNDLE_FETCH_FAILED
            }
            ErrorKind::IntegrationNotConnected => DETAIL_INTEGRATION_NOT_CONNECTED,
            ErrorKind::MintUnconfigured => DETAIL_MINT_UNCONFIGURED,
            ErrorKind::GithubReconnectRequired => DETAIL_GITHUB_RECONNECT,
            ErrorKind::GithubMintFailed => DETAIL_MINT_FAILED,
            ErrorKind::ConnectorReconnectRequired => DETAIL_CONNECTOR_RECONNECT,
            ErrorKind::ConnectorMintFailed => DETAIL_CONNECTOR_MINT_FAILED,
            ErrorKind::GrantRequired => DETAIL_GRANT_REQUIRED,
            ErrorKind::WriteUnapproved => DETAIL_WRITE_UNAPPROVED,
            ErrorKind::BindingDrift => DETAIL_BINDING_DRIFT,
            ErrorKind::WriteSpendExhausted => DETAIL_WRITE_SPEND_EXHAUSTED,
            ErrorKind::MemoryFleetNotFound => DETAIL_MEMORY_AGENTSFLEET_NOT_FOUND,
            // The one kind besides `Rejected` whose sentence the CALL SITE
            // chose. Four operations answer `UZ-MEM-003`, and which of them a
            // 503 came from is the only fact its reader can act on — see
            // [`super::report::memory_unavailable`].
            ErrorKind::MemoryUnavailable { detail, .. } => detail,
            ErrorKind::MemoryEntryNotFound => DETAIL_MEMORY_ENTRY_NOT_FOUND,
        }
    }
}
