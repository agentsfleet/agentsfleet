//! What this crate answers with — which is another crate's error, on purpose.
//!
//! Every other crate under `rustd/` owns an [`Error`](afd_core::error::Error)
//! of its own. This one does not, and the reason is worth stating rather than
//! leaving a reader to infer from a missing file:
//! [`Credentials`](crate::Credentials) exists to implement `afd_auth`'s
//! [`CredentialDirectory`](afd_auth::directory::CredentialDirectory) and
//! [`CapabilitySource`](afd_auth::capability::CapabilitySource) seams, and both
//! of those signatures mandate [`Unavailable`]. A crate implementing a foreign
//! trait does not get to choose the trait's error type, and inventing a second
//! one to convert from at the boundary would add a type that exists only to be
//! erased one line later.
//!
//! # Why the alias is here anyway
//!
//! `docs/RUST_ERROR_STANDARD.md` rule 1 asks that a reader never has to check
//! WHICH error a signature returns to know what it is. That is satisfied by an
//! alias whichever crate owns the type it defaults to — and this file is where
//! a reader goes to find out that the answer is `afd_auth`'s, rather than
//! discovering it from an import halfway down `credentials/rows.rs`.
//!
//! # Why `Unavailable` and not something finer
//!
//! It is deliberately opaque. A pool timeout, a malformed row, a subject that
//! will not parse — every one of them means the same thing to the layer above:
//! the directory could not answer, so no authentication decision can be made.
//! An authentication decision must never branch on WHY, because "the datastore
//! is slow" and "this credential is bad" have to stay distinguishable, and a
//! richer error here is how they stop being.

pub use afd_auth::error::Unavailable;

/// The result every fallible function in this crate returns.
///
/// Defaulted to [`Unavailable`] rather than to a type of this crate's own, for
/// the reason the module note gives. The spelling is the same one every sibling
/// crate uses, which is the property rule 1 is actually after.
pub type Result<T, E = Unavailable> = core::result::Result<T, E>;
