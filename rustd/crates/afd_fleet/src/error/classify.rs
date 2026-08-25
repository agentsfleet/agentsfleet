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
    DETAIL_DATABASE_ERROR, DETAIL_DATABASE_UNAVAILABLE, DETAIL_EVENT_MALFORMED,
    DETAIL_PROVIDER_UNRESOLVED, DETAIL_QUEUE_UNAVAILABLE, DETAIL_REGISTRATION_FAILED,
    DETAIL_RUNNER_NOT_FOUND, Error, ErrorKind,
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
            | ErrorKind::Vault { .. } => error_code::INTERNAL_OPERATION_FAILED,
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
    /// # A known divergence, ported deliberately
    ///
    /// [`ErrorKind::ProviderEndpoint`] answers `false` — transient — because
    /// `SecretEndpointInvalid` is absent from `resolveTenant`'s permanent list
    /// and falls through its `else`. A stored endpoint that fails the SSRF
    /// guard cannot fix itself, so retrying it forever is almost certainly a
    /// latent defect in the Zig rather than a decision. It is copied rather
    /// than corrected because the two daemons must write the same rows during
    /// the cutover, and correcting it here would have the Rust daemon write a
    /// terminal row the Zig does not — which is Invariant 5. Flipping it later
    /// is one line, and `an_ssrf_refusal_is_ported_as_transient` is the test
    /// that will fail when someone does, so the change cannot be silent.
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.inner.kind {
            ErrorKind::ProviderMalformed { .. }
            | ErrorKind::ProviderSecretMissing
            | ErrorKind::ProviderPlatformKeyMissing
            | ErrorKind::ProviderNoWorkspace => true,
            // See the divergence note above for `ProviderEndpoint`; everything
            // else here is infrastructure, and infrastructure recovers.
            ErrorKind::ProviderEndpoint { .. }
            | ErrorKind::Vault { .. }
            | ErrorKind::Datastore { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RunnerVanished
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Envelope { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::Entropy { .. } => false,
        }
    }
}
