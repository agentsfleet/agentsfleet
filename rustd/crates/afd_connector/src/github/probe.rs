//! The two calls this connect spends its user token on.
//!
//! Split from the module root because the halves fail differently and are
//! tested differently: everything there is a pure function over a body already
//! in hand, and everything here is a request that can time out, be refused, or
//! answer something this build cannot read. Keeping them in one file meant the
//! parse tests and the vendor's status mapping shared a length budget, and the
//! status mapping is the half that decides what a person is told.
//!
//! Every host goes through [`crate::endpoint::redirected`], so a lane that
//! pointed the token exchange at a fake vendor gets these too.

use serde_json::Value;

use super::{Found, Installation, parse_listing};
use crate::endpoint;
use crate::error::{self, Result};

/// The installations the authorized person can reach, at most two asked for.
///
/// Two is the bound because the answer is a count of three states — none, one,
/// several — and a third row would never change it.
const USER_INSTALLATIONS: &str = "https://api.github.com/user/installations?per_page=2";

/// One repository of a claimed installation: a 200 proves the token opens it.
const INSTALLATION_REPOSITORIES: &str = "https://api.github.com/user/installations";
/// See [`INSTALLATION_REPOSITORIES`].
const REPOSITORIES_PROBE_SUFFIX: &str = "/repositories?per_page=1";

/// The REST version every call names, so a vendor default change cannot move
/// the shape of the answer under this parse.
const HEADER_API_VERSION: &str = "x-github-api-version";
/// See [`HEADER_API_VERSION`].
const API_VERSION: &str = "2022-11-28";
/// See [`HEADER_API_VERSION`].
const HEADER_AUTHORIZATION: &str = "authorization";
/// See [`HEADER_API_VERSION`].
const BEARER_PREFIX: &str = "Bearer ";
/// See [`HEADER_API_VERSION`].
const HEADER_ACCEPT: &str = "accept";
/// See [`HEADER_API_VERSION`].
const CONTENT_TYPE_JSON: &str = "application/vnd.github+json";

/// The vendor answers that mean "this token does not open that installation",
/// as opposed to "the vendor could not say".
///
/// 404 is GitHub's answer for an installation the token cannot see at all, and
/// 401/403 for one it carries no authorization against. Every OTHER refusal is
/// the vendor having a bad minute, which is a different sentence to a person
/// and a different code on the wire — see [`opens`].
const STATUS_UNAUTHORIZED: u16 = 401;
/// See [`STATUS_UNAUTHORIZED`].
const STATUS_FORBIDDEN: u16 = 403;
/// See [`STATUS_UNAUTHORIZED`].
const STATUS_NOT_FOUND: u16 = 404;

/// Resolves the installation this connect binds.
///
/// A claimed id is probed and taken; with no claim the listing decides.
///
/// # Errors
/// Reports a vendor that could not be reached, one that answered and refused,
/// and an answer this build cannot read. A claim the token does not open, an
/// empty listing and a listing of several are [`Found`] states the caller
/// refuses under the ownership code — see the module note.
pub async fn resolve(
    client: &reqwest::Client,
    pinned: Option<&str>,
    token: &str,
    claimed: Option<&str>,
) -> Result<Found> {
    if let Some(id) = claimed {
        return Ok(if opens(client, pinned, token, id).await? {
            Found::One(Installation {
                id: id.to_owned(),
                account: None,
            })
        } else {
            Found::None
        });
    }
    let listed = fetch(client, pinned, token, USER_INSTALLATIONS).await?;
    let status = listed.status();
    if !status.is_success() {
        return Err(error::exchange_refused(status.as_u16()));
    }
    let body: Value = serde_json::from_str(&listed.text().await?)
        .map_err(|_unreadable| error::exchange_unreadable())?;
    parse_listing(&body).ok_or_else(error::exchange_unreadable)
}

/// Whether the token opens the claimed installation.
///
/// # Errors
/// Reports a vendor that could not be reached, and one whose refusal says
/// nothing about ownership.
///
/// A vendor that ANSWERED "not yours" and a vendor that could not answer are
/// different facts, and this is the only place that can still tell them apart.
/// Reading every non-2xx as `false` sent the caller [`Found::None`], which the
/// callback renders as the ownership refusal — telling a person their own
/// installation is not theirs because GitHub had a bad minute, under a code
/// `is_retryable` reports as final. The listing branch in [`resolve`] already
/// maps that outage to [`error::exchange_refused`], and one vendor status has
/// no business meaning two things depending on which route the browser took.
async fn opens(
    client: &reqwest::Client,
    pinned: Option<&str>,
    token: &str,
    id: &str,
) -> Result<bool> {
    let probe = format!("{INSTALLATION_REPOSITORIES}/{id}{REPOSITORIES_PROBE_SUFFIX}");
    let answer = fetch(client, pinned, token, &probe).await?;
    let status = answer.status();
    if status.is_success() {
        return Ok(true);
    }
    match status.as_u16() {
        STATUS_UNAUTHORIZED | STATUS_FORBIDDEN | STATUS_NOT_FOUND => Ok(false),
        unanswered => Err(error::exchange_refused(unanswered)),
    }
}

/// One authenticated GET at the vendor, on the lane's pinned host if any.
async fn fetch(
    client: &reqwest::Client,
    pinned: Option<&str>,
    token: &str,
    vendor: &str,
) -> Result<reqwest::Response> {
    let endpoint = endpoint::redirected(vendor, pinned).ok_or_else(error::exchange_unreadable)?;
    Ok(client
        .get(endpoint)
        .header(HEADER_AUTHORIZATION, format!("{BEARER_PREFIX}{token}"))
        .header(HEADER_ACCEPT, CONTENT_TYPE_JSON)
        .header(HEADER_API_VERSION, API_VERSION)
        .send()
        .await?)
}
