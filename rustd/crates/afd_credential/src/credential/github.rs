//! The GitHub App half of the mint: an App JWT exchanged for a scoped
//! installation token.
//!
//! # What `octocrab` does, and the one thing it must not be asked to do
//!
//! The App JWT — `iss`, `iat`, `exp`, the RS256 signature, the refresh before
//! expiry — is `octocrab`'s, and so is the transport. None of that is worth
//! owning.
//!
//! Its `installation_and_token()` is NOT used, and the reason is a security
//! one rather than a stylistic one: that method posts a hardcoded `{}` body,
//! and an empty body means the installation's FULL scope. It is also the only
//! place in the crate that reaches this endpoint, so there is no scoped variant
//! to prefer. The narrowing this milestone rests on is sent through the generic
//! `post`, with [`ScopedRequest`] as the body.
//!
//! # And why the RESPONSE is not read through `octocrab::models` either
//!
//! `InstallationPermissions` is a struct of seven named fields with no
//! catch-all, and `pull_requests` is not one of them. Deserialising into it
//! would do two harmful things at once: make the permission a write mint
//! REQUESTS unreadable, and silently DROP any permission GitHub granted that
//! the struct does not model. A check written against it would pass while the
//! token carried `administration: write`, which is the exact overreach the
//! check exists to refuse — the typed model would make it invisible.
//!
//! So the response is read into [`Granted`], whose permission map is open.
//! An unmodelled permission arrives as [`Permission::Unknown`], which is
//! greater than [`Permission::Read`] and therefore refused.

use std::collections::{BTreeMap, HashSet};

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use octocrab::Octocrab;
use octocrab::models::AppId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::credential::platform::GithubApp;

/// The permission a repository-scoped mint asks for on `contents`.
///
/// Read and write both ask for it; the level is what differs, and it is the
/// level the response is checked against.
const PERMISSION_CONTENTS: &str = "contents";

/// The permission a WRITE mint additionally asks for.
///
/// Its absence is the read scope, which is why a read mint sends no entry for
/// it rather than sending one set to read.
const PERMISSION_PULL_REQUESTS: &str = "pull_requests";

/// How far one permission reaches.
///
/// Ordered, and that ordering is the whole check: "granted more than was asked
/// for" is `granted > requested`, a comparison the compiler derives, rather
/// than a chain of string equalities that has to be read to be believed.
///
/// [`Self::Unknown`] is last on purpose. A level GitHub introduces after this
/// was written sorts ABOVE write, so it is refused rather than admitted — an
/// unrecognised permission level must never be the permissive branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Fetch, and nothing more.
    Read,
    /// What opening a draft Pull Request needs.
    Write,
    /// Administrative reach. Never requested by this daemon.
    Admin,
    /// A level this daemon does not model.
    ///
    /// `#[serde(other)]` is what makes an unknown level VISIBLE. Without it a
    /// value outside this enum fails the whole deserialisation, and a caller
    /// that treated a parse failure as "no permissions granted" would be
    /// admitting the token it could not read.
    #[serde(other)]
    Unknown,
}

/// The body that narrows an installation token to one fleet's declared reach.
///
/// # Repositories go by BARE name, and that is GitHub's rule, not a choice
///
/// GitHub scopes an installation token by repository name WITHIN the
/// installation's own account, so the owner never reaches the wire. A binding
/// naming `acme/payments` is therefore sent as `payments`, and GitHub will
/// happily grant `<installed-account>/payments` if a repository by that bare
/// name exists there. It cannot cross a tenant — an installation belongs to one
/// account — but it is a real mis-scope inside an operator's own installation,
/// and nothing on the request side can prevent it.
///
/// That is why [`Granted::verify`] exists and why it checks the RESPONSE.
#[derive(Debug, Serialize)]
pub struct ScopedRequest {
    /// Bare repository names, owner stripped.
    repositories: Vec<String>,
    /// Exactly the permissions this access level needs, and no others.
    permissions: BTreeMap<&'static str, Permission>,
}

impl ScopedRequest {
    /// The narrowest request that satisfies `binding`.
    #[must_use]
    pub fn for_binding(binding: &RepositoryBinding) -> Self {
        let contents = match binding.access() {
            Access::Read => Permission::Read,
            Access::Write => Permission::Write,
        };
        let mut permissions = BTreeMap::from([(PERMISSION_CONTENTS, contents)]);
        if binding.access() == Access::Write {
            // Asked for ONLY at write. A read mint sends no entry at all,
            // because the absence is the read scope — sending
            // `pull_requests: read` would request a grant the fleet did not
            // declare and then have to explain it in the response check.
            permissions.insert(PERMISSION_PULL_REQUESTS, Permission::Write);
        }
        Self {
            repositories: binding
                .repositories()
                .iter()
                .map(|repository| bare_name(repository))
                .collect(),
            permissions,
        }
    }

    /// What this request asked for, for the response to be checked against.
    #[must_use]
    pub const fn permissions(&self) -> &BTreeMap<&'static str, Permission> {
        &self.permissions
    }
}

