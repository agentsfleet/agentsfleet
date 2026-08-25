//! Turning a credential into the row it names, behind a seam.
//!
//! The Rust spelling of Zig's injected `LookupFn`, and it exists for the reason
//! that one does: the routing decision must be provable without a network or a
//! database. `src/auth/` in the Zig tree may not import `src/db/` at all —
//! `make test-auth` greps for it — and this crate reaches the same wall by
//! construction, because `afd_auth` does not list `sqlx` and so cannot name it.
//!
//! The Postgres implementations live where Zig's do: with the host
//! (`cmd/serve_runner_lookup.zig`, `cmd/cli_credential_lookup.zig`), which for
//! this port is §5. What ships here is the trait, the record it returns, and —
//! under `test-util` — an in-memory directory that proves every branch.
//!
//! # Why the record is an enum and not a struct with optional fields
//!
//! The same argument [`crate::principal`] makes. A flat record with
//! `tenant`, `subject`, `runner` and `degraded` all optional needs four rules
//! about which combinations are legal, and each rule is a comment a lookup
//! implementation has to remember. Here a machine record has no tenant field to
//! populate and a person record has no `degraded` field to invert.

use afd_core::id::Uuid7;

use crate::credential::{CredentialKind, Presented};
use crate::error::Unavailable;
use crate::principal::Subject;

/// The SHA-256 of a presented credential, lower-case hexadecimal.
///
/// The form every credential column stores, and the only form that leaves this
/// crate carrying anything derived from a secret. A digest is not itself
/// secret — that is the entire point of storing it instead of the value — so
/// unlike [`Presented`] it renders in full.
///
/// # Why comparison is delegated rather than made constant-time here
///
/// RULE CTM requires a constant-time compare for a secret compared IN PROCESS.
/// This one is not: it goes to Postgres as a bind parameter and is matched by
/// an indexed equality. That is safe for a different reason — the digest's
/// input is 256 bits of cryptographic entropy, so a timing oracle on the digest
/// buys an attacker nothing short of a preimage. The spec sentence "all hash
/// compares timing-safe" is imprecise about which mechanism is doing the work;
/// this note is the precise version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(Box<str>);

/// Bytes in a SHA-256 digest.
const DIGEST_BYTES: usize = 32;
/// Characters in its lower-case hexadecimal rendering.
pub const DIGEST_HEX_LEN: usize = DIGEST_BYTES * 2;

impl Digest {
    /// Hashes a presented credential the way `api_key.zig::sha256Hex` does.
    ///
    /// Over the WHOLE presented value, marker included — the Zig daemon hashes
    /// `provided`, not the body after the prefix, and the stored column holds
    /// that. Hashing the body alone would authenticate nothing.
    #[must_use]
    pub fn of(presented: &Presented) -> Self {
        Self::of_minted(presented.expose())
    }

    /// Hashes a credential this daemon just MINTED.
    ///
    /// A minted value has no `Authorization` header to parse and cannot be
    /// blank, so it never passes through [`Presented`] — but it MUST be hashed
    /// by the same code that hashes what the holder later presents, or the row
    /// stores a digest no lookup will ever match. Both paths therefore land
    /// here, and [`Digest::of`] is the thin wrapper.
    ///
    /// Splitting this out rather than making the minter construct a
    /// `Presented` it does not need also avoids an unreachable blank-check
    /// branch at every mint site, which would be dead code this workspace
    /// measures.
    #[must_use]
    pub fn of_minted(raw: &str) -> Self {
        use sha2::Digest as _;
        let hashed = sha2::Sha256::digest(raw.as_bytes());
        let mut hex = String::with_capacity(DIGEST_HEX_LEN);
        for byte in hashed {
            use std::fmt::Write as _;
            // A two-digit lower-case hex write into a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
        }
        Self(hex.into())
    }

    /// The digest as the lower-case hex a credential column stores.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether the row a credential names is still usable.
///
/// A row is RETURNED rather than filtered out when it is dead, so a revoked
/// credential can answer its own code instead of being indistinguishable from
/// one that never existed. That distinction is the difference between telling a
/// terminal "stop retrying this" and letting it retry forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// The row is active and the credential may be honoured.
    Live,
    /// The row exists and has been revoked, cordoned, drained or deleted.
    Revoked,
}

/// What a directory found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialRecord {
    /// A credential belonging to a person.
    ///
    /// Carries the identity-provider SUBJECT, never a `core.users` primary key.
    /// All three Zig person paths store the subject in a field named `user_id`
    /// with a comment at each site explaining the name is wrong; [`Subject`] is
    /// the fix, and a directory implementation that hands over a row id no
    /// longer compiles.
    ///
    /// It carries no workspace ceiling, and has no field for one: a ceiling
    /// reaches a principal only from a session token's `workspace_id` claim,
    /// which no stored credential has.
    Person {
        /// The tenant the person acts in.
        tenant: Uuid7,
        /// The provider subject capabilities resolve against.
        subject: Subject,
        /// Whether the row is still usable.
        live: Liveness,
    },
    /// A credential belonging to a machine.
    ///
    /// No tenant field, because a runner holds no tenant authority: secret
    /// delivery to a runner is placement, not a standing grant.
    Machine {
        /// The `fleet.runners` row the credential proved.
        runner: Uuid7,
        /// The row's reconciled verdict, read in the same statement as the
        /// token hash so the lease gate never re-reads the row.
        degraded: bool,
        /// Whether the row is still usable.
        live: Liveness,
    },
}

/// Resolves a credential digest to the record it names.
///
/// # Errors
/// [`Unavailable`] when the datastore could not answer — never for a credential
/// that simply matches nothing, which is `Ok(None)`. Collapsing the two would
/// report a Postgres outage as an authentication rejection, and the runner
/// client counts rejections toward a self-termination ceiling.
///
/// # Design
///
/// One method, per `M-DI-HIERARCHY`'s "each trait should be relatively narrow".
/// Returns `impl Future` rather than being an `async fn` in a `dyn`-safe trait
/// because callers take it as a generic parameter — one level deep, which is
/// the rung the guideline puts above `dyn Trait`.
pub trait CredentialDirectory: Send + Sync + std::fmt::Debug {
    /// Looks `digest` up in the store `kind` names.
    ///
    /// One method across all three stored classes rather than one trait each,
    /// because one implementation holds one pool and runs three queries — which
    /// is what the Zig host does with three `LookupFn`s wired from the same
    /// `serve_boot`. Splitting it would make the plumbing three times as wide
    /// to say the same thing.
    ///
    /// `kind` is never [`CredentialKind::OidcSessionToken`]: a session token is
    /// verified, never looked up, and the one call site proves it by dispatch.
    /// An implementation asked anyway should answer `Ok(None)`.
    ///
    /// # Errors
    /// [`Unavailable`] when the datastore could not answer.
    fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> impl Future<Output = Result<Option<CredentialRecord>, Unavailable>> + Send;
}
