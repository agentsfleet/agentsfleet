//! One procedure, three credential classes, and a table for the differences.
//!
//! `tenant_api_key.zig`, `cli_credential.zig` and `runner_bearer.zig` are the
//! same procedure written three times — hash, look up, check liveness, resolve
//! capability, build a principal. Everything that differs between them is a
//! CONSTANT, and every constant is buried inside a hand-written body where
//! nothing can see that its neighbour disagrees.
//!
//! That is not a hypothetical. `cli_credential.zig` shape-checks its value
//! before hashing so a truncated paste costs no round trip; the other two do
//! not, and no comment anywhere says why. The asymmetry is invisible because
//! the three bodies are never read side by side.
//!
//! Here the differences are [`HashedClass`] constants and the procedure is
//! [`Registry::authenticate`]. A class cannot acquire a shape check its
//! neighbours lack, because there is nowhere to write one.
//!
//! # What the registry is, and why it is not a `Vec<Box<dyn _>>`
//!
//! `~/Projects/oss/core_api-develop`'s `lib-auth` supplies the vocabulary this
//! module uses — its `FlowDelegate { opens_door, subject_is_present }` is this
//! crate's `kind`/`authenticate`, and its `lib-auth`/`api-auth` split is the
//! Rust spelling of Zig's `make test-auth` portability wall. Both are adopted.
//!
//! Its `FlowBuilder` is not. That registry is a `Vec<Box<dyn FlowDelegate>>`
//! scanned by equality (`flow.rs:58-64`), so a door nobody registered resolves
//! to `None` and becomes a 401 at run time. Add a credential class, miss one
//! `raw_backend(…)` line at boot, and every request of that class reports
//! "invalid token" — a wiring bug wearing an authentication error's clothes,
//! findable only in production.
//!
//! Dispatch here is an exhaustive `match` on [`CredentialKind`]. A new class
//! fails the BUILD until it is wired, which is the guarantee
//! [`crate::scope::Scope`] already gets from `wire()`/`bit()`, and the reason
//! the escape hatch is the only place dynamism is allowed to enter.

use crate::capability::CapabilitySource;
use crate::credential::{CredentialKind, Presented};
use crate::directory::{CredentialDirectory, CredentialRecord, Digest, Liveness};
use crate::error::AuthError;
use crate::plane::Plane;
use crate::principal::{Person, PersonCredential, Principal, Runner};
use crate::scope::parse_claim;
use crate::verifier::{TokenVerifier, VerifyError};

/// Hex characters in every stored credential's body.
///
/// All three minters draw 32 bytes and render them lower-case
/// (`cli_credential.zig::RANDOM_BYTES`, `api_keys/tenant.zig::KEY_RANDOM_BYTES`,
/// `runner/register.zig::TOKEN_RANDOM_BYTES`), so the number is one fact and
/// not three (RULE UFS).
const BODY_HEX_LEN: usize = 64;

/// The person-credential classes a STORED credential can produce.
///
/// A session token is absent and cannot be added: it is verified rather than
/// looked up, so no [`HashedClass`] could name one. That narrowing is also what
/// keeps [`HashedClass`] `Copy` — a session token carries a workspace ceiling,
/// and a ceiling is a `Uuid7`, which is a boxed string. The type system and the
/// domain agree here, which is usually the sign the type is the right one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredPersonCredential {
    /// `agt_t`, resolving to the person named in the row's `created_by`.
    TenantApiKey,
    /// `afc_`, resolving to the person who ran `agentsfleet login`.
    CliCredential,
}

impl StoredPersonCredential {
    /// The principal-facing credential class.
    const fn into_person(self) -> PersonCredential {
        match self {
            Self::TenantApiKey => PersonCredential::TenantApiKey,
            Self::CliCredential => PersonCredential::CliCredential,
        }
    }
}

/// Everything that distinguishes one stored-credential class from another.
///
/// A new class is a new const of this type. There is no procedure to copy, so
/// there is no procedure to copy WRONGLY.
#[derive(Debug, Clone, Copy)]
struct HashedClass {
    /// The class this describes.
    kind: CredentialKind,
    /// The marker this class's values carry.
    ///
    /// Held here as well as on [`CredentialKind`] so the shape check needs no
    /// `Option`: a stored class ALWAYS has a marker, and asking a type that can
    /// answer `None` would add a branch nothing can reach.
    prefix: &'static str,
    /// Hex characters expected after the marker.
    body_hex_len: usize,
    /// What a person authenticated this way holds as their credential class,
    /// or `None` when the class names a machine.
    person_credential: Option<StoredPersonCredential>,
    /// The answer when nothing matches the digest, or the value is malformed.
    unknown: AuthError,
    /// The answer when the row exists and is no longer live.
    revoked: AuthError,
}