/// The bare repository name GitHub scopes by, from a qualified `owner/name`.
///
/// Splits on the LAST separator: a repository name cannot contain one, so
/// whatever follows it is the name even when an owner does something unusual.
fn bare_name(qualified: &str) -> String {
    qualified
        .rsplit_once('/')
        .map_or_else(|| qualified.to_owned(), |(_owner, name)| name.to_owned())
}

/// One repository a minted token turned out to reach.
#[derive(Debug, Deserialize)]
struct Reached {
    /// The QUALIFIED `owner/name`, which is what makes the check meaningful:
    /// the request could only name the bare half.
    full_name: String,
}

/// What GitHub actually minted.
///
/// Deserialised into this rather than `octocrab::models::InstallationToken` —
/// see the module header for why that model cannot be used safely here.
#[derive(Debug, Deserialize)]
pub struct Granted {
    /// The installation token itself.
    pub token: String,
    /// When it stops working, as GitHub spells it (RFC 3339).
    pub expires_at: Option<String>,
    /// Every permission granted, INCLUDING ones this daemon does not model.
    #[serde(default)]
    permissions: BTreeMap<String, Permission>,
    /// The repositories the token reaches.
    ///
    /// `Option`, and a `None` is refused rather than assumed: a request that
    /// named repositories is answered with the ones it was granted, so a
    /// missing array means something happened this code does not model.
    repositories: Option<Vec<Reached>>,
}

/// Why a minted token was refused after the exchange.
///
/// Separate from a mint FAILURE: the exchange succeeded and GitHub handed back
/// a working credential. What failed is that the credential does not match what
/// the fleet declared, so it is discarded rather than delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overreach {
    /// The token reaches a repository that was never declared, or misses one
    /// that was — the bare-name mis-scope this check exists for.
    Repositories,
    /// The token carries a permission, or a level, beyond what was asked for.
    Permissions,
    /// The response described no reach at all.
    ///
    /// Refused rather than assumed, for the reason [`Self::Repositories`] is:
    /// unknown reach must never be the permissive branch.
    Unstated,
}

impl Granted {
    /// Whether this token reaches exactly what was declared, and no further.
    ///
    /// # Errors
    /// Refuses a token whose reach or permissions exceed — or fall short of —
    /// the declaration, and one that states neither.
    pub fn verify(
        &self,
        binding: &RepositoryBinding,
        requested: &BTreeMap<&'static str, Permission>,
    ) -> Result<(), Overreach> {
        self.verify_repositories(binding)?;
        self.verify_permissions(requested)
    }

    /// Set equality between what was declared and what was granted.
    ///
    /// Both directions in one comparison, because both matter: a token reaching
    /// MORE than was declared is the mis-scope, and one reaching LESS fails
    /// later at the vendor where the fleet has no local explanation for it.
    /// Case-folded because GitHub owners and repository names are.
    fn verify_repositories(&self, binding: &RepositoryBinding) -> Result<(), Overreach> {
        let Some(reached) = self.repositories.as_ref() else {
            return Err(Overreach::Unstated);
        };
        let reached: HashSet<String> = reached
            .iter()
            .map(|repository| repository.full_name.to_lowercase())
            .collect();
        let declared: HashSet<String> = binding
            .repositories()
            .iter()
            .map(|repository| repository.to_lowercase())
            .collect();
        if reached == declared {
            Ok(())
        } else {
            Err(Overreach::Repositories)
        }
    }

    /// Every granted permission is either an ambient read, or exactly what was
    /// asked for.
    ///
    /// GitHub attaches read-level grants of its own — `metadata` rides on every
    /// installation token — so a read-level extra is expected and passes.
    /// Anything above read that was not requested, at any name this daemon does
    /// or does not model, is refused.
    fn verify_permissions(
        &self,
        requested: &BTreeMap<&'static str, Permission>,
    ) -> Result<(), Overreach> {
        if self.permissions.is_empty() {
            return Err(Overreach::Unstated);
        }
        // Nothing granted beyond an ambient read except exactly what was asked
        // for, at exactly the level it was asked for.
        let within_request = self.permissions.iter().all(|(name, granted)| {
            *granted <= Permission::Read || requested.get(name.as_str()) == Some(granted)
        });
        // And everything asked for was actually granted, so a token quietly
        // NARROWER than the declaration is refused here rather than at the
        // vendor.
        let fully_granted = requested
            .iter()
            .all(|(name, want)| self.permissions.get(*name) == Some(want));
        if within_request && fully_granted {
            Ok(())
        } else {
            Err(Overreach::Permissions)
        }
    }
}

#[cfg(test)]
mod tests;

/// The vault-handle field naming the App installation to mint for.
const FIELD_INSTALLATION_ID: &str = "installation_id";

