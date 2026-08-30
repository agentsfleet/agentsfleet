//! Deliveries authenticated by a signature over the body rather than a bearer.
//!
//! # The order every route in this family follows
//!
//! Resolve, open the secret, verify, and only THEN read the body. A daemon that
//! parsed first would be running a deserializer over unauthenticated bytes on a
//! public endpoint, which is a surface an attacker reaches with no credential at
//! all. [`verified`] is what makes the order structural rather than conventional:
//! it hands back the raw body only on the far side of the wall, so a route
//! cannot read one it has not proved.
//!
//! # Why verification is here and not in a layer
//!
//! `route/webhook.rs` marks these `Guard::WebhookSignature`, but `plane_of`
//! answers `None` for that guard — there is no PRINCIPAL to resolve, so the
//! shared authentication layer has nothing to do. The secret is per-fleet and
//! lives behind a vault read keyed by the URL's own `fleet_id`, which means the
//! check cannot happen before routing has picked the fleet out of the path.
//! `webhook_sig.zig` reaches the same conclusion from the other direction: its
//! middleware takes a `lookup` callback into the database, which is a layer
//! that had to become a handler to do its job.

pub(crate) mod app_route;
pub(crate) mod approval_route;
pub(crate) mod github;
pub(crate) mod github_route;
pub(crate) mod identity_route;
pub(crate) mod qstash_route;
pub(crate) mod receive_route;
pub(crate) mod svix_route;

mod verify;
pub(crate) mod verify_platform;
mod verify_svix;

pub(crate) use self::verify::verified;
pub(crate) use self::verify_platform::{verified_app, verified_connector_events};

use afd_core::error_code;
use axum::body::Bytes;
use http::HeaderMap;

use crate::handler::Refusal;

/// The most bytes a delivery on this surface may CARRY.
///
/// `github.zig`'s `MAX_BODY_SIZE`, and it is the semantic cap: a body past it
/// earns `UZ-WH-030` and a sentence naming the limit, so a sender that is
/// posting too much learns which limit it hit.
pub(crate) const MAX_BODY_SIZE: usize = 1024 * 1024;

/// The most bytes this daemon will BUFFER before it refuses.
///
/// Deliberately above [`MAX_BODY_SIZE`] rather than equal to it, and the gap is
/// the point. The transport limit exists so one unauthenticated request cannot
/// make this daemon hold an arbitrary amount of memory; the semantic cap exists
/// so a sender gets a coded answer. Setting them equal would collapse the
/// second into the first — every over-cap delivery would earn `axum`'s bare 413
/// instead of `UZ-WH-030`, and a sender reading its delivery log would see a
/// status with no registry code to search. One doubling is enough headroom for
/// the coded answer to be the one a real sender meets, while an absurd body is
/// still refused before it is read.
pub const BUFFER_CEILING: usize = MAX_BODY_SIZE * 2;

/// The refusal a delivery past the cap earns.
const DETAIL_TOO_LARGE: &str = "The webhook body exceeds the 1 MiB limit. Reduce the payload size.";

/// Refuses a delivery past [`MAX_BODY_SIZE`].
///
/// Checked on the buffered length BEFORE the body is hashed: the cap is what
/// bounds the work one unauthenticated request can ask of this daemon, and
/// spending an HMAC over a body to discover it was too big would spend exactly
/// what the cap exists to protect.
///
/// # Errors
/// `UZ-WH-030`, with the limit in the sentence.
pub(crate) fn within_cap(body: &Bytes) -> Result<(), Refusal> {
    if body.len() > MAX_BODY_SIZE {
        return Err(Refusal::coded(
            error_code::WEBHOOK_PAYLOAD_TOO_LARGE,
            DETAIL_TOO_LARGE,
        ));
    }
    Ok(())
}

/// The header GitHub names the delivery's kind in.
///
/// Read by both routes that serve GitHub — the per-fleet one and the App
/// ingress — so it is declared once. Two copies of a provider's wire header
/// name is the shape that lets one surface be fixed and the other left.
pub(crate) const HEADER_EVENT: &str = "x-github-event";

/// The header GitHub carries its own delivery identifier in.
///
/// The value a redelivery REPEATS, which is what makes it the idempotency key
/// on the per-fleet route — see [`afd_ingress::Delivery::event_id`]. The App
/// ingress deliberately does NOT key on it; that route's reason is in
/// [`afd_ingress::replay_id`].
pub(crate) const HEADER_DELIVERY: &str = "x-github-delivery";

/// The refusal a delivery this daemon cannot read as its own claimed event
/// earns.
///
/// `UZ-WH-002`'s sentence, and it is one sentence for two causes on purpose: a
/// missing event header and a body that is not the event the header names are
/// the same bug from a sender's side, and telling them apart tells a forger
/// which half they got right.
pub(crate) const DETAIL_EVENT_HEADER: &str =
    "Webhook payload could not be parsed. Check Content-Type and body.";

/// One header's value, when it is one this daemon can read as text.
///
/// A header carrying bytes that are not visible ASCII resolves to `None`, which
/// every caller then treats as absent — see [`verify`]'s own copy of this note
/// for why the two are not told apart on the signature path.
pub(crate) fn text<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// The response bodies these routes answer with.
///
/// Re-exported rather than defined here: they are public wire, declared in
/// `public/openapi.json` and read by senders and the dashboard, so `afd_wire`
/// owns the shape and this plane names it.
pub(crate) use afd_wire::ingress::{Accepted, Ignored};

/// The reason a delivery to a fleet nobody is running is dropped.
///
/// `error_entries.zig:135-137`: a webhook to a paused fleet answers 200
/// `{"ignored":"fleet_paused"}`, because a sender's retry queue adds no value
/// for a fleet somebody paused on purpose. `UZ-WH-003` was retired to make room
/// for exactly this answer.
pub(crate) const REASON_FLEET_PAUSED: &str = "fleet_paused";

/// The reason a delivery of a kind this daemon serves no rule for is dropped.
///
/// Distinct from every other reason in this family: the others are DECISIONS
/// about a delivery this daemon understands, and this is an absence. Only one
/// of them is a reason to add code.
pub(crate) const REASON_UNSUPPORTED_EVENT: &str = "unsupported_event";

/// The reason a delivery outside the trigger's allow-list is dropped.
pub(crate) const REASON_EVENT_NOT_SUBSCRIBED: &str = "event_not_subscribed";

/// What an approval resolved through this surface records as its detail.
pub(crate) const REASON_APPROVAL_WEBHOOK: &str = "resolved by approval webhook";

/// What every provider-driven wake is prefixed with.
///
/// The actor names the PROVIDER and no person. A delivery carries a sender's
/// login and recording that would let an actor-shaped assertion certify that a
/// human woke this fleet when a webhook did — the same reasoning
/// `afd_events::ACTOR_MACHINE` carries for an api-key wake.
const ACTOR_PREFIX: &str = "webhook:";

/// The actor a delivery from `source` records.
pub(crate) fn actor(source: &str) -> String {
    format!("{ACTOR_PREFIX}{source}")
}

/// The body as a fleet's prose reads it, when it is a JSON document at all.
///
/// `None` for bytes that will not parse. A generic webhook body has no schema
/// this daemon can check, so the whole of what is asked is that it BE a
/// document — the fleet's own prose is what reads it, and prose cannot reason
/// over a form-encoded string or a fragment of XML.
///
/// The bytes are handed on unchanged rather than re-serialized: what the fleet
/// reasons over must be what the sender signed, and a document that went
/// through a parse and back is a different byte string.
pub(crate) fn json_payload(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body).ok()?;
    String::from_utf8(body.to_vec()).ok()
}
