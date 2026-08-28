//! The refresh-grant half of the mint: a vaulted refresh token exchanged for a
//! short-lived access token at the provider's token endpoint (RFC 6749 §6).
//!
//! One implementation, three providers. Zoho, Jira and Linear differ by the URL
//! they post to and by nothing else, which is why the endpoint is a field on
//! [`crate::secrets::connector::Exchange`] and there is no per-provider type
//! here — a fourth refresh provider is a row in that table and no code in this
//! file.
//!
//! # What the crates took over
//!
//! `integration_oauth_refresh.zig` is 245 lines, and most of them are
//! re-derivations:
//!
//! - `percentEncode` + `isUnreserved` + `buildForm` — an RFC 3986 encoder and a
//!   format template. `reqwest`'s `form` feature serialises the grant through
//!   `serde_urlencoded`. This matters beyond tidiness: the values are
//!   provider-issued opaque bytes, and a `&` or `=` that escapes its field
//!   changes the SHAPE of the form the token endpoint parses.
//! - `isValidTokenPath` — a hand-written check that a handle's path carries no
//!   query, fragment or whitespace, guarding where platform credentials get
//!   `POST`ed. The URL parser answers that, and answers it the way the client
//!   that will actually dial the address does.
//! - `intValue`'s float arm, with its `MAX_SAFE_FLOAT_I64` guard against
//!   `@intFromFloat` panicking on a hostile provider float. `Duration`'s own
//!   checked constructor rejects the same inputs and cannot trap.
//!
//! # The one thing this refuses that the Zig does not
//!
//! `accounts_base` is a field on the VAULT HANDLE — Zoho's multi-data-centre
//! shape, where a token is redeemable only at the accounts server it was issued
//! by. The Zig shape-checks the path appended to it and never checks the base
//! at all, so a handle written with an attacker's host would POST this
//! deployment's `client_secret` to that host. Here the composed endpoint goes
//! through [`crate::provider::validate_endpoint`] — the same `https`-and-SSRF
//! guard a tenant's `base_url` passes — and a refused one fails the mint
//! rather than dialling.

use afd_core::credential::FIELD_REFRESH_TOKEN;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::credential::platform::OauthApp;
use crate::provider::validate_endpoint;


/// The vault-handle field naming the accounts server this refresh token is
/// redeemable at.
///
/// Zoho multi-data-centre only, and absent for the single-region providers,
/// which fall through to the endpoint their descriptor declares. Refreshing at
/// the wrong data centre's accounts server fails `invalid_grant` exactly as the
/// initial exchange would.
const FIELD_ACCOUNTS_BASE: &str = "accounts_base";

/// The vault-handle field overriding the path appended to
/// [`FIELD_ACCOUNTS_BASE`].
///
/// Per-handle configuration rather than a provider branch — which is the whole
/// reason it is a handle field and not a fourth arm of the exchange enum.
const FIELD_TOKEN_PATH: &str = "token_path";

/// The path appended to an `accounts_base` that names none of its own.
///
/// The Zoho shape, which is the only provider that carries a base at all.
const DEFAULT_TOKEN_PATH: &str = "/oauth/v2/token";

/// The OAuth 2.0 error code meaning the refresh token is dead.
///
/// Revoked, expired, or issued by a different client. A human reconnects; no
/// retry helps, which is why it answers [`Outcome::ReconnectRequired`] rather
/// than a failure — at ANY status, because providers disagree about which one
/// carries it.
const ERROR_INVALID_GRANT: &str = "invalid_grant";

/// The access-token lifetime assumed when the response states none.
///
/// Deliberately short. An assumed lifetime that is too long caches a dead token
/// and a child meets a 401 mid-run; one that is too short re-mints early, which
/// costs a round trip. The floor is the safe direction and it is the Zig's.
const DEFAULT_ACCESS_TTL: Duration = Duration::from_mins(5);

/// The longest `expires_in` this daemon will believe: ten years.
///
/// Past any real access-token lifetime, so a value beyond it is a malformed or
/// hostile body rather than a generous provider — and believing it would park a
/// dead token in the broker's cache for the life of the process.
const MAX_ACCESS_TTL: Duration = Duration::from_hours(10 * 365 * 24);

/// The form body of an RFC 6749 §6 refresh grant.
///
/// Client authentication rides the BODY rather than a `Basic` header, which is
/// what all three declared providers accept and what the Zig posts.
//
// `struct_field_names` is silenced rather than obeyed: `grant_type` is RFC
// 6749's own field name and it is what serde puts on the wire, so renaming it
// to satisfy a lint would mean renaming it BACK with a `#[serde(rename)]`.
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the RFC 6749 form parameters, serialised as written"
)]
#[derive(Serialize)]
struct Grant<'a> {
    /// Always `refresh_token`; the constant is the field's value, so it is
    /// spelled where serde reads it.
    grant_type: &'static str,
    /// The tenant's stored refresh token.
    refresh_token: &'a str,
    /// This deployment's public client half.
    client_id: &'a str,
    /// The half that authenticates the grant. Never logged, never returned.
    client_secret: &'a str,
}

