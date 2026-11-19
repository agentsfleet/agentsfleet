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

/// How much of the text's SHA-256 a fleet's operation id carries: the first
/// 16 bytes, as 32 hexadecimal characters.
const FLEET_KEY_DIGEST_HEX: usize = 32;

/// The operation id a fleet's steer derives: the sender's event, then a
/// digest of the masked words.
///
/// The same words from the same run are one operation, so a model that
/// resends after a lost answer gets the first run back. Taken over the
/// MASKED text, so the key holds nothing derived from a secret value.
pub(super) fn fleet_operation_id(sender_event: &Uuid7, masked_text: &str) -> String {
    let digest = af_core::etag::sha256_hex(masked_text.as_bytes());
    let head = digest.get(..FLEET_KEY_DIGEST_HEX).unwrap_or(&digest);
    [sender_event.as_str(), head].join(KEY_SEPARATOR)
}

/// The admission a fleet's steer asks for: a person's steer in every field but
/// its actor, its body and its reply, and the same on every repeat.
///
/// The reply is the sender event's own destination, copied inside the
/// admission statement, so the target answers in the thread the sender was
/// asked from.
pub(super) const fn fleet_steer_admission<'a>(
    fleet: &'a Uuid7,
    workspace: &'a Uuid7,
    actor: &'a str,
    request_json: &'a str,
    key: &'a str,
    sender_event: &'a Uuid7,
) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        key: Key::Repeated(key),
        fleet,
        workspace,
        actor,
        event_type: EventType::Chat,
        request_json,
        reply: Reply::Inherit {
            event_id: sender_event,
        },
    }
}
