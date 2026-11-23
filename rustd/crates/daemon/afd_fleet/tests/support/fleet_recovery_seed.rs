//! Shared turn fixtures for producer identity and reply destinations.
use crate::support::Fixtures;
use af_core::id::Uuid7;
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Turns};
const ACTOR: &str = "webhook:fixture";
const REQUEST_JSON: &str = r#"{"delivery":"fixture"}"#;

/// A turn for `fleet`, keyed so no two fleets dedup onto one row:
/// `(producer, producer_key)` is unique across the whole ledger.
pub(crate) const fn incoming_turn<'a>(
    fleet: &'a Uuid7,
    workspace: &'a Uuid7,
    key: &'a str,
) -> IncomingTurn<'a> {
    IncomingTurn {
        producer: Producer::Webhook,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor: ACTOR,
        event_type: TurnKind::Webhook,
        request_json: REQUEST_JSON,
        reply: afd_turns::Reply::None,
    }
}

/// The ledger over this fixture's datastores.
pub(crate) fn ledger(fixtures: &Fixtures) -> Turns {
    Turns::for_tests(fixtures.database.clone(), fixtures.bus().clone())
}

/// A repeated producer key scoped to this fixture's fleet.
pub(crate) fn producer_key(fleet: &Uuid7, suffix: &str) -> String {
    format!("{fleet}:{suffix}")
}