/// What a token endpoint answered, under either a success or a failure status.
///
/// One shape for both because the failure body is read for exactly one field
/// and a second struct would be a second `serde_json::from_slice` over the same
/// bytes. Every field is optional: which are present is what the status and the
/// checks below decide about.
#[derive(Deserialize)]
struct Answered {
    /// The minted credential.
    access_token: Option<String>,
    /// Its lifetime in seconds, however the provider spells a number.
    expires_in: Option<Seconds>,
    /// A replacement refresh token, when the provider rotates.
    refresh_token: Option<String>,
    /// The OAuth error code, on a failure body.
    error: Option<String>,
}

/// An `expires_in`, as the providers actually write it.
///
/// Untagged because this is one field with three renderings and not three
/// fields: a JSON integer, a JSON float, and the string form real OAuth servers
/// emit (`"expires_in":"3600"`). Anything else — an object, an array, a
/// bool — fails the whole deserialisation, which is the malformed-body path.
#[derive(Deserialize)]
#[serde(untagged)]
enum Seconds {
    /// The ordinary rendering.
    Whole(u64),
    /// A provider that wrote a decimal.
    Fractional(f64),
    /// A provider that quoted it.
    Text(String),
}

impl Seconds {
    /// This lifetime as a duration, or `None` when it is not one.
    ///
    /// Negative, non-finite and absurd values all answer `None` and take the
    /// malformed-body path. Nothing here converts a provider-controlled float
    /// with `as`: [`Duration::try_from_secs_f64`] rejects exactly the inputs
    /// that would make such a cast meaningless.
    fn duration(&self) -> Option<Duration> {
        let lifetime = match self {
            Self::Whole(seconds) => Duration::from_secs(*seconds),
            Self::Fractional(seconds) => Duration::try_from_secs_f64(*seconds).ok()?,
            Self::Text(seconds) => Duration::from_secs(seconds.parse().ok()?),
        };
        (lifetime <= MAX_ACCESS_TTL).then_some(lifetime)
    }
}

/// Everything one refresh exchange needs.
#[derive(Debug, Clone, Copy)]
pub struct Refresh<'a> {
    /// This deployment's OAuth client for the connector being minted.
    pub app: &'a OauthApp,
    /// The workspace's stored handle, carrying the refresh token.
    pub handle: &'a Value,
    /// The endpoint the connector's descriptor declares. A handle's own
    /// `accounts_base` overrides it; nothing else can.
    pub token_url: &'a str,
    /// The client the exchange is posted through. Shared and passed in, so the
    /// broker's connection pool and timeout are one decision made once.
    pub http: &'a reqwest::Client,
    /// The instant the token's expiry is measured from.
    pub now_ms: i64,
}

/// Exchanges the handle's refresh token for a fresh access token.
///
/// # Outcomes
///
/// [`Outcome::ReconnectRequired`] when the handle carries no refresh token, and
/// when the provider answers `invalid_grant`. [`Retry::Transient`] on a
/// transport failure, a body that would not arrive, and a 5xx — the request may
/// never have been seen. [`Retry::Permanent`] on a handle that is not an
/// object, an unconfigured platform client, a refused endpoint, a malformed
/// body, and every other 4xx: none of them changes on a retry.
pub async fn mint(refresh: Refresh<'_>) -> Outcome {
    let Some(handle) = refresh.handle.as_object() else {
        return Outcome::MintFailed(Retry::Permanent);
    };
    let Some(refresh_token) = handle.get(FIELD_REFRESH_TOKEN).and_then(Value::as_str) else {
        // A handle with no refresh token is a connection that was removed or
        // never finished. A human reconnects it.
        return Outcome::ReconnectRequired;
    };
    // A base this daemon will not dial, or a path that would move where the
    // client secret is posted, refuses here — nothing is sent.
    let Some(endpoint) = endpoint(handle, refresh.token_url) else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    let response = refresh
        .http
        .post(endpoint.as_ref())
        // `form` sets `application/x-www-form-urlencoded` and encodes every
        // value — see the module header for why that is not ours to write.
        .form(&Grant {
            grant_type: "refresh_token",
            refresh_token,
            client_id: &refresh.app.client_id,
            client_secret: &refresh.app.client_secret,
        })
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await;

    let Ok(response) = response else {
        // Network, DNS, TLS, timeout. Retryable.
        return Outcome::MintFailed(Retry::Transient);
    };
    let status = response.status();
    let Ok(body) = response.bytes().await else {
        // The status arrived and the body did not: the connection dropped
        // mid-response, which is the same class of fault as never reaching it.
        return Outcome::MintFailed(Retry::Transient);
    };

    // A body that does not parse is malformed on a success status, and simply
    // uninformative on a failure one — where the status still classifies.
    let answered = serde_json::from_slice::<Answered>(&body).ok();
    if status.is_success() {
        answered.map_or(Outcome::MintFailed(Retry::Permanent), |answered| {
            granted(&answered, refresh_token, refresh.now_ms)
        })
    } else {
        classify(status, answered.as_ref())
    }
}

