//! `/v1/connectors/**` and a workspace's `/connectors/**` — what those routes
//! answer with.
//!
//! # Why these are here rather than beside their handlers
//!
//! Every one of these is a shape a client parses. A struct declared inside the
//! handler that writes it is a contract only one side can see: the dashboard
//! and the CLI generate against THIS crate, so a field renamed in a handler-
//! local struct is a silent break, while the same rename here fails a build and
//! shows up in a fixture diff. Nothing on this surface is answered with
//! `serde_json::json!` — an untyped document is a shape the compiler cannot
//! check and a generator cannot read.
//!
//! # Borrowed, like the rest of this crate
//!
//! `Cow<'a, str>` behind `#[serde(borrow)]` rather than `String`. A catalogue
//! row's provider id and display name are `&'static str` from the registry, and
//! a connection's label is a slice of a document already in memory — none of
//! them needs a copy to be written out.
//!
//! # No `skip_serializing_if`
//!
//! An absent optional is `null`, never a missing key. A dashboard reading
//! `label` on an object that sometimes lacks it would have to branch on
//! `undefined` as well as on `null`, which is two states for one fact. The
//! whole crate holds this rule for the same reason.

use std::borrow::Cow;

use serde::Serialize;

/// What a workspace holding a landed grant is told.
///
/// `oauth_status.zig`'s `STATUS_CONNECTED`, kept byte-for-byte: the dashboard
/// switches on this string and a cutover has both daemons answering the route.
pub const STATUS_CONNECTED: &str = "connected";

/// What a workspace holding nothing is told — see [`STATUS_CONNECTED`].
pub const STATUS_NOT_CONNECTED: &str = "not_connected";

/// The wire spelling of a connector whose flow is a consent hop.
///
/// `registry.zig` renders `@tagName(spec.archetype)`, so these two strings are
/// its variant names and are a wire contract the dashboard switches on rather
/// than a description this surface is free to improve.
pub const ARCHETYPE_OAUTH2: &str = "oauth2";

/// The wire spelling of a connector whose flow is an App installation.
pub const ARCHETYPE_APP_INSTALL: &str = "app_install";

// Every other field of the stored handle is the broker's business, and none of
// it belongs in a document a browser holds.
/// One provider's connection, as a status read returns it.
///
/// This response carries no access token and no expiry. It tells you whether
/// someone connected this provider, and what the connection is called.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionView<'a> {
    /// Whether this workspace holds a landed grant — [`STATUS_CONNECTED`] or
    /// [`STATUS_NOT_CONNECTED`].
    pub status: Cow<'a, str>,
    /// What a person sees the connection called, when the grant named one.
    pub label: Option<Cow<'a, str>>,
}

/// One row of `GET …/connectors`.
///
/// No secret material and no field that could carry any — the whole document is
/// four facts about availability.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogueEntry<'a> {
    /// The provider's route segment, which is also its stored id.
    pub id: Cow<'a, str>,
    /// Which flow connecting it runs — see [`ARCHETYPE_OAUTH2`].
    pub archetype: Cow<'a, str>,
    /// The name a card shows.
    pub display_name: Cow<'a, str>,
    /// Whether this DEPLOYMENT has been set up to connect it.
    pub configured: bool,
    /// Whether THIS workspace holds a landed grant for it.
    pub connected: bool,
}

/// `POST …/connectors/{provider}/connect` — where a person is sent to consent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsentRedirect<'a> {
    /// The provider's own page, carrying this round-trip's signed state.
    pub install_url: Cow<'a, str>,
}

/// The connect landed and no dashboard page could be named to send the person to.
///
/// `POST /v1/connectors/{provider}/callback` answers this as a `200`, never
/// as a failure. The grant is sealed and the connection is live by the time
/// this is written. An error would tell a person their connect did not work
/// when it did, and the next thing they would do is press Connect again.
// The value is [`STATUS_CONNECTED`], the word a status read answers once the
// grant has landed. `callback.zig` answers the same one field for the same
// reason.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Connected<'a> {
    /// Always `connected`; the same word a status read answers once the grant
    /// has landed.
    pub status: Cow<'a, str>,
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{Connected, ConnectionView};

    /// An absent label is `null`, never a missing key.
    ///
    /// The module's no-`skip_serializing_if` rule, asserted where a client
    /// would notice it: a dashboard branching on `undefined` as well as `null`
    /// is two states for one fact.
    #[test]
    fn test_an_unlabelled_connection_still_carries_the_key() {
        let view = ConnectionView {
            status: Cow::Borrowed(super::STATUS_NOT_CONNECTED),
            label: None,
        };

        assert_eq!(
            serde_json::to_string(&view).ok().as_deref(),
            Some(r#"{"status":"not_connected","label":null}"#)
        );
    }

    /// The landing answer is the status word a later status read agrees with.
    #[test]
    fn test_a_landed_connect_answers_the_status_a_read_would() {
        let landed = Connected {
            status: Cow::Borrowed(super::STATUS_CONNECTED),
        };

        assert_eq!(
            serde_json::to_string(&landed).ok().as_deref(),
            Some(r#"{"status":"connected"}"#),
            "the dashboard switches on this word in two places; they must agree"
        );
    }
}