/// `agt_t` — resolves to the person named in the row's `created_by`.
const TENANT_API_KEY: HashedClass = HashedClass {
    kind: CredentialKind::TenantApiKey,
    prefix: crate::credential::TENANT_API_KEY_PREFIX,
    body_hex_len: BODY_HEX_LEN,
    person_credential: Some(StoredPersonCredential::TenantApiKey),
    unknown: AuthError::InvalidOrMissingToken,
    revoked: AuthError::TenantKeyRevoked,
};

/// `afc_` — resolves to the person who ran `agentsfleet login`.
const CLI_CREDENTIAL: HashedClass = HashedClass {
    kind: CredentialKind::CliCredential,
    prefix: crate::credential::CLI_CREDENTIAL_PREFIX,
    body_hex_len: BODY_HEX_LEN,
    person_credential: Some(StoredPersonCredential::CliCredential),
    unknown: AuthError::InvalidOrMissingToken,
    revoked: AuthError::CliCredentialRevoked,
};

/// `agt_r` — resolves to a machine, whose capabilities are derived, not asked.
const RUNNER_TOKEN: HashedClass = HashedClass {
    kind: CredentialKind::RunnerToken,
    prefix: crate::credential::RUNNER_TOKEN_PREFIX,
    body_hex_len: BODY_HEX_LEN,
    person_credential: None,
    unknown: AuthError::InvalidRunnerToken,
    revoked: AuthError::RunnerStateBlocked,
};

impl HashedClass {
    /// The class describing `kind`, or `None` for the class that is verified
    /// rather than looked up.
    ///
    /// Exhaustive: a new [`CredentialKind`] fails to compile here until it has
    /// declared whether it is stored, which is the question whose wrong answer
    /// would route a credential to a path that cannot prove it.
    const fn of(kind: CredentialKind) -> Option<Self> {
        match kind {
            CredentialKind::TenantApiKey => Some(TENANT_API_KEY),
            CredentialKind::CliCredential => Some(CLI_CREDENTIAL),
            CredentialKind::RunnerToken => Some(RUNNER_TOKEN),
            CredentialKind::OidcSessionToken => None,
        }
    }

    /// Whether `presented` has this class's shape: exact length, and a body of
    /// lower-case hexadecimal after the marker.
    ///
    /// Mirrors `cli_credential.zig::looksWellFormed`, applied to all three
    /// classes rather than one. It changes no verdict — a malformed value
    /// matches no row and answers the same code either way — and saves the
    /// round trip that could only have said so.
    ///
    /// The marker itself is NOT re-checked. [`CredentialKind::of`] established
    /// it, which is what selected this class; checking it again would add a
    /// branch no input can take, and an unreachable branch is worse than the
    /// duplication it avoids.
    fn accepts_shape(self, presented: &Presented) -> bool {
        let raw = presented.expose().as_bytes();
        raw.len() == self.prefix.len() + self.body_hex_len
            && raw
                .iter()
                .skip(self.prefix.len())
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    }
}

/// The credential classes a group of routes accepts, and what proves them.
///
/// # Type parameters
///
/// Three, flat and differently-roled, and none of them nests: `D` looks a
/// digest up, `C` answers what a subject may do, `V` verifies a signed token.
/// `M-DI-HIERARCHY` puts generics above `dyn Trait`, and `M-SIMPLE-ABSTRACTIONS`
/// tolerates one level — which is what this is. The request path costs no
/// allocation and no virtual call as a result.
///
/// `V` defaults to [`crate::verifier::NoVerifier`], so a deployment with no
/// identity provider names no type and gets the documented behaviour: the
/// prefixed classes still resolve, and the session-token class refuses.
#[derive(Debug, Clone)]
pub struct Registry<D, C, V = crate::verifier::NoVerifier> {
    plane: Plane,
    directory: D,
    capabilities: C,
    verifier: V,
}

