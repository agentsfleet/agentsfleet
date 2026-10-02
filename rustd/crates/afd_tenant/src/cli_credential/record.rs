//! The records the credential verbs take and answer: who a subject is, what a
//! mint needs, and what a mint or a revoke hands back.

use afd_auth::minted::Minted;
use afd_core::id::Uuid7;

use super::MachineName;

/// The person a proven subject names.
///
/// Every field comes from ONE read. The tenant is the joined user row's, which
/// is the authoritative one — the copy stamped on a credential row at mint is
/// provenance, never authority.
///
/// The mint and revoke paths read `id` and `tenant` and ignore the rest;
/// `GET /v1/users/me` renders all five. One record rather than a narrow one for
/// the writes and a wide one for the read: the question both ask is "who is this
/// subject", and two answers to it would be two things to keep true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// `core.users.id`, which the credential's foreign key points at.
    pub id: Uuid7,
    /// The tenant that user belongs to.
    pub tenant: Uuid7,
    /// The address the account was opened with.
    pub email: String,
    /// What they asked to be called, when they said. `NULL` in the column when
    /// the identity provider sent no name, and absent from the wire in turn.
    pub display_name: Option<String>,
    /// That tenant's name, which is what a person recognises.
    pub tenant_name: String,
}

/// What minting one credential needs.
#[derive(Debug, Clone, Copy)]
pub struct MintRequest<'a> {
    /// Whose credential it is, as `core.users.id`.
    pub user: &'a Uuid7,
    /// The tenant that user belongs to.
    pub tenant: &'a Uuid7,
    /// The terminal's label, already parsed.
    pub machine: MachineName<'a>,
    /// The deployment answering this request.
    ///
    /// Never a value the caller supplied: a credential and the deployment that
    /// minted it are one fact, and a client-asserted host would let them
    /// disagree.
    pub deployment: &'a str,
    /// Where the mint was requested from, for the audit trail.
    pub from_address: &'a str,
}

/// A credential, and the one view of its plaintext that will ever exist.
///
/// No `Clone`, for [`crate::apikey::Revealed`]'s reason: a second copy of a
/// credential is a second thing to zero, and the one that gets missed is the
/// one that stays in the heap.
#[derive(Debug)]
pub struct Revealed {
    /// The credential row's identifier.
    pub id: Uuid7,
    /// The terminal's label.
    pub machine_name: String,
    /// The plaintext, which zeroes when this is dropped.
    pub credential: Minted,
    /// The deployment that minted it.
    pub deployment: String,
}

/// A credential that this call revoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    /// The credential row's identifier.
    pub id: Uuid7,
    /// When the row records it stopped working.
    pub revoked_at_ms: i64,
}
