//! What was presented, and which class it belongs to.
//!
//! This module holds the routing decision: which credential class a presented
//! value belongs to.
//!
//! # Why a table and not a chain of `if`
//!
//! A branch chain exists when the credential classes it routes to are
//! hand-written procedures that differ only in constants — routing to them
//! then has to be hand-written too, and kept in the same order as a paragraph
//! of `docs/AUTH.md`.
//!
//! [`CredentialKind::of`] is a walk over a table. There is no branch to put in
//! the wrong order because there is no branch, and a new class is one row.
//!
//! # The property a chain would hold by accident
//!
//! `agt_t` and `agt_r` differ in one byte. A branch chain says nothing about
//! two markers not being prefixes of one another — it holds because of the
//! order somebody wrote the branches in, and a future `agt_` class would break
//! it silently by being tested first. The table is asserted PREFIX-FREE at
//! compile time, so the property is a build failure rather than a review
//! question.
//!
//! # The session token is the fall-through because it has no marker
//!
//! Not because it is "last". A session token carries nothing this daemon
//! minted, so there is no prefix to key on; every other class is something the
//! backend issued and marked. Stating it that way is what makes the ordering
//! non-arbitrary: the prefixed classes are matched because they CAN be, and the
//! remainder is what is left. Classifying is not accepting — an unparseable
//! token still fails at the verifier, and a deployment with no verifier refuses
//! the whole class.

use afd_crypto::secret::SecretString;
use std::fmt;

/// One credential class the daemon can prove.
///
/// A new class is a variant here, a row in the prefix table where it carries a
/// marker, and an arm wherever a [`crate::authenticate::Registry`] dispatches —
/// and the build fails until all three exist. That is the same guarantee
/// [`crate::scope::Scope`] gets from its exhaustive `wire()`/`bit()` matches.
///
/// # Why this is not `#[non_exhaustive]`
///
/// It was, and the attribute made the paragraph above false. `#[non_exhaustive]`
/// forces every crate OUTSIDE this one to write a `_` arm, and the crate that
/// most needs to match exhaustively — whichever one implements
/// [`crate::directory::CredentialDirectory`] — is outside by construction,
/// because `afd_auth` cannot name `sqlx`. A new class would have fallen into
/// somebody's catch-all and resolved to nothing, silently, which is the exact
/// failure the doc promises cannot happen.
///
/// The attribute buys API stability for downstream semver, and these crates
/// have no downstream: they are built only by this workspace's pinned
/// toolchain. It cost a compile-time guarantee and bought nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CredentialKind {
    /// `agt_t` — a tenant api-key, resolving to the person who minted it.
    TenantApiKey,
    /// `afc_` — a durable credential minted by `agentsfleet login`.
    CliCredential,
    /// `agt_r` — a host runner's machine credential.
    RunnerToken,
    /// A signed session token from the identity provider.
    ///
    /// Carries no marker of its own: it is what a presented value is when it is
    /// none of the others.
    OidcSessionToken,
}

/// The marker a tenant api-key carries.
pub const TENANT_API_KEY_PREFIX: &str = "agt_t";
/// The marker a credential minted by `agentsfleet login` carries.
pub const CLI_CREDENTIAL_PREFIX: &str = "afc_";
/// The runner wire's token prefix, the one the runner presents.
pub use afd_wire::paths::RUNNER_TOKEN_PREFIX;

/// Marker → class.
///
/// Order carries no meaning, which is the point: `PREFIXES_ARE_PREFIX_FREE`
/// proves no entry is a prefix of another, so first-match and longest-match
/// agree and the walk cannot classify a credential as a shorter class that
/// happens to sit earlier.
const PREFIXES: [(&str, CredentialKind); 3] = [
    (TENANT_API_KEY_PREFIX, CredentialKind::TenantApiKey),
    (RUNNER_TOKEN_PREFIX, CredentialKind::RunnerToken),
    (CLI_CREDENTIAL_PREFIX, CredentialKind::CliCredential),
];

impl CredentialKind {
    /// Every class, in catalogue order.
    pub const ALL: [Self; 4] = [
        Self::TenantApiKey,
        Self::CliCredential,
        Self::RunnerToken,
        Self::OidcSessionToken,
    ];

    /// The class `presented` belongs to.
    ///
    /// A table walk, not a branch chain. Falls through to
    /// [`CredentialKind::OidcSessionToken`] when no marker matches.
    /// Routing and the prefix-free assertion share [`const_starts_with`] on
    /// purpose: a matcher used only by the assertion would be proving a
    /// property about a comparison the router does not perform.
    #[must_use]
    pub fn of(presented: &Presented) -> Self {
        let raw = presented.expose();
        let mut table: &[(&str, Self)] = &PREFIXES;
        while let [(prefix, kind), rest @ ..] = table {
            if const_starts_with(raw, prefix) {
                return *kind;
            }
            table = rest;
        }
        Self::OidcSessionToken
    }

