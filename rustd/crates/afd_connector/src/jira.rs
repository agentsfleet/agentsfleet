//! Which Atlassian site a Jira grant actually reaches.
//!
//! # A second call, because the token alone does not say
//!
//! Atlassian's three-legged OAuth answers a token that is scoped to whatever
//! sites the person authorized, and names none of them. Every later API call
//! goes to `api.atlassian.com/ex/jira/{cloud_id}`, so a grant stored without a
//! cloud id is a credential with nowhere to spend it — connected in the
//! dashboard and unusable by the fleet that declared it. `jira/callback.zig`
//! makes the same call for the same reason, and it is the one provider delta
//! that costs a round trip.
//!
//! # The first site, and why that is the right one rather than a shortcut
//!
//! The endpoint answers every site the grant reaches, in the order Atlassian
//! ranks them. A connector binds ONE site — the fleet's prose names a project,
//! not a tenancy — so a person with two sites gets the one Atlassian put first,
//! which is the one their own UI defaults to. Picking differently would mean
//! asking the person a question this flow has no screen for.

use serde_json::Value;

use crate::endpoint;
use crate::error::{self, Result};

/// Where the sites a grant reaches are listed.
const ACCESSIBLE_RESOURCES: &str = "https://api.atlassian.com/oauth/token/accessible-resources";

/// The header the freshly minted access token is presented in.
const HEADER_AUTHORIZATION: &str = "authorization";

/// The scheme it is presented under.
const BEARER_PREFIX: &str = "Bearer ";

/// The header asking for JSON back.
const HEADER_ACCEPT: &str = "accept";

/// See [`HEADER_ACCEPT`].
const CONTENT_TYPE_JSON: &str = "application/json";

/// Response fields, one spelling each.
const FIELD_ID: &str = "id";
/// See [`FIELD_ID`].
const FIELD_NAME: &str = "name";
/// See [`FIELD_ID`].
const FIELD_URL: &str = "url";

/// Handle fields this provider adds beyond the shared triple.
pub(crate) const HANDLE_CLOUD_ID: &str = "cloud_id";
/// See [`HANDLE_CLOUD_ID`].
pub(crate) const HANDLE_SITE_URL: &str = "site_url";

/// The Atlassian site a grant will act on.
#[derive(Debug, Clone)]
pub struct Site {
    /// The tenancy every later API path is built from.
    pub cloud_id: String,
    /// Where a person would go to see it.
    pub url: String,
    /// What that site calls itself, for the handle's operator-facing label.
    pub name: String,
}

/// The site this access token reaches.
///
/// Takes the host a lane pinned rather than a URL, so a call site cannot hand
/// this an endpoint of its own: where the listing lives is Atlassian's fact and
/// belongs here, beside the fields it is parsed for. `jira/callback.zig:87`
/// draws the same line — the path is the provider file's, the origin is the
/// deployment's one override.
///
/// # Errors
/// Reports an Atlassian that could not be reached, one that answered and
/// refused, and an answer carrying no site — the last is a grant that
/// authorized nothing, which is indistinguishable from a failed exchange as far
/// as what the person should do about it.
pub async fn resolve(
    client: &reqwest::Client,
    pinned: Option<&str>,
    access_token: &str,
) -> Result<Site> {
    let answer = client
        .get(endpoint::redirected(ACCESSIBLE_RESOURCES, pinned))
        .header(
            HEADER_AUTHORIZATION,
            format!("{BEARER_PREFIX}{access_token}"),
        )
        .header(HEADER_ACCEPT, CONTENT_TYPE_JSON)
        .send()
        .await?;

    let status = answer.status();
    if !status.is_success() {
        return Err(error::exchange_refused(status.as_u16()));
    }

    // Read as text for the reason `afd_cron::qstash` reads its own answer that
    // way: this workspace resolves `reqwest` without its `json` feature.
    let body = answer.text().await?;
    let sites: Value =
        serde_json::from_str(&body).map_err(|_unreadable| error::exchange_unreadable())?;

    first_site(&sites).ok_or_else(error::exchange_unreadable)
}

/// The first site of an accessible-resources answer — see the module note.
fn first_site(sites: &Value) -> Option<Site> {
    let site = sites.as_array()?.first()?;
    let text = |name: &str| site.get(name)?.as_str().map(str::to_owned);
    Some(Site {
        cloud_id: text(FIELD_ID)?,
        url: text(FIELD_URL).unwrap_or_default(),
        name: text(FIELD_NAME).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use serde_json::json;

    use super::first_site;

    /// The first entry is the site, with its label and its address.
    #[test]
    fn the_first_listed_site_is_the_one_a_grant_binds() {
        let sites = json!([
            {"id": "cloud-1", "url": "https://one.atlassian.net", "name": "One"},
            {"id": "cloud-2", "url": "https://two.atlassian.net", "name": "Two"},
        ]);

        let site = first_site(&sites).expect("a listed site");

        assert_eq!(site.cloud_id, "cloud-1");
        assert_eq!(site.url, "https://one.atlassian.net");
        assert_eq!(site.name, "One");
    }

    /// A site with no label is still a site.
    ///
    /// The cloud id is the only field a later API call needs; the URL and the
    /// name are what an operator reads. Requiring them would refuse a working
    /// grant over a cosmetic absence.
    #[test]
    fn a_site_without_a_label_or_address_still_binds() {
        let site = first_site(&json!([{"id": "cloud-1"}])).expect("a listed site");

        assert_eq!(site.cloud_id, "cloud-1");
        assert!(site.url.is_empty());
        assert!(site.name.is_empty());
    }

    /// An answer naming no site is no site.
    ///
    /// The empty array is the live case: a person who authorized the app
    /// against no site at all. The grant would store a cloud id of nothing and
    /// every later call would 404 with no line connecting it to the connect.
    #[test]
    fn an_answer_naming_no_site_binds_nothing() {
        for answer in [
            json!([]),
            json!({}),
            json!([{"url": "https://x"}]),
            json!(null),
        ] {
            assert!(first_site(&answer).is_none(), "`{answer}` names no site");
        }
    }
}
