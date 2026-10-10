//! Where a stored refresh handle sends the platform's client secret.
//!
//! A refresh grant posts this deployment's `client_id` and `client_secret`, and
//! a Zoho handle names the accounts server that post lands on. The handle is
//! bytes the workspace wrote, and the generic secrets route accepts any of
//! them, so the server is held HERE, at the code that posts: [`endpoint`] dials
//! only the descriptor's own URL or one of [`ZOHO_ACCOUNTS_SERVERS`], and
//! refuses everything else before a byte leaves the process. The SSRF guard a
//! tenant `base_url` passes is not enough for this, because it admits any
//! public `https` host.
//!
//! [`steer`] is the one reader of the field, shared with the lease plane's mint
//! admission, which refuses the same handles earlier and under the provider
//! they claim to be.

use serde_json::{Map, Value};

/// The vault-handle field naming the accounts server this refresh token is
/// redeemable at.
///
/// Zoho multi-data-centre only, and absent for the single-region providers,
/// which fall through to the endpoint their descriptor declares. Refreshing at
/// the wrong data centre's accounts server fails `invalid_grant` exactly as the
/// initial exchange would.
const FIELD_ACCOUNTS_BASE: &str = "accounts_base";

/// The path every Zoho accounts server serves the token exchange on.
const TOKEN_PATH: &str = "/oauth/v2/token";

/// Every accounts server a Zoho callback can write onto a handle, one per data
/// centre, exactly as written.
///
/// The connect flow resolves a callback's `location` to one of these in
/// `afd_connector::zoho`; a test in the lease plane, which sees both crates,
/// holds the two lists to each other. Canada is the irregular entry: a
/// different apex domain, as Zoho documents it.
pub const ZOHO_ACCOUNTS_SERVERS: [&str; 7] = [
    "https://accounts.zoho.com",
    "https://accounts.zoho.eu",
    "https://accounts.zoho.in",
    "https://accounts.zoho.com.au",
    "https://accounts.zoho.com.cn",
    "https://accounts.zoho.jp",
    "https://accounts.zohocloud.ca",
];

/// Where a stored handle asks its refresh to be posted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steer<'a> {
    /// Where the connector's descriptor says; the handle names no server.
    Declared,
    /// To the accounts server `base` the handle names.
    To(&'a str),
}

/// Where `handle` asks its refresh to be posted.
///
/// A base that is not a string is no override at all, as the exchange has
/// always read it: the post goes where the descriptor says.
#[must_use]
pub fn steer(handle: &Value) -> Steer<'_> {
    handle.as_object().map_or(Steer::Declared, fields)
}

/// Whether `base` is one of Zoho's accounts servers, exactly as written.
#[must_use]
pub fn is_zoho_accounts_server(base: &str) -> bool {
    ZOHO_ACCOUNTS_SERVERS.contains(&base)
}

/// [`steer`], over a handle already known to be an object.
fn fields(handle: &Map<String, Value>) -> Steer<'_> {
    handle
        .get(FIELD_ACCOUNTS_BASE)
        .and_then(Value::as_str)
        .map_or(Steer::Declared, Steer::To)
}

/// Where this exchange posts, or `None` to refuse the mint unsent.
///
/// The handle's own accounts server wins over the descriptor's declared
/// endpoint, because a refresh token is redeemable at one data centre and
/// posting it at another fails `invalid_grant`. It wins only when it is one of
/// the servers a callback writes: every address dialled is then a constant of
/// this build, so there is nothing tenant-shaped left to parse or validate.
pub(super) fn endpoint(handle: &Map<String, Value>, declared: &str) -> Option<Box<str>> {
    match fields(handle) {
        Steer::Declared => Some(declared.into()),
        Steer::To(base) => {
            is_zoho_accounts_server(base).then(|| format!("{base}{TOKEN_PATH}").into())
        }
    }
}

#[cfg(test)]
#[path = "steer_tests.rs"]
mod tests;