impl<D, C, V> Registry<D, C, V>
where
    D: CredentialDirectory,
    C: CapabilitySource,
    V: TokenVerifier,
{
    /// Builds the registry a plane's routes authenticate against.
    pub const fn new(plane: Plane, directory: D, capabilities: C, verifier: V) -> Self {
        Self {
            plane,
            directory,
            capabilities,
            verifier,
        }
    }

    /// Authenticates a whole `Authorization` header value.
    ///
    /// # Errors
    /// The plane's own refusal when the header is absent, not a `Bearer`, or
    /// carries a blank token — one branch, as `bearer.zig` intends, and the
    /// same one a wrong-class credential lands in, so a caller cannot tell the
    /// two apart.
    pub async fn authenticate_header(&self, header: &str) -> Result<Principal, AuthError> {
        let presented = Presented::from_authorization(header).map_err(|_blank| self.refusal())?;
        self.authenticate(&presented).await
    }

    /// Authenticates a credential already parsed out of its header.
    ///
    /// # Errors
    /// [`AuthError`], carrying the registry code and the client-visible detail.
    pub async fn authenticate(&self, presented: &Presented) -> Result<Principal, AuthError> {
        let kind = CredentialKind::of(presented);
        if !self.plane.admits(kind) {
            return Err(self.refusal());
        }
        // The total dispatch. A new `CredentialKind` fails to compile here, and
        // that is deliberately the ONLY place in the crate where adding a class
        // is felt.
        match HashedClass::of(kind) {
            Some(class) => self.authenticate_stored(class, presented).await,
            None => self.authenticate_token(presented).await,
        }
    }

    /// How this plane refuses something it will not consider.
    const fn refusal(&self) -> AuthError {
        self.plane.refusal()
    }

    /// The one stored-credential procedure, driven by `class`.
    async fn authenticate_stored(
        &self,
        class: HashedClass,
        presented: &Presented,
    ) -> Result<Principal, AuthError> {
        if !class.accepts_shape(presented) {
            return Err(class.unknown);
        }
        let digest = Digest::of(presented);
        let found = self.directory.resolve(class.kind, &digest).await?;
        let record = found.ok_or(class.unknown)?;
        match record {
            CredentialRecord::Machine {
                runner,
                degraded,
                live,
            } => {
                if live == Liveness::Revoked {
                    return Err(class.revoked);
                }
                // A machine's capabilities are DERIVED from the variant, so
                // there is no provider to ask and no assignment to widen.
                match class.person_credential {
                    None => Ok(Principal::Runner(Runner::new(runner, degraded))),
                    // The directory answered with a shape this class cannot
                    // produce. Fail closed rather than mint a runner principal
                    // from a person's credential.
                    Some(_person) => Err(class.unknown),
                }
            }
            CredentialRecord::Person {
                tenant,
                subject,
                live,
            } => {
                if live == Liveness::Revoked {
                    return Err(class.revoked);
                }
                let credential = class.person_credential.ok_or(class.unknown)?.into_person();
                // The credential proved WHO. The provider answers WHAT, per
                // request — so narrowing someone reaches every credential they
                // hold without a deploy and without a backfill.
                let scopes = self.capabilities.capabilities(&subject).await?;
                Ok(Principal::Person(Person::new(
                    credential, tenant, subject, scopes,
                )))
            }
        }
    }

    /// The session-token path: verify, then read the claim off the token.
    ///
    /// The one class that consults no [`CapabilitySource`], because its
    /// capability claim rides on the credential itself.
    async fn authenticate_token(&self, presented: &Presented) -> Result<Principal, AuthError> {
        let claims = match self.verifier.verify(presented).await {
            Ok(claims) => claims,
            Err(err) => return Err(Self::redact(err, self.refusal())),
        };
        // The same parser all three credential shapes feed, so they cannot
        // drift in how a claim string becomes a capability set. An absent claim
        // is the empty set, which every non-empty requirement refuses.
        let scopes = claims
            .scope_claim
            .as_deref()
            .map_or(crate::scope::ScopeSet::EMPTY, parse_claim);
        let tenant = claims.tenant.ok_or_else(|| self.refusal())?;
        Ok(Principal::Person(Person::new(
            PersonCredential::SessionToken {
                workspace_scope: claims.workspace_scope,
            },
            tenant,
            claims.subject,
            scopes,
        )))
    }

    /// Turns a verifier's honest account into what a client is told.
    ///
    /// The single boundary between [`VerifyError`] and [`AuthError`], which is
    /// what keeps "which failure leaks what" out of every verifier
    /// implementation. Two mappings survive the redaction and both are Zig
    /// parity: expiry keeps its own code because it leaks nothing and its
    /// remedy differs, and a key-set failure is an outage rather than a
    /// rejection because it is not evidence about the caller's token.
    const fn redact(err: VerifyError, fallback: AuthError) -> AuthError {
        match err {
            VerifyError::Expired => AuthError::TokenExpired,
            _ if err.is_provider_fault() => AuthError::Unavailable,
            _ => fallback,
        }
    }
}
