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
pub(crate) mod github;
pub(crate) mod github_route;

mod verify;

pub(crate) use self::verify::{verified, verified_app};

use serde::Serialize;

/// What a delivery this daemon accepted is answered with.
///
/// `202`, and the event id, so a provider's delivery log carries the identifier
/// an operator can search the fleet's history by. A replayed delivery answers
/// the FIRST attempt's id rather than a new one — that is the whole point of
/// the at-most-once claim, and a sender comparing two responses should see the
/// same event both times.
#[derive(Debug, Serialize)]
pub(crate) struct Accepted<'a> {
    /// The event the fleet will run, or already ran.
    pub(crate) event_id: &'a str,
    /// Whether an earlier delivery already produced it.
    ///
    /// Reported rather than hidden: a provider debugging a duplicate wants to
    /// know this daemon SAW the repeat and declined to run twice, which is a
    /// different fact from the delivery having been lost.
    pub(crate) replayed: bool,
}

/// What a delivery this daemon deliberately dropped is answered with.
///
/// `200` and a reason, never a 4xx. Every one of these is a real,
/// correctly-signed delivery that simply does not wake this fleet — a green
/// build, a label edit, a paused fleet. Answering an error would put it in the
/// sender's retry queue forever, and retrying changes none of them. The shape
/// is `{"ignored": "<reason>"}`, which is `error_entries.zig:135`'s
/// `{"ignored":"fleet_paused"}` generalised over every reason.
#[derive(Debug, Serialize)]
pub(crate) struct Ignored<'a> {
    /// Which rule dropped it.
    pub(crate) ignored: &'a str,
}

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
