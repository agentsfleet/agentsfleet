//! Where in a Slack thread an answer belongs.
//!
//! One type for both ends of the round trip: the mention producer writes it
//! into the admission as the reply destination, and the outbound poster reads
//! it back off the delivery job. A shared type is what keeps the two from
//! spelling a key differently and stranding every answer owed under the old
//! spelling.

use serde::{Deserialize, Serialize};

/// A Slack thread, as a reply destination's address.
///
/// `channel_id` and `thread_ts` are required and non-empty: an address
/// missing either posts nowhere, and [`non_empty`] turns `""` into the same
/// refusal as absent before a request is built. `team_id` is recorded for the
/// operator reading a row and ignored by the poster, which posts with the
/// workspace's own grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// The workspace Slack team the mention arrived from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// The channel the thread lives in.
    #[serde(deserialize_with = "non_empty")]
    pub channel_id: String,
    /// The thread's root message, which a reply is threaded under.
    #[serde(deserialize_with = "non_empty")]
    pub thread_ts: String,
}

impl Thread {
    /// The address this thread is recorded under.
    ///
    /// # Errors
    /// Never in practice: three strings always serialise. Reported rather than
    /// unwrapped, so a caller decides what an impossible failure means.
    pub fn address(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// The thread a recorded address names, or `None` when it names nowhere.
    #[must_use]
    pub fn parse(address: &str) -> Option<Self> {
        serde_json::from_str(address).ok()
    }
}

/// Refuses a string field that is present and empty.
fn non_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom("empty"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use super::Thread;

    /// What the producer writes, the poster reads back whole.
    #[test]
    fn a_thread_round_trips_through_its_address() {
        let thread = Thread {
            team_id: Some("T024BE7LD".to_owned()),
            channel_id: "C0123456789".to_owned(),
            thread_ts: "1700000000.000100".to_owned(),
        };
        let address = thread.address().expect("three strings serialise");
        assert_eq!(Thread::parse(&address), Some(thread));
    }

    /// Every address naming nowhere is refused before a request is built.
    #[test]
    fn an_address_naming_nowhere_is_refused() {
        for stored in [
            r#"{"channel_id":"","thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":"C123","thread_ts":""}"#,
            r#"{"channel_id":"C123"}"#,
            r#"{"thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":42,"thread_ts":"1700000000.000100"}"#,
            r#"{"channel_id":"C123","reply_thread_ts":"1700000000.000100"}"#,
            "{}",
            "not json",
        ] {
            assert_eq!(Thread::parse(stored), None, "`{stored}` names nowhere");
        }
    }
}
