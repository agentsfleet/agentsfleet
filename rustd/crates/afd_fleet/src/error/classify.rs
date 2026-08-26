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
    DETAIL_BUDGET_EXHAUSTED, DETAIL_CONFIG_UNREADABLE, DETAIL_CREDENTIAL_MISSING,
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_EVENT_MALFORMED, DETAIL_LEASE_LOST,
    DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND, DETAIL_PROVIDER_UNRESOLVED,
    DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED, DETAIL_RENEWAL_NO_CREDITS,
    DETAIL_RUNNER_NOT_FOUND, DETAIL_STALE_FENCE, DETAIL_VAULT_DATA_INVALID, Error, ErrorKind,
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

    /// Whether an authenticated runner's row has since disappeared.
    ///
    /// Answered separately from a rejection because the remedy differs: the
    /// token is real and the enrolment is gone, so the host must be re-enrolled
    /// rather than retried.
    #[must_use]
    pub fn is_runner_vanished(&self) -> bool {
        matches!(self.inner.kind, ErrorKind::RunnerVanished)
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
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => {
                error_code::INTERNAL_DB_QUERY
            }
            ErrorKind::RunnerVanished => error_code::RUN_INVALID_RUNNER_TOKEN,
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
            | ErrorKind::ProviderMalformed { .. }
            | ErrorKind::ProviderSecretMissing
            | ErrorKind::ProviderPlatformKeyMissing
            | ErrorKind::ProviderNoWorkspace
            | ErrorKind::ProviderEndpoint { .. }
            // A stored config this daemon cannot read joins the family for the
            // registry reason the queue does: the finer code an operator would
            // want does not exist in the Zig registry, and minting one here
            // would fire the ERROR REGISTRY gate over a registry this family
            // does not own. The parser's own error says which rule the
            // document broke, and it survives in the source chain.
            | ErrorKind::ConfigUnreadable { .. }
            | ErrorKind::Vault { .. } => error_code::INTERNAL_OPERATION_FAILED,
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
            ErrorKind::CredentialMissing => error_code::AGENTSFLEET_CREDENTIAL_MISSING,
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
            ErrorKind::RunnerVanished => DETAIL_RUNNER_NOT_FOUND,
            ErrorKind::Datastore { .. } => DETAIL_DATABASE_UNAVAILABLE,
            ErrorKind::Query { .. } | ErrorKind::RowMalformed { .. } => DETAIL_DATABASE_ERROR,
            ErrorKind::Queue { .. } => DETAIL_QUEUE_UNAVAILABLE,
            ErrorKind::Envelope { .. } => DETAIL_EVENT_MALFORMED,
            ErrorKind::Mint { .. } | ErrorKind::Entropy { .. } => DETAIL_REGISTRATION_FAILED,
            ErrorKind::ProviderMalformed { .. }
            | ErrorKind::ProviderSecretMissing
            | ErrorKind::ProviderPlatformKeyMissing
            | ErrorKind::ProviderNoWorkspace
            | ErrorKind::ProviderEndpoint { .. }
            | ErrorKind::Vault { .. } => DETAIL_PROVIDER_UNRESOLVED,
            ErrorKind::VaultDataInvalid => DETAIL_VAULT_DATA_INVALID,
            ErrorKind::CredentialMissing => DETAIL_CREDENTIAL_MISSING,
            ErrorKind::ConfigUnreadable { .. } => DETAIL_CONFIG_UNREADABLE,
            ErrorKind::StaleFence => DETAIL_STALE_FENCE,
            ErrorKind::LeaseNotFound => DETAIL_LEASE_NOT_FOUND,
            ErrorKind::LeaseLost => DETAIL_LEASE_LOST,
            ErrorKind::LeaseMaxRuntime => DETAIL_LEASE_MAX_RUNTIME,
            ErrorKind::RenewalNoCredits => DETAIL_RENEWAL_NO_CREDITS,
            ErrorKind::BudgetExhausted => DETAIL_BUDGET_EXHAUSTED,
        }
    }

    /// Whether this failure is a stored-CONFIGURATION fault rather than an
    /// infrastructure one.
    ///
    /// The question the admission pass turns on, and the reason it is one
    /// method rather than a `match` at the call site: a permanent fault earns
    /// the terminal `gate_blocked` row, and a transient one leaves the delivery
    /// leasable for the next poll. `resolveTenant` decides the same thing with
    /// a four-arm `switch` and an `else`, which is how the classification came
    /// to be a property of the arm ORDER instead of a property of the failure.
    ///
    /// Exhaustive, so a new kind fails the build until it is classified — the
    /// same device [`Error::code`] uses.
    ///
    /// # A deliberate divergence from the Zig
    ///
    /// [`ErrorKind::ProviderEndpoint`] answers `true`, and the Zig's
    /// `resolveTenant` answers the equivalent `false`: `SecretEndpointInvalid`
    /// is absent from its permanent list and falls through its `else`, so a
    /// stored endpoint that fails the SSRF guard is re-polled forever. The
    /// event never terminates, no terminal row is written, and the only trace
    /// is a warn line repeating at the poll interval.
    ///
    /// That is a latent defect rather than a decision — a stored URL pointing
    /// at the metadata service does not become safe by being retried — and the
    /// reason it is corrected here rather than copied is that the row-parity it
    /// would cost is documentary. The dual-run differ went with the Zig
    /// integration lanes, so nothing compares the two daemons' rows at runtime;
    /// what grades Invariant 5 is REVIEW reading the ported SQL side by side,
    /// and this changes no statement. Registered as a divergence beside the
    /// issue-time debit (Indy, this stream).
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.inner.kind {
            ErrorKind::ProviderMalformed { .. }
            | ErrorKind::ProviderSecretMissing
            | ErrorKind::ProviderPlatformKeyMissing
            | ErrorKind::ProviderNoWorkspace
            // A declared credential nobody stored, and a stored body that is
            // not an addressable object, are both things a human has to go and
            // fix. `resolveSecretsMap`'s `error.CredentialNotFound` reaches
            // `blockEvent` through the fleet loop's own permanent arm, so this
            // classification is the Zig's rather than a correction to it.
            | ErrorKind::CredentialMissing
            | ErrorKind::VaultDataInvalid
            // A document that will not parse does not become parseable by
            // being read again. Every poll would re-read the same bytes, fail
            // the same rule, and leave the delivery leasable forever — so this
            // earns the terminal row, which is the thing that puts the fleet
            // in front of a human.
            | ErrorKind::ConfigUnreadable { .. }
            // The corrected one — see the divergence note above.
            | ErrorKind::ProviderEndpoint { .. } => true,
            // Everything else is infrastructure, and infrastructure recovers.
            //
            // `Vault` stays here on purpose, next to the variant that just
            // moved. An envelope that will not open is USUALLY permanent too —
            // a damaged row, a rotated key — but it is also what a truncated
            // read or a half-written row looks like, and those do recover. The
            // asymmetry is not an oversight: a stored URL is data this daemon
            // parsed and rejected, while an unopened envelope is data it never
            // got to see.
            ErrorKind::Vault { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RunnerVanished
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Envelope { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Mint { .. }
            // The six lease-lifecycle refusals are not configuration faults at
            // all — nothing is stored wrong and nobody has to go and fix a
            // document. They answer a runner about ONE request against ONE
            // lease, and the event behind them stays exactly as leasable as it
            // was. A `true` here would write a terminal `gate_blocked` row for
            // a fleet whose only problem was that one runner reported late.
            | ErrorKind::StaleFence
            | ErrorKind::LeaseNotFound
            | ErrorKind::LeaseLost
            | ErrorKind::LeaseMaxRuntime
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            | ErrorKind::Entropy { .. } => false,
        }
    }
}