/// Where this exchange posts.
///
/// The handle's own accounts server wins over the descriptor's declared
/// endpoint — a refresh token is redeemable at one data centre and posting it
/// at another fails `invalid_grant`. `None` refuses the mint.
///
/// The composed URL is validated even though only the base is tenant-adjacent,
/// because the check is about the address that will be DIALLED and the path is
/// part of composing it.
fn endpoint(handle: &serde_json::Map<String, Value>, declared: &str) -> Option<Box<str>> {
    let Some(base) = handle.get(FIELD_ACCOUNTS_BASE).and_then(Value::as_str) else {
        // No override: the descriptor's endpoint, which is a compile-time
        // constant in this repository and has nothing to validate.
        return Some(declared.into());
    };
    let path = handle
        .get(FIELD_TOKEN_PATH)
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_TOKEN_PATH);
    let composed = format!("{base}{path}");
    // The guard answers `https`, a parseable authority, and a host outside the
    // SSRF ranges. What it does NOT answer is whether the path smuggled a query
    // or a fragment, which is the one thing `isValidTokenPath` was for — asked
    // here of the parse rather than of the bytes.
    let parsed = url::Url::parse(&composed).ok()?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return None;
    }
    validate_endpoint(&composed).ok()?;
    Some(composed.into())
}

/// The outcome of a 2xx body.
///
/// A success status with no `access_token` is a malformed body, not a mint: the
/// one field this exchange exists to obtain is the one that must be there.
fn granted(answered: &Answered, posted: &str, now_ms: i64) -> Outcome {
    let Some(token) = answered.access_token.as_deref() else {
        return Outcome::MintFailed(Retry::Permanent);
    };
    // ABSENT is the conservative default; PRESENT and unreadable is a malformed
    // body, and is never fed into expiry arithmetic.
    let lifetime = match answered.expires_in.as_ref() {
        None => DEFAULT_ACCESS_TTL,
        Some(stated) => {
            let Some(lifetime) = stated.duration() else {
                return Outcome::MintFailed(Retry::Permanent);
            };
            lifetime
        }
    };
    let Ok(lifetime_ms) = i64::try_from(lifetime.as_millis()) else {
        return Outcome::MintFailed(Retry::Permanent);
    };

    Outcome::Ok(Minted {
        token: Zeroizing::new(token.to_owned()),
        expires_at_ms: now_ms.saturating_add(lifetime_ms),
        rotated_refresh_token: rotated(answered, posted),
    })
}

/// The replacement refresh token, when the provider genuinely issued one.
///
/// Deduplicated HERE because this is the only place holding both the posted and
/// the returned value, which keeps the vault write-back a caller performs
/// unconditional on `Some`. An echo of what was posted is not a rotation, and an
/// EMPTY replacement is a broken provider or proxy — writing either back would
/// replace a working handle with one that mints nothing.
fn rotated(answered: &Answered, posted: &str) -> Option<Zeroizing<String>> {
    answered
        .refresh_token
        .as_deref()
        .filter(|issued| !issued.is_empty() && *issued != posted)
        .map(|issued| Zeroizing::new(issued.to_owned()))
}

/// The outcome of a non-2xx answer.
///
/// `invalid_grant` at ANY status is a reconnect: providers return it under 400
/// and 401 both, and what it means does not depend on which. Everything else
/// splits the way the GitHub mint's does — the vendor's fault retries, the
/// request's does not.
fn classify(status: reqwest::StatusCode, answered: Option<&Answered>) -> Outcome {
    let invalid_grant = answered
        .and_then(|answered| answered.error.as_deref())
        .is_some_and(|error| error == ERROR_INVALID_GRANT);
    if invalid_grant {
        return Outcome::ReconnectRequired;
    }
    if status.is_server_error() {
        Outcome::MintFailed(Retry::Transient)
    } else {
        Outcome::MintFailed(Retry::Permanent)
    }
}

#[cfg(test)]
mod tests;
