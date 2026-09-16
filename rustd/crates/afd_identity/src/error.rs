//! The error vocabulary this crate answers with, and the alias beside it.
//!
//! Both failure modes used to live next to the code that raises them — a
//! `BlankSecret` struct in `provider.rs`, `ClaimUnavailable` in
//! `capability.rs` — and `docs/RUST_ERROR_STANDARD.md` recorded that as an open
//! gap rather than a choice. Gathering them here is the point of the rule: a
//! reader looking for "how can this crate fail" reads one file rather than
//! finding out one signature at a time.
//!
//! The hull is `afd_core::error_shell!`, the same one every sibling crate
//! carries, so the boxed kind keeps `Result` pointer-sized on the `Ok` path and
//! the backtrace, the `Display` and the `source()` that skips our own kind are
//! all generated rather than written here for the ninth time.
//!
//! # Why two of the three survived as their own types and one did not
//!
//! `BlankSecret` was a unit struct that exactly one function returned and
//! nothing anywhere matched on. It is a kind of [`Error`] now, because a
//! distinct type earns its keep only when a caller DISCRIMINATES on it.
//!
//! [`ClaimUnavailable`] and [`MetadataUnwritten`] do earn it, and so they stay:
//! `UnknownSubject` is deliberately NOT an outage — the caller matches on it and
//! answers with the empty capability set — and folding it into a general
//! "something went wrong" would take that decision away from the only layer able
//! to make it. Both stay `Copy + PartialEq + Eq`, because the callers that match
//! on them compare them; only the crate-level [`Error`] takes the hull. They
//! compose into it through `error_lifts!` for callers that only propagate.
//!
//! # The codes
//!
//! A blank backend secret is a deployment that cannot serve capabilities at all,
//! so it answers `STARTUP_ENV_CHECK` — the family every other boot-time
//! configuration refusal already uses. A claim that did not come back is
//! `AUTH_UNAVAILABLE`, which is what the gate above already answers for a
//! directory it could not reach. A metadata writeback is best-effort and behind
//! an already-committed tenant row, so it is internal rather than either.

use afd_core::error_code::{self, ErrorCode};

mod raise;

pub(crate) use self::raise::blank_secret;
#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to that crate's own [`Error`] — the shape
/// `core_api` has run in production on for years, and the one bun uses
/// (`pub type Result<T, E = Error>`). The default parameter is what lets the
/// few functions answering with a different error keep the same spelling:
/// `Result<T>` for the common case, `Result<T, ClaimUnavailable>` where the
/// caller has to tell an outage from an answer.
///
/// The point is not brevity. It is that a reader never has to check WHICH
/// error a signature returns to know it is this crate's, and a new call site
/// cannot quietly introduce a second error type without saying so.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// An identity failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
///
/// Composed by `From` per `docs/RUST_ERROR_STANDARD.md` rule 2, so `?` lifts and
/// the underlying failure survives as a `source()` for the fatal renderer to
/// walk. `#[error(transparent)]` on both composed arms because neither adds
/// anything a caller does not already have — the specific type's own message IS
/// the explanation, and wrapping it in a second sentence would only make the
/// chain longer.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The identity provider's backend secret is blank.
    ///
    /// `clerk_scope_fetch.zig` treats an absent or blank secret as
    /// `MissingSecret` for the same reason: capabilities cannot resolve at all
    /// without it, which is an outage rather than an empty grant, and saying so
    /// at boot beats discovering it on the first authenticated request.
    ///
    /// Carries no source: nothing failed underneath, the value was simply blank
    /// (`RUST_ERROR_STANDARD` rule 4's second half).
    #[error("the identity provider's backend secret must not be blank")]
    BlankSecret,
    /// A capability claim did not come back.
    #[error(transparent)]
    Claim { source: ClaimUnavailable },
    /// A signup's tenant never reached the identity provider.
    #[error(transparent)]
    Metadata { source: MetadataUnwritten },
}

impl Error {
    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::BlankSecret => (error_code::STARTUP_ENV_CHECK, DETAIL_BLANK_SECRET),
            ErrorKind::Claim { .. } => (error_code::AUTH_UNAVAILABLE, DETAIL_CLAIM),
            ErrorKind::Metadata { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                afd_core::error::DETAIL_OPERATION_FAILED,
            ),
        }
    }

    /// The registry code a caller is refused with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence a caller is told.
    ///
    /// Static, and never the `source()` chain: an operator reads the chain in
    /// the log, and a caller who could read it would learn which provider this
    /// daemon brokers with and how it failed.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }

    /// Whether the identity provider behind this crate could not be reached.
    ///
    /// An outage answers 503, where a blank secret is a deployment that will
    /// never answer and a best-effort writeback is neither.
    #[must_use]
    pub fn is_provider_unavailable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Claim {
                source: ClaimUnavailable::Unreachable
            } | ErrorKind::Metadata {
                source: MetadataUnwritten::Unreachable
            }
        )
    }
}

/// The sentence a deployment with no backend secret earns.
const DETAIL_BLANK_SECRET: &str = "Identity provider is not configured";

/// The sentence a claim that did not come back earns.
const DETAIL_CLAIM: &str = "Identity provider unavailable";

/// Why a signup's tenant did not reach the identity provider.
///
/// Its own type for the reason [`ClaimUnavailable`] is: a caller
/// DISCRIMINATES on it. The writeback is best-effort — the tenant row is
/// already committed when it runs, so a provider outage must not turn signup
/// into a 500 — and what separates "this deployment is misconfigured" from
/// "the provider is having a bad minute" is exactly what an operator reading
/// the log needs, because only one of them is theirs to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MetadataUnwritten {
    /// The provider could not be reached, or answered a status with no other
    /// reading.
    #[error("the identity provider could not be written to")]
    Unreachable,
    /// The provider refused this daemon's own credential.
    ///
    /// Distinct because it is never transient and never the provider's fault:
    /// `CLERK_SECRET_KEY` is wrong or has lost the permission to write
    /// metadata, and retrying forever would only hide that.
    #[error("the identity provider refused this daemon's credential")]
    Unauthorized,
    /// The provider does not know the subject the event was about.
    ///
    /// The person was deleted between the event and this write. Nothing to
    /// repair and nothing to retry.
    #[error("the identity provider does not know this subject")]
    UnknownSubject,
}

/// Why a claim did not come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClaimUnavailable {
    /// The provider could not be asked.
    #[error("the identity provider could not be reached")]
    Unreachable,
    /// The provider answered that it does not know this subject.
    ///
    /// Not an outage. The person is gone — their credential outlived them —
    /// so they resolve to no capabilities and every gate refuses them by name.
    /// Telling a terminal to retry would be telling it to retry forever.
    #[error("the identity provider does not know this subject")]
    UnknownSubject,
}
