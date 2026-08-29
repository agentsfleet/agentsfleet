//! Reading a provider's exchange answer as the handle this daemon will seal.
//!
//! # Two shapes, not five
//!
//! Slack answers an `{"ok":true,…}` envelope carrying a long-lived bot token
//! and the team it was installed on. The other three OAuth connectors answer
//! the ordinary `{access_token, refresh_token, expires_in}` triple RFC 6749
//! describes, and differ only in the extra fields their handle carries —
//! Zoho's data-centre base, Jira's cloud id. `oauth_refresh.zig` reaches the
//! same two-shape split, and says the same thing about why: a new refresh
//! provider is a small delta rather than a copied file.
//!
//! # `ok:false` is a REFUSED exchange, not an unreadable one
//!
//! Slack answers HTTP 200 with `{"ok":false,"error":"invalid_code"}` for a code
//! it will not redeem, so the transport says success and the exchange failed.
//! Reading the envelope is the only way to tell, which is why this is a parse
//! that can refuse rather than a deserialize that cannot.

use afd_core::clock::UnixMillis;
use serde_json::{Map, Value};

use crate::provider::Provider;

/// The field a handle names its own connector in.
///
/// Read by the runner plane when it opens the handle, so it is part of the
/// stored shape rather than a convenience.
pub(crate) const HANDLE_INTEGRATION: &str = "integration";

/// Handle fields, one spelling each (RULE UFS).
const HANDLE_BOT_TOKEN: &str = "bot_token";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_BOT_USER_ID: &str = "bot_user_id";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_TEAM_ID: &str = "team_id";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_TEAM_NAME: &str = "team_name";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_SCOPES: &str = "scopes";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_ACCESS_TOKEN: &str = "access_token";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_REFRESH_TOKEN: &str = "refresh_token";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_EXPIRES_AT: &str = "expires_at_ms";
/// See [`HANDLE_BOT_TOKEN`].
const HANDLE_CONNECTED_AT: &str = "connected_at_ms";
/// See [`HANDLE_BOT_TOKEN`].
pub(crate) const HANDLE_LABEL: &str = "label";

/// Wire fields of a provider's exchange answer, one spelling each.
const WIRE_OK: &str = "ok";
/// See [`WIRE_OK`].
const WIRE_ACCESS_TOKEN: &str = "access_token";
/// See [`WIRE_OK`].
const WIRE_REFRESH_TOKEN: &str = "refresh_token";
/// See [`WIRE_OK`].
const WIRE_EXPIRES_IN: &str = "expires_in";
/// See [`WIRE_OK`].
const WIRE_SCOPE: &str = "scope";
/// See [`WIRE_OK`].
const WIRE_TEAM: &str = "team";
/// See [`WIRE_OK`].
const WIRE_AUTHED_USER: &str = "authed_user";
/// See [`WIRE_OK`].
const WIRE_ID: &str = "id";
/// See [`WIRE_OK`].
const WIRE_NAME: &str = "name";
/// See [`WIRE_OK`]. Spelled apart from the handle field of the same name: one
/// is Slack's wire contract and the other is this daemon's stored shape, and
/// they are free to diverge even though they agree today.
const WIRE_BOT_USER_ID: &str = "bot_user_id";

/// Milliseconds in a second, for the expiry arithmetic.
const MS_PER_SECOND: i64 = 1_000;

/// What Slack's install answer routes back to a workspace.
///
/// The reverse-routing row: an event arriving from a Slack team has to resolve
/// to the workspace that installed the app, and the team id is the only handle
/// the delivery carries.
#[derive(Debug, Clone)]
pub struct Install {
    /// The provider's own id for the account — Slack's `team.id`.
    pub external_account_id: String,
    /// Who pressed Connect, as the provider names them.
    pub installed_by: String,
    /// What the install actually granted, which may be less than was asked.
    pub scopes: Vec<String>,
}

/// A parsed grant: the document to seal, and the routing row it implies.
#[derive(Debug, Clone)]
pub struct Grant {
    /// The handle, as the runner plane will read it back.
    ///
    /// A `Map` rather than a struct per provider, because the shape IS
    /// per-provider and the sealer takes a document either way — five structs
    /// would be five serializers for one write.
    pub handle: Map<String, Value>,
    /// The reverse-routing row, for the one archetype that has inbound events.
    pub install: Option<Install>,
}

