//! The session blob's instants, spelled as the Lua scripts compare them.
//!
//! The scripts read `*_at_ms` off the blob and compare them against an
//! `ARGV` instant as plain numbers, so an instant is stored as whole
//! milliseconds since the epoch. Rust holds a [`Timestamp`]; this module is
//! the one place the two spellings meet.

use jiff::Timestamp;
use serde::{Deserializer, Serializer};

/// `instant` as the scripts store it.
pub(super) fn epoch_ms(instant: Timestamp) -> i64 {
    instant.as_millisecond()
}

/// A stored count read back as an instant, refusing one outside jiff's range.
fn from_epoch_ms<E: serde::de::Error>(count: i64) -> Result<Timestamp, E> {
    Timestamp::from_millisecond(count).map_err(E::custom)
}

/// `#[serde(with)]` for a required instant.
pub(super) mod required {
    use super::{Deserializer, Serializer, Timestamp};

    pub(in crate::session) fn serialize<S: Serializer>(
        instant: &Timestamp,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(super::epoch_ms(*instant))
    }

    pub(in crate::session) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Timestamp, D::Error> {
        super::from_epoch_ms(<i64 as serde::Deserialize>::deserialize(deserializer)?)
    }
}

/// `#[serde(with)]` for an instant a session may not have reached yet.
pub(super) mod optional {
    use super::{Deserializer, Serializer, Timestamp};

    #[expect(
        clippy::ref_option,
        reason = "serde's `with` hands the field over by reference"
    )]
    pub(in crate::session) fn serialize<S: Serializer>(
        instant: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match instant {
            Some(instant) => serializer.serialize_some(&super::epoch_ms(*instant)),
            None => serializer.serialize_none(),
        }
    }

    pub(in crate::session) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Timestamp>, D::Error> {
        <Option<i64> as serde::Deserialize>::deserialize(deserializer)?
            .map(super::from_epoch_ms)
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "test: an unmet precondition should fail the test loudly"
    )]

    use jiff::Timestamp;
    use serde_json::{Value, json};

    use crate::session::{SessionState, SessionStatus};

    const CREATED: Timestamp = Timestamp::constant(1_700_000_000, 123_000_000);
    const EXPIRES: Timestamp = Timestamp::constant(1_700_000_300, 0);

    fn session(approved_at: Option<Timestamp>) -> SessionState {
        SessionState {
            session_id: "s".to_owned(),
            status: SessionStatus::Pending,
            cli_public_key: "k".to_owned(),
            token_name: "t".to_owned(),
            dashboard_public_key: None,
            ciphertext: None,
            nonce: None,
            verification_code_hmac_hex: None,
            verification_attempts: 0,
            created_at: CREATED,
            expires_at: EXPIRES,
            approved_at,
            consumed_at: None,
            aborted_reason: None,
            clerk_user_id: None,
            consumed_client_fingerprint_hex: None,
            consume_payload_expires_at: None,
        }
    }

    #[test]
    fn instants_keep_the_keys_and_numbers_the_scripts_compare() {
        let stored = serde_json::to_value(session(Some(EXPIRES))).expect("a session encodes");

        assert_eq!(
            stored.get("created_at_ms"),
            Some(&json!(1_700_000_000_123_i64))
        );
        assert_eq!(
            stored.get("expires_at_ms"),
            Some(&json!(1_700_000_300_000_i64))
        );
        assert_eq!(
            stored.get("approved_at_ms"),
            Some(&json!(1_700_000_300_000_i64))
        );
        assert!(
            stored.get("consumed_at_ms").is_none(),
            "an instant not reached is left out, as the scripts expect"
        );
    }

    #[test]
    fn a_stored_session_reads_back_unchanged_with_and_without_its_optional_instants() {
        for approved_at in [None, Some(EXPIRES)] {
            let original = session(approved_at);
            let text = serde_json::to_string(&original).expect("a session encodes");

            let read: SessionState = serde_json::from_str(&text).expect("a session decodes");

            assert_eq!(read, original);
        }
    }

    #[test]
    fn a_stored_count_outside_jiffs_range_is_refused() {
        let mut stored = serde_json::to_value(session(None)).expect("a session encodes");
        for key in ["created_at_ms", "approved_at_ms"] {
            let mut bad = stored.clone();
            bad.as_object_mut()
                .expect("a session is an object")
                .insert(key.to_owned(), Value::from(i64::MAX));

            assert!(
                serde_json::from_value::<SessionState>(bad).is_err(),
                "{key} past jiff's range is refused"
            );
        }
        stored
            .as_object_mut()
            .expect("a session is an object")
            .insert("approved_at_ms".to_owned(), Value::Null);
        let read: SessionState = serde_json::from_value(stored).expect("a null instant decodes");
        assert_eq!(read.approved_at, None);
    }
}
