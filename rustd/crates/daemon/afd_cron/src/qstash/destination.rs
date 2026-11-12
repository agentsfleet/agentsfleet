//! Where the scheduler calls a schedule back: this deployment's own origin and
//! the one path a signed fire arrives on.

use url::Url;

use crate::error::{self, Result};

/// The path the signed fire arrives back on.
///
/// Pinned byte-for-byte: it is half of the `sub` claim a fire token is verified
/// against, so a change here would make every schedule already registered
/// unverifiable.
pub const INGRESS_PATH: &str = "/v1/ingress/qstash/schedules";

/// The longest destination this daemon will register.
pub const MAX_DESTINATION_BYTES: usize = 2048;

/// Where the scheduler should call back to, for this deployment.
///
/// # Errors
/// A refusal rather than a silently truncated URL: one kind for a value that
/// is not a URL or is past [`MAX_DESTINATION_BYTES`], another for a query or a
/// fragment. The destination
/// rides RAW inside the provider's own request path, so a `?` or `#` here would
/// be read as the provider request's query or fragment and register a callback
/// this daemon never meant.
pub fn destination_url(api_url: &str) -> Result<String> {
    let parsed = Url::parse(api_url).map_err(|_unparsed| error::destination_unusable())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(error::destination_unusable());
    }

    // Asked of the PARSED url rather than of the string. A `#` inside a
    // percent-encoded path is not a fragment and a `?` in userinfo is not a
    // query, so a substring search answers this question wrongly in both
    // directions — which for a value that rides raw inside the provider's own
    // request path is the difference between a callback that works and one
    // silently truncated at the first `?`.
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(error::destination_not_plain_origin());
    }

    let base = parsed.as_str().trim_end_matches('/');
    let destination = format!("{base}{INGRESS_PATH}");
    if destination.len() > MAX_DESTINATION_BYTES {
        return Err(error::destination_unusable());
    }
    Ok(destination)
}
