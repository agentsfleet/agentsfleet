//! A steer builds one scoped ledger key and the same admission on every attempt.

use af_core::id::Uuid7;
use af_wire::event::EventType;
use afd_admission::{Admission, Key, Producer, Reply};

/// Joins the fleet to the caller's operation id in the ledger key.
const KEY_SEPARATOR: &str = ":";

/// The ledger key a caller's operation id becomes on one fleet.
pub(super) fn scoped_key(fleet: &str, operation: &str) -> String {
    [fleet, operation].join(KEY_SEPARATOR)
}

/// The admission a steer asks for: the same fields on the first send and on
/// every repeat, which is what makes the digest comparison mean anything.
pub(super) const fn steer_admission<'a>(
    fleet: &'a Uuid7,
    workspace: &'a Uuid7,
    actor: &'a str,
    request_json: &'a str,
    key: Option<&'a str>,
) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        // Mapped explicitly, never by `unwrap_or`-ing into a default: `Key`'s
        // own note warns that an `Option` lets a caller which HAS an identity
        // lose deduplication by omission, and this is the call site that would
        // do it.
        key: match key {
            Some(repeated) => Key::Repeated(repeated),
            None => Key::Unrepeatable,
        },
        fleet,
        workspace,
        actor,
        event_type: EventType::Chat,
        request_json,
        // A steer is read on the event tail that carried it, never posted to a thread.
        reply: Reply::None,
    }
}
