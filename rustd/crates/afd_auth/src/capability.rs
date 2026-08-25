//! What a person may do, asked of whoever owns the answer.
//!
//! The Rust spelling of Zig's `scopes.ScopeFn`, and it carries the same rule
//! `docs/AUTH.md` states twice: **a credential proves WHO, the provider answers
//! WHAT.** No credential class grants anything of its own. Narrowing a person
//! at the identity provider narrows every credential they hold, on the next
//! request past the freshness window, with no deploy and no backfill.
//!
//! # Why this is a seam and not a call
//!
//! It reaches the network, and a middleware's branches must be provable without
//! one. Zig injects a function pointer for exactly this reason; the trait is the
//! same seam with the argument types written down.
//!
//! # The three outcomes, and why one of them is not an error
//!
//! - **A claim resolves.** Parsed by [`crate::scope::parse_claim`] — the same
//!   parser the token path uses, so three credential shapes cannot drift in how
//!   a claim string becomes a capability set.
//! - **The subject is unknown to the provider.** [`ScopeSet::EMPTY`], which is
//!   an ANSWER and not a failure: the person is gone, so every gate refuses them
//!   by name. Telling a terminal "try again" would be telling it to retry a
//!   credential that will never work.
//! - **The provider could not be reached.** [`Unavailable`]. Never an empty
//!   set — an empty set reads to an operator as a demotion they never received,
//!   and it would be indistinguishable from the case above.

use crate::error::Unavailable;
use crate::principal::Subject;
use crate::scope::ScopeSet;

/// Answers "what may this subject do, right now?".
///
/// # Errors
/// [`Unavailable`] when the provider could not be asked — distinct from a
/// subject the provider does not know, which resolves to [`ScopeSet::EMPTY`].
///
/// # Design
///
/// One method, per `M-DI-HIERARCHY`. Consumed as a generic parameter rather
/// than a `dyn`, so the request path costs no allocation and no virtual call.
pub trait CapabilitySource: Send + Sync + std::fmt::Debug {
    /// Resolves the capability set for `subject`.
    ///
    /// # Errors
    /// [`Unavailable`] when the provider could not be asked.
    fn capabilities(
        &self,
        subject: &Subject,
    ) -> impl Future<Output = Result<ScopeSet, Unavailable>> + Send;
}

/// A source that refuses every request.
///
/// What a deployment with no provider secret configured holds. It is NOT an
/// empty-set source, and the difference is the whole point: an unconfigured
/// provider is an outage every gate reports as such, while an empty set would
/// authenticate a caller and then refuse them at every gate as though they had
/// been narrowed to nothing. `clerk_scope_resolver.zig` makes the same choice
/// by treating an absent secret as a fetch failure.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCapabilitySource;

impl CapabilitySource for NoCapabilitySource {
    fn capabilities(
        &self,
        _subject: &Subject,
    ) -> impl Future<Output = Result<ScopeSet, Unavailable>> + Send {
        std::future::ready(Err(Unavailable))
    }
}
