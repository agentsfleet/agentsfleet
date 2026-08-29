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

use serde::ser::SerializeMap as _;
use serde::{Serialize, Serializer};

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

/// `GET …/connectors/{provider}` — one provider's connection.
///
/// Carries no token and no expiry. A status read answers whether a person has
/// connected and what it is called; every other field of the stored handle is
/// the broker's business and none of it belongs in a document a browser holds.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConsentRedirect<'a> {
    /// The provider's own page, carrying this round-trip's signed state.
    pub install_url: Cow<'a, str>,
}

/// `POST /v1/connectors/{provider}/events` — a delivery acknowledged and not
/// acted on.
///
/// `200` with a reason, never a 4xx. Every one of these is a real,
/// correctly-signed delivery that simply wakes nothing, and answering an error
/// would put it in the sender's retry queue forever without changing it. The
/// shape is `error_entries.zig:135`'s `{"ignored":"fleet_paused"}` generalised
/// over every reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryIgnored<'a> {
    /// Which rule dropped it.
    pub ignored: Cow<'a, str>,
}

/// `POST /v1/connectors/{provider}/events` — the answer to an endpoint-
/// ownership handshake.
///
/// # Why the KEY is data
///
/// A vendor proves it is talking to the endpoint it registered by posting a
/// value and requiring it back. Slack's is `challenge`; another vendor's is
/// its own word. So the field NAME is provider data, and a struct with a
/// `challenge` member would be one connector's spelling frozen into the type
/// that serves every connector.
///
/// This is still a typed shape rather than an untyped document: the invariant —
/// exactly one key, whose name and value both come from the registry entry — is
/// written once, here, in the crate that owns wire shapes. A handler assembling
/// a map would be re-deciding it per call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeEcho<'a> {
    /// The key the provider looks for in the response.
    pub field: Cow<'a, str>,
    /// Exactly the value the request carried under that key.
    pub value: Cow<'a, str>,
}

impl Serialize for HandshakeEcho<'_> {
    /// Writes the one pair, and nothing else.
    ///
    /// Hand-written rather than `#[serde(flatten)]` over a map, because flatten
    /// buys nothing here and costs the guarantee: a map is free to hold two
    /// entries or none, and this document is exactly one by construction.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(self.field.as_ref(), self.value.as_ref())?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{ConnectionView, DeliveryIgnored, HandshakeEcho};

    /// The handshake answer is the one pair, under the name it was asked for.
    ///
    /// Asserted as BYTES rather than through a parsed value: what proves the
    /// endpoint is the document the vendor reads, and a comparison that parsed
    /// first would pass over an envelope wrapped around the pair.
    #[test]
    fn test_the_handshake_echo_is_exactly_the_one_pair() {
        let echo = HandshakeEcho {
            field: Cow::Borrowed("challenge"),
            value: Cow::Borrowed("3eZbrw1a"),
        };

        assert_eq!(
            serde_json::to_string(&echo).ok().as_deref(),
            Some(r#"{"challenge":"3eZbrw1a"}"#),
            "a vendor reads this document literally; an envelope around it \
             fails the ownership check"
        );
    }

    /// The key travels from the value, so another vendor's word works too.
    #[test]
    fn test_the_handshake_echo_carries_whatever_key_it_was_given() {
        let echo = HandshakeEcho {
            field: Cow::Borrowed("nonce"),
            value: Cow::Borrowed("abc123"),
        };

        assert_eq!(
            serde_json::to_string(&echo).ok().as_deref(),
            Some(r#"{"nonce":"abc123"}"#)
        );
    }

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

    /// The drop acknowledgement is the reason and nothing else.
    #[test]
    fn test_an_ignored_delivery_answers_only_its_reason() {
        let ignored = DeliveryIgnored {
            ignored: Cow::Borrowed("event_producer_not_ported"),
        };

        assert_eq!(
            serde_json::to_string(&ignored).ok().as_deref(),
            Some(r#"{"ignored":"event_producer_not_ported"}"#)
        );
    }
}
