//! Shared admission fixtures for producer identity and reply destinations.
use crate::support::Fixtures;
use af_core::id::Uuid7;
use af_wire::event::EventType;
use afd_admission::{Admission, Admissions, Key, Producer};
const ACTOR: &str = "webhook:fixture";
const REQUEST_JSON: &str = r#"{"delivery":"fixture"}"#;

/// An admission for `fleet`, keyed so no two fleets dedup onto one row:
/// `(producer, producer_key)` is unique across the whole ledger.
pub(crate) const fn admission<'a>(
    fleet: &'a Uuid7,
    workspace: &'a Uuid7,
    key: &'a str,
) -> Admission<'a> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: EventType::Webhook,
        request_json: REQUEST_JSON,
        reply: afd_admission::Reply::None,
    }
}

/// The ledger over this fixture's datastores.
pub(crate) fn ledger(fixtures: &Fixtures) -> Admissions {
    Admissions::for_tests(fixtures.database.clone(), fixtures.queue().clone())
}

/// A repeated producer key scoped to this fixture's fleet.
pub(crate) fn producer_key(fleet: &Uuid7, suffix: &str) -> String {
    format!("{fleet}:{suffix}")
}
