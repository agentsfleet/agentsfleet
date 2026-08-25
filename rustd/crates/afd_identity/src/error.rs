//! The error vocabulary this crate answers with, and the alias beside it.
//!
//! Both failure modes used to live next to the code that raises them — a
//! `BlankSecret` struct in `provider.rs`, `ClaimUnavailable` in
//! `capability.rs` — and `docs/RUST_ERROR_STANDARD.md` recorded that as an open
//! gap rather than a choice. Gathering them here is the point of the rule: a
//! reader looking for "how can this crate fail" reads one file rather than
//! finding out one signature at a time.
//!
//! # Why one of the two survived as its own type and the other did not
//!
//! `BlankSecret` was a unit struct that exactly one function returned and
//! nothing anywhere matched on. It is a variant of [`Error`] now, because a
//! distinct type earns its keep only when a caller DISCRIMINATES on it.
//!
//! [`ClaimUnavailable`] does earn it, and so it stays: `UnknownSubject` is
//! deliberately NOT an outage — the caller matches on it and answers with the
//! empty capability set — and folding it into a general "something went wrong"
//! would take that decision away from the only layer able to make it. It
//! composes into [`Error`] by `From` for callers that only propagate.

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

/// Anything this crate can fail with.
///
/// Composed by `From` per `docs/RUST_ERROR_STANDARD.md` rule 2, so `?` lifts
/// and the underlying failure survives as a `source()` for the fatal renderer
/// to walk. `#[error(transparent)]` on both arms because neither adds anything
/// a caller does not already have — the specific type's own message IS the
/// explanation, and wrapping it in a second sentence would only make the chain
/// longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The identity provider's backend secret is blank.
    ///
    /// `clerk_scope_fetch.zig` treats an absent or blank secret as
    /// `MissingSecret` for the same reason: capabilities cannot resolve at all
    /// without it, which is an outage rather than an empty grant, and saying so
    /// at boot beats discovering it on the first authenticated request.
    #[error("the identity provider's backend secret must not be blank")]
    BlankSecret,
    /// A capability claim did not come back.
    #[error(transparent)]
    Claim(#[from] ClaimUnavailable),
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