/// Slack's `oauth.v2.access` answer, as a handle and an install row.
///
/// `delimiter` is the provider's, taken from [`crate::registry::Oauth2Flow`]
/// rather than spelled here. The answer's delimiter is a SECOND fact beside the
/// request's, and hard-coding it is the mistake this crate exists partly to
/// avoid — see that field's own note.
///
/// `None` for `{"ok":false}` and for any answer missing a field the handle
/// cannot be built without — see the module note on why 200 is not enough.
#[must_use]
pub fn slack(body: &Value, delimiter: char) -> Option<Grant> {
    if body.get(WIRE_OK)?.as_bool() != Some(true) {
        return None;
    }
    let team = body.get(WIRE_TEAM)?;
    let team_id = text(team, WIRE_ID)?;
    let scope = text(body, WIRE_SCOPE).unwrap_or_default();

    let mut handle = Map::new();
    handle.insert(HANDLE_INTEGRATION.into(), Provider::Slack.id().into());
    handle.insert(
        HANDLE_BOT_TOKEN.into(),
        text(body, WIRE_ACCESS_TOKEN)?.into(),
    );
    handle.insert(
        HANDLE_BOT_USER_ID.into(),
        text(body, WIRE_BOT_USER_ID)?.into(),
    );
    handle.insert(HANDLE_TEAM_ID.into(), team_id.clone().into());
    handle.insert(
        HANDLE_TEAM_NAME.into(),
        text(team, WIRE_NAME).unwrap_or_default().into(),
    );
    handle.insert(HANDLE_SCOPES.into(), scope.clone().into());

    Some(Grant {
        handle,
        install: Some(Install {
            external_account_id: team_id,
            installed_by: body
                .get(WIRE_AUTHED_USER)
                .and_then(|user| text(user, WIRE_ID))
                .unwrap_or_default(),
            // Split here rather than stored as the vendor's comma-joined
            // string: the column is a `text[]`, and a reader splitting it back
            // out would be the second place the delimiter is known.
            scopes: scope
                .split(delimiter)
                .filter(|granted| !granted.is_empty())
                .map(Into::into)
                .collect(),
        }),
    })
}

/// The refresh-token triple, as the handle a broker later mints from.
///
/// `extras` are the provider's own additions — Zoho's accounts base, Jira's
/// cloud id — merged in by the caller that knows them, which is what keeps this
/// one parse for three connectors.
///
/// `None` for an answer missing any of the three: an access token with no
/// refresh token is a credential that expires and cannot be renewed, which
/// would look connected and stop working within the hour.
#[must_use]
pub fn refresh_triple(
    provider: Provider,
    body: &Value,
    label: &str,
    connected_at: UnixMillis,
    extras: Map<String, Value>,
) -> Option<Grant> {
    let expires_in = body.get(WIRE_EXPIRES_IN)?.as_i64()?;

    let mut handle = extras;
    handle.insert(HANDLE_INTEGRATION.into(), provider.id().into());
    handle.insert(
        HANDLE_ACCESS_TOKEN.into(),
        text(body, WIRE_ACCESS_TOKEN)?.into(),
    );
    handle.insert(
        HANDLE_REFRESH_TOKEN.into(),
        text(body, WIRE_REFRESH_TOKEN)?.into(),
    );
    handle.insert(
        HANDLE_EXPIRES_AT.into(),
        connected_at
            .saturating_add_millis(expires_in.saturating_mul(MS_PER_SECOND))
            .as_millis()
            .into(),
    );
    handle.insert(HANDLE_CONNECTED_AT.into(), connected_at.as_millis().into());
    handle.insert(HANDLE_LABEL.into(), label.into());

    Some(Grant {
        handle,
        // No inbound events, so nothing to route back: these three are asked
        // questions by a fleet and never wake one.
        install: None,
    })
}

/// One non-empty string field of a document.
fn text(document: &Value, name: &str) -> Option<String> {
    let value = document.get(name)?.as_str()?;
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests;
