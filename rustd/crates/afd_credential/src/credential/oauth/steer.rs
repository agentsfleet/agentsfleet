//! Where a stored refresh handle sends the platform's client secret.
//!
//! A refresh grant posts this deployment's `client_id` and `client_secret`, and
//! the handle can move where that post lands: `accounts_base` names an accounts
//! server, `token_path` the path beneath it. Both are bytes the workspace
//! wrote, and the generic secrets route accepts any of them. So the fields are
//! read here once, by [`steer`], for two readers: [`endpoint`], which composes
//! the address the exchange dials, and the lease plane's mint admission, which
//! holds the address to the accounts servers the connect flow can write before
//! any exchange runs. Read in two places, a handle could pass the check as one
//! address and be posted to another.

use serde_json::{Map, Value};

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
/// No connect flow writes it. It is honoured for a handle that carries it and
/// refused at mint admission, so its only source is a hand-stored handle.
const FIELD_TOKEN_PATH: &str = "token_path";

/// The path appended to an `accounts_base` that names none of its own.
///
/// The Zoho shape, which is the only provider that carries a base at all.
const DEFAULT_TOKEN_PATH: &str = "/oauth/v2/token";

/// Where a stored handle asks its refresh to be posted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steer<'a> {
    /// Where the connector's descriptor says; the handle names no server.
    Declared,
    /// To the accounts server `base`, at `path` when the handle names one.
    To {
        /// The handle's `accounts_base`.
        base: &'a str,
        /// The handle's `token_path`, when it carries one.
        path: Option<&'a str>,
    },
}

/// Where `handle` asks its refresh to be posted.
///
/// A field that is not a string is no override at all, as the exchange has
/// always read it: a non-string base posts where the descriptor says.
#[must_use]
pub fn steer(handle: &Value) -> Steer<'_> {
    handle.as_object().map_or(Steer::Declared, fields)
}

/// [`steer`], over a handle already known to be an object.
fn fields(handle: &Map<String, Value>) -> Steer<'_> {
    handle
        .get(FIELD_ACCOUNTS_BASE)
        .and_then(Value::as_str)
        .map_or(Steer::Declared, |base| Steer::To {
            base,
            path: handle.get(FIELD_TOKEN_PATH).and_then(Value::as_str),
        })
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
pub(super) fn endpoint(handle: &Map<String, Value>, declared: &str) -> Option<Box<str>> {
    let Steer::To { base, path } = fields(handle) else {
        // No override: the descriptor's endpoint, which is a compile-time
        // constant in this repository and has nothing to validate.
        return Some(declared.into());
    };
    let composed = format!("{base}{}", path.unwrap_or(DEFAULT_TOKEN_PATH));
    // The guard answers `https`, a parseable authority, and a host outside the
    // SSRF ranges. What it does NOT answer is whether the path smuggled a query
    // or a fragment, which is the one thing `isValidTokenPath` was for — asked
    // here of the parse rather than of the bytes. Nor does it answer whether
    // the host is the provider's: that is the mint admission's question.
    let parsed = url::Url::parse(&composed).ok()?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return None;
    }
    validate_endpoint(&composed).ok()?;
    Some(composed.into())
}

#[cfg(test)]
#[path = "steer_tests.rs"]
mod tests;
