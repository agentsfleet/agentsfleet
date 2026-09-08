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

/// The listing request, for the suite that proves the vendor contract.
///
/// Feature-gated rather than plain `pub` for [`crate::exchange::Exchange::probe_request`]'s
/// reason: production has no use for a request it does not send, and the seam
/// exists so a live test can send the daemon's OWN request instead of a
/// lookalike. A lookalike is precisely what would have missed the missing
/// `User-Agent` — a hand-rolled `reqwest` call in a test is as likely to omit
/// it as the daemon was.
#[cfg(feature = "test-util")]
pub fn probe_listing_request(client: &reqwest::Client, token: &str) -> reqwest::RequestBuilder {
    request(client, String::from(USER_INSTALLATIONS), token)
}

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
/// See [`HEADER_API_VERSION`].
const HEADER_USER_AGENT: &str = "user-agent";

/// What this daemon calls itself at GitHub's REST API, because GitHub refuses
/// a request that calls itself nothing.
///
/// Not politeness and not telemetry: `api.github.com` answers **403** —
/// "Request forbidden by administrative rules. Please make sure your request
/// has a User-Agent header" — before it reads the `Authorization` header at
/// all. `reqwest` sends no default one, so every call in this module was
/// refused by that rule, and the refusal arrived exactly where a token that
/// cannot see an installation would arrive. That is the trap: [`opens`] reads
/// 403 as "this token does not open that installation", so a missing header
/// did not look like a broken request — it looked like a person with no
/// access, for every person and every installation.
///
/// The name matches `afd_library`'s GitHub client, which has always sent one.
/// Same vendor, same rule, and this module is the half that did not follow.
const USER_AGENT: &str = "agentsfleetd";

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
        // The LISTING refused, not the exchange: the code was redeemed and the
        // token in `token` is the proof. Naming the exchange here is what sent
        // a live diagnosis to the client secret for a vendor that was
        // objecting to the request this module makes.
        return Err(error::installation_listing_refused(status.as_u16()));
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
        unanswered => Err(error::installation_listing_refused(unanswered)),
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
    Ok(request(client, endpoint, token).send().await?)
}

/// The request both vendor calls go out as, headers and all.
///
/// Extracted and named for the reason [`crate::exchange::Exchange::request`]
/// is: the HEADERS are a contract with the vendor, and a contract nothing
/// exercises is a comment. `tests/vendor_contract.rs` sends THIS builder at
/// the live endpoint, so a header dropped here fails a lane rather than a
/// person's connect.
pub(crate) fn request(
    client: &reqwest::Client,
    endpoint: String,
    token: &str,
) -> reqwest::RequestBuilder {
    client
        .get(endpoint)
        .header(HEADER_AUTHORIZATION, format!("{BEARER_PREFIX}{token}"))
        .header(HEADER_ACCEPT, CONTENT_TYPE_JSON)
        .header(HEADER_API_VERSION, API_VERSION)
        .header(HEADER_USER_AGENT, USER_AGENT)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{CONTENT_TYPE_JSON, USER_AGENT, USER_INSTALLATIONS, request};

    /// Every header GitHub's REST API requires rides both vendor calls.
    ///
    /// The `User-Agent` is the one this asserts for: `api.github.com` answers
    /// 403 without it, BEFORE reading `Authorization`, and [`super::opens`]
    /// reads 403 as "this token does not open that installation". So dropping
    /// the header does not surface as a broken request — it surfaces as every
    /// person lacking access to every installation, which is a defect that
    /// reads like a permissions problem at the vendor. Asserted on the
    /// daemon's own builder, because a lookalike request written here is as
    /// likely to omit the header as the daemon was.
    #[test]
    fn the_installation_probe_names_itself_to_the_vendor() {
        let built = request(
            &reqwest::Client::new(),
            String::from(USER_INSTALLATIONS),
            "user-to-server-token",
        )
        .build()
        .expect("a request the client can send");

        let headers = built.headers();
        assert_eq!(
            headers
                .get("user-agent")
                .map(|value| value.to_str().unwrap_or_default()),
            Some(USER_AGENT),
        );
        assert_eq!(
            headers
                .get("accept")
                .map(|value| value.to_str().unwrap_or_default()),
            Some(CONTENT_TYPE_JSON),
        );
        assert!(headers.contains_key("authorization"));
        assert!(headers.contains_key("x-github-api-version"));
    }
}