    /// The marker this class carries, or `None` for the one that carries none.
    ///
    /// Exhaustive, so a new variant fails to compile until it has answered
    /// whether it is markered — the question whose wrong answer would put a
    /// class in the fall-through where nothing routes to it.
    #[must_use]
    pub const fn prefix(self) -> Option<&'static str> {
        match self {
            Self::TenantApiKey => Some(TENANT_API_KEY_PREFIX),
            Self::CliCredential => Some(CLI_CREDENTIAL_PREFIX),
            Self::RunnerToken => Some(RUNNER_TOKEN_PREFIX),
            Self::OidcSessionToken => None,
        }
    }
}

/// Whether `haystack` begins with `needle`, in a `const` context.
///
/// Slice patterns rather than indexing, the device `scope.rs`'s `KNOWN_BITS`
/// already uses: there is no index, so `clippy::indexing_slicing` needs no
/// exemption to walk a slice at compile time.
const fn const_starts_with(haystack: &str, needle: &str) -> bool {
    let mut left = haystack.as_bytes();
    let mut right = needle.as_bytes();
    while let [want, right_rest @ ..] = right {
        match left {
            [got, left_rest @ ..] if *got == *want => {
                left = left_rest;
                right = right_rest;
            }
            _ => return false,
        }
    }
    true
}

/// No marker is a prefix of another marker.
///
/// The property that makes a first-match walk equal a longest-match one. Each
/// unordered pair is checked once in both directions, so two identical markers
/// mapped to different classes fail here too. A future `agt_` class added
/// carelessly fails at BUILD time rather than by quietly swallowing `agt_t` and
/// `agt_r` at run time.
const PREFIXES_ARE_PREFIX_FREE: bool = {
    let mut rest: &[(&str, CredentialKind)] = &PREFIXES;
    let mut ok = true;
    while let [(a, _), tail @ ..] = rest {
        let mut others: &[(&str, CredentialKind)] = tail;
        while let [(b, _), others_tail @ ..] = others {
            if const_starts_with(a, b) || const_starts_with(b, a) {
                ok = false;
            }
            others = others_tail;
        }
        rest = tail;
    }
    ok
};

const _: () = assert!(
    PREFIXES_ARE_PREFIX_FREE,
    "no credential marker may be a prefix of another: first-match routing would shadow the longer one"
);

/// The scheme prefix, including its separating space.
///
/// Matched case-SENSITIVELY, as a plain `strip_prefix`. A lower-case `bearer `
/// is refused, and `tests/bearer_prefix_routing.rs` pins it.
const BEARER_SCHEME: &str = "Bearer ";

/// The bytes trimmed before deciding a token is blank.
const BLANK_BYTES: [char; 4] = [' ', '\t', '\r', '\n'];

/// A credential exactly as the caller sent it.
///
/// Guards two invariants (`M-STRONG-TYPES-GUARD`), and neither is decorative:
///
/// - **Non-blank.** A header whose token is empty or all whitespace is one
///   401 branch for every caller. Refusing it in the constructor means no
///   later code has to remember.
/// - **Never rendered.** `Debug` prints the length and nothing else. A bearer
///   value in a log IS the credential, and a `#[derive(Debug)]` on any struct
///   that transitively holds one is how it gets there.
///
/// The value is stored untrimmed. Whitespace is consulted only to decide
/// blankness; [`crate::directory::Digest::of`] hashes the raw bytes, so a
/// stored digest matches exactly what the minter rendered. The bytes live in a
/// [`SecretString`], which wipes them when the request that carried them ends
/// (Invariant 5).
///
/// No equality: a credential is compared by its digest, and a derived `==` on
/// the plaintext would be a non-constant-time comparison waiting for a caller.
#[derive(Clone)]
pub struct Presented(SecretString);

/// A presented credential that carried nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a presented credential must not be blank")]
pub struct Blank;

impl Presented {
    /// Wraps the token half of an `Authorization: Bearer <token>` header.
    ///
    /// # Errors
    /// Returns [`Blank`] when the value is empty or only whitespace.
    pub fn new(raw: &str) -> Result<Self, Blank> {
        if raw.trim_matches(BLANK_BYTES).is_empty() {
            return Err(Blank);
        }
        Ok(Self(SecretString::new(raw.to_owned())))
    }

    /// Parses a whole `Authorization` header value.
    ///
    /// # Errors
    /// Returns [`Blank`] when the header does not begin with `Bearer ` or
    /// carries no token after it.
    pub fn from_authorization(header: &str) -> Result<Self, Blank> {
        header
            .strip_prefix(BEARER_SCHEME)
            .map_or(Err(Blank), Self::new)
    }

    /// The raw value, for hashing or for a verifier.
    ///
    /// Named `expose` rather than `as_str` so every call site reads as a
    /// deliberate act — the reason `afd_crypto`'s secret newtypes name theirs
    /// that way.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }

    /// How many bytes were presented. The only thing about it safe to log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.expose().len()
    }

    /// Whether nothing was presented.
    ///
    /// Never true — the constructor refuses it — and present because a type
    /// answering `len` should answer this.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Renders the length, never the value.
impl fmt::Debug for Presented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Presented({} bytes, redacted)", self.len())
    }
}
