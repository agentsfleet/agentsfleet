//! GitHub's second round trip: which App installation the person can reach.
//!
//! # A user authorization proves a person, not an installation
//!
//! A GitHub App is INSTALLED on an account, and the connect flow redeems a
//! user-authorization code — a bearer for the person who pressed Connect, with
//! no installation behind it. What binds a workspace is the installation, so
//! the token is spent on one more question before anything is sealed: which
//! installations of this App can that person reach? `github/ownership.zig`
//! asked the same question with the same two calls, and this is that logic on
//! the Rust tree.
//!
//! # Exactly one, or nothing is written
//!
//! The call is what repairs drift: an installation that outlived this
//! deployment's datastore is still listed, and listing it is how a workspace
//! gets it back without a person clicking through GitHub's settings. But a
//! listing of two is a choice, and choosing an organisation for a person is
//! a way to route another team's pull requests into their workspace. So two
//! refuses, none refuses, and a claimed `installation_id` the token cannot open
//! refuses — all under the ownership code, all before the vault is touched.
//!
//! # Every host here follows the exchange's pin
//!
//! Both calls go through [`crate::endpoint::redirected`], so a lane that
//! pointed the token exchange at a fake vendor gets these too. The alternative
//! — a real `api.github.com` call from a test — would prove nothing and would
//! send a freshly minted user token off the machine.

use serde_json::{Map, Value};

use crate::grant::parse::HANDLE_INTEGRATION;
use crate::grant::{Grant, Install, InstallClaim};
use crate::provider::Provider;

mod probe;

#[cfg(feature = "test-util")]
pub use probe::probe_listing_request;
pub use probe::resolve;

/// Wire fields of the listing, one spelling each.
const WIRE_INSTALLATIONS: &str = "installations";
/// See [`WIRE_INSTALLATIONS`].
const WIRE_ID: &str = "id";
/// See [`WIRE_INSTALLATIONS`].
const WIRE_ACCOUNT: &str = "account";
/// See [`WIRE_INSTALLATIONS`].
const WIRE_LOGIN: &str = "login";
/// See [`WIRE_INSTALLATIONS`].
const WIRE_ACCESS_TOKEN: &str = "access_token";

/// Handle fields the broker's mint reads back — `afd_credential`'s GitHub
/// exchange opens [`HANDLE_INSTALLATION_ID`] and accepts a decimal string.
pub(crate) const HANDLE_INSTALLATION_ID: &str = "installation_id";
/// See [`HANDLE_INSTALLATION_ID`].
const HANDLE_LABEL: &str = "label";
/// See [`HANDLE_INSTALLATION_ID`].
const HANDLE_CONNECTED_AT: &str = "connected_at_ms";

/// The longest decimal an installation id is allowed to be on the query.
///
/// `github/callback.zig`'s `MAX_INSTALLATION_ID_LEN`. GitHub's ids are far
/// shorter; the cap is what stops a query parameter from becoming a path.
pub const MAX_INSTALLATION_ID_LEN: usize = 32;

/// One installation the person can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    /// GitHub's numeric id, carried as the decimal the handle stores.
    pub id: String,
    /// The account it is installed on, for the connection's label.
    pub account: Option<String>,
}

/// What the listing found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// The App is installed nowhere this person can reach.
    None,
    /// The one installation to bind.
    One(Installation),
    /// More than one, which this daemon will not choose between.
    Several,
}

impl Found {
    /// The word an operator's log names this outcome by.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::None => "no_accessible_installation",
            Self::One(_) => "one_installation",
            Self::Several => "several_installations",
        }
    }
}

/// Whether `id` has the shape of an installation id on the query: decimal
/// digits, non-empty, within the cap.
#[must_use]
pub fn is_installation_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_INSTALLATION_ID_LEN
        && id.bytes().all(|byte| byte.is_ascii_digit())
}

/// The user token out of the exchange's answer, or nothing readable.
#[must_use]
pub(crate) fn access_token(body: &Value) -> Option<String> {
    let token = body.get(WIRE_ACCESS_TOKEN)?.as_str()?;
    (!token.is_empty()).then(|| token.to_owned())
}

/// The listing as the three-state answer, or `None` for a body this build
/// cannot read as one.
#[must_use]
pub(crate) fn parse_listing(body: &Value) -> Option<Found> {
    let items = body.get(WIRE_INSTALLATIONS)?.as_array()?;
    let [only] = items.as_slice() else {
        return Some(if items.is_empty() {
            Found::None
        } else {
            Found::Several
        });
    };
    let id = match only.get(WIRE_ID)? {
        Value::Number(number) => number.as_u64()?.to_string(),
        Value::String(text) if is_installation_id(text) => text.clone(),
        _unusable => return None,
    };
    let account = only
        .get(WIRE_ACCOUNT)
        .and_then(|account| account.get(WIRE_LOGIN))
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty())
        .map(str::to_owned);
    Some(Found::One(Installation { id, account }))
}

/// The grant a resolved installation lands as: the handle the broker mints
/// from, and the routing row the App ingress resolves the workspace by.
///
/// The row is EXCLUSIVE — an installation another workspace already routes is
/// refused rather than moved, which is the divergence from Slack's re-point
/// that `docs/AUTH.md` records: a Slack team reinstalled elsewhere is one
/// account choosing again, where a GitHub installation claimed twice is two
/// workspaces claiming one organisation's pull requests.
#[must_use]
pub fn grant(installation: &Installation, connected_at_ms: i64) -> Grant {
    let mut handle = Map::new();
    handle.insert(HANDLE_INTEGRATION.into(), Provider::GitHub.id().into());
    handle.insert(
        HANDLE_INSTALLATION_ID.into(),
        installation.id.clone().into(),
    );
    handle.insert(HANDLE_CONNECTED_AT.into(), connected_at_ms.into());
    if let Some(account) = installation.account.as_ref() {
        handle.insert(HANDLE_LABEL.into(), account.clone().into());
    }
    Grant {
        handle,
        install: Some(Install {
            external_account_id: installation.id.clone(),
            installed_by: String::new(),
            scopes: Vec::new(),
            claim: InstallClaim::Exclusive,
        }),
    }
}

#[cfg(test)]
#[path = "github/tests.rs"]
mod tests;