/// How long an installation token lasts, per GitHub's documentation.
///
/// Bounded locally rather than read from the response's `expires_at`, which is
/// `integration_github.zig`'s decision and is carried over deliberately. It
/// holds only while GitHub never issues a SHORTER one: were that to change,
/// this would cache a token past its death rather than re-mint early, and a
/// child would meet a 401 mid-run. Recorded here because the risk is invisible
/// at the call site (Indy vetoed narrowing it, this stream).
const INSTALLATION_TOKEN_TTL_MS: i64 = 60 * 60 * 1000;

/// Everything one GitHub mint needs.
#[derive(Debug, Clone, Copy)]
pub struct Exchange<'a> {
    /// This deployment's App — the signing key never leaves the process.
    pub app: &'a GithubApp,
    /// The workspace's stored handle, naming which installation to mint for.
    pub handle: &'a Value,
    /// The fleet's declared reach. `None` refuses BEFORE anything is signed.
    pub binding: Option<&'a RepositoryBinding>,
    /// The instant the token's expiry is measured from.
    pub now_ms: i64,
}

/// Mints a repository-scoped installation token.
///
/// # The order of the checks is the security property
///
/// The binding is required BEFORE the JWT is built and before any request is
/// sent. A missing declaration cannot reach the exchange, because the body that
/// would carry it is what narrows the token: GitHub reads an empty body as the
/// installation's FULL permission set across EVERY repository it covers, for an
/// hour. So the absence of a declaration refuses rather than widens — the
/// direction `integration_github.zig` also fails in, and the one that matters.
pub async fn mint(exchange: Exchange<'_>) -> Outcome {
    let Some(installation_id) = exchange
        .handle
        .as_object()
        .and_then(|handle| handle.get(FIELD_INSTALLATION_ID))
        .and_then(installation_id)
    else {
        // A handle with no installation is a connection that was removed or
        // never finished, which a human reconnects — not a failure to retry.
        return Outcome::ReconnectRequired;
    };
    let Some(binding) = exchange.binding else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    let key = match jsonwebtoken::EncodingKey::from_rsa_pem(exchange.app.private_key_pem.as_bytes())
    {
        Ok(key) => key,
        // The platform's own key does not parse. Permanent, and an operator's
        // to fix — no tenant can act on it, and no retry will change it.
        Err(_unusable) => return Outcome::MintFailed(Retry::Permanent),
    };
    let Ok(client) = Octocrab::builder()
        .app(AppId(exchange.app.app_id), key)
        .build()
    else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    let request = ScopedRequest::for_binding(binding);
    // `post`, NOT `installation_and_token()`. That method sends a hardcoded
    // `{}` and would mint the installation's full scope — see the module
    // header. This is the one call that carries the narrowing.
    let granted: Granted = match client
        .post(
            format!("/app/installations/{installation_id}/access_tokens"),
            Some(&request),
        )
        .await
    {
        Ok(granted) => granted,
        Err(error) => return classify(&error),
    };

    if let Err(overreach) = granted.verify(binding, request.permissions()) {
        // The exchange SUCCEEDED and the token is real; it simply reaches
        // further, or less far, than the fleet declared. It is dropped here
        // rather than delivered, and the token never leaves this function.
        tracing::warn!(
            ?overreach,
            event = "github_mint_overreach",
            "discarding a GitHub token whose reach does not match the fleet's binding"
        );
        return Outcome::MintFailed(Retry::Permanent);
    }

    Outcome::Ok(Minted {
        token: granted.token.into(),
        expires_at_ms: exchange.now_ms.saturating_add(INSTALLATION_TOKEN_TTL_MS),
        // An App installation token has nothing to rotate: the App key is the
        // long-lived credential and it never leaves this deployment, so there
        // is no per-tenant secret for GitHub to replace.
        rotated_refresh_token: None,
    })
}

/// The installation id, however the handle spells it.
///
/// GitHub's id is numeric and a stored handle may carry it either as a JSON
/// number or as the string a form posted. Both are accepted, and anything else
/// is not — a handle whose installation is an object or an array names no
/// installation at all.
fn installation_id(stored: &Value) -> Option<u64> {
    match stored {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _not_an_id => None,
    }
}

/// Sorts an exchange failure into what the caller should do about it.
///
/// A 401 or 404 is the installation being gone, which is a reconnect. Anything
/// at or above 500 is the vendor, which is worth retrying. Every other status,
/// and every failure to read what came back, is permanent — the same split
/// `integration_github.zig` makes, expressed against `octocrab`'s typed status
/// rather than a raw `u16`.
fn classify(error: &octocrab::Error) -> Outcome {
    let octocrab::Error::GitHub { source, .. } = error else {
        // No status at all: a transport failure, a timeout, a body that would
        // not decode. Transient, because the request may never have been seen.
        return Outcome::MintFailed(Retry::Transient);
    };
    match source.status_code {
        http::StatusCode::UNAUTHORIZED | http::StatusCode::NOT_FOUND => Outcome::ReconnectRequired,
        status if status.is_server_error() => Outcome::MintFailed(Retry::Transient),
        _client_error => Outcome::MintFailed(Retry::Permanent),
    }
}
