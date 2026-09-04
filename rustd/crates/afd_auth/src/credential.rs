//! What was presented, and which class it belongs to.
//!
//! This module holds the routing decision that
//! the retired daemon's `auth/middleware/bearer_or_api_key.zig:74-124` spells as a
//! chain of `if`, and it is the reason that chain does not survive the port.
//!
//! # The chain was never the problem
//!
//! Deleting the `if` and leaving everything else would buy nothing. The chain
//! exists because the three credential classes it routes to are three
//! hand-written procedures that differ only in constants — so routing to them
//! has to be hand-written too, and has to be kept in the same order as a
//! paragraph of `docs/AUTH.md`.
//!
//! [`CredentialKind::of`] is a walk over a table. There is no branch to put in
//! the wrong order because there is no branch, and a new class is one row.
//!
//! # The property the chain holds by accident
//!
//! `agt_t` and `agt_r` differ in one byte. Nothing in the Zig chain says two
//! markers must not be prefixes of one another — it holds because of the order
//! somebody wrote the branches in, and a future `agt_` class would break it
//! silently by being tested first. The table is asserted PREFIX-FREE at compile
//! time, so the property is a build failure rather than a review question.
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

/// the retired daemon's `auth/middleware/tenant_api_key.zig`'s `TENANT_KEY_PREFIX`.
pub const TENANT_API_KEY_PREFIX: &str = "agt_t";
/// the retired daemon's `auth/cli_credential.zig`'s `PREFIX`.
pub const CLI_CREDENTIAL_PREFIX: &str = "afc_";
/// `src/lib/contract/protocol.zig`'s `RUNNER_TOKEN_PREFIX`.
pub const RUNNER_TOKEN_PREFIX: &str = "agt_r";

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
/// Matched case-SENSITIVELY, as `bearer.zig`'s
/// `startsWith(u8, auth, "Bearer ")` does. Leniency here would accept a header
/// the Zig daemon refuses, which is a behaviour divergence and therefore a bug.
const BEARER_SCHEME: &str = "Bearer ";

/// The bytes `bearer.zig` trims before deciding a token is blank.
const BLANK_BYTES: [char; 4] = [' ', '\t', '\r', '\n'];

/// A credential exactly as the caller sent it.
///
/// Guards two invariants (`M-STRONG-TYPES-GUARD`), and neither is decorative:
///
/// - **Non-blank.** `bearer.zig` answers `null` for a header whose token is
///   empty or all whitespace, so every caller maps that to one 401 branch.
///   Refusing it in the constructor means no later code has to remember.
/// - **Never rendered.** `Debug` prints the length and nothing else. A bearer
///   value in a log IS the credential, and a `#[derive(Debug)]` on any struct
///   that transitively holds one is how it gets there.
///
/// The value is stored untrimmed. `parseBearerToken` returns the raw slice and
/// only trims to decide blankness, so trimming here would hash different bytes
/// than the Zig daemon hashes.
#[derive(Clone, PartialEq, Eq)]
pub struct Presented(Box<str>);

/// A presented credential that carried nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a presented credential must not be blank")]
pub struct Blank;

impl Presented {
    /// Wraps the token half of an `Authorization: Bearer <token>` header.
    ///
    /// # Errors
    /// Returns [`Blank`] when the value is empty or only whitespace — the
    /// condition `bearer.zig`'s `parseBearerToken` answers `null` for.
    pub fn new(raw: &str) -> Result<Self, Blank> {
        if raw.trim_matches(BLANK_BYTES).is_empty() {
            return Err(Blank);
        }
        Ok(Self(raw.into()))
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
        &self.0
    }

    /// How many bytes were presented. The only thing about it safe to log.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether nothing was presented.
    ///
    /// Never true — the constructor refuses it — and present because a type
    /// answering `len` should answer this.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Renders the length, never the value.
impl fmt::Debug for Presented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Presented({} bytes, redacted)", self.0.len())
    }
}

/// Zeroes the credential when the request that carried it ends (Invariant 5).
///
/// A `Box<str>` is freed, not overwritten, so without this the bytes sit in
/// released heap for as long as the allocator leaves them there. Hand-written
/// rather than derived for the reason the workspace `Cargo.toml` records:
/// `zeroize_derive` is the last crate in this graph on `syn` 2, and a duplicate
/// `syn` fails `clippy::multiple_crate_versions`.
impl Drop for Presented {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        // Take the buffer so the write happens on memory nothing can observe
        // afterwards, and go through the byte view because `Box<str>` has no
        // mutable one. `zeroize` rather than a loop so the write is not
        // eliminated as dead.
        let mut bytes = std::mem::take(&mut self.0).into_boxed_bytes();
        bytes.zeroize();
    }
}
