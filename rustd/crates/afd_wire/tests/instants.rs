//! Instants on the wire: three fraction digits in UTC, any offset in, nothing
//! finer than a millisecond accepted.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::instant::SUB_MILLISECOND;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// A shape carrying one required and one optional instant.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Stamped {
    #[serde(with = "afd_wire::instant")]
    created_at: Timestamp,
    #[serde(with = "afd_wire::instant::option")]
    revoked_at: Option<Timestamp>,
}

/// Ascending instants whose digit counts differ, so a variable-width print
/// would sort them wrong.
const ASCENDING_MS: [i64; 4] = [1_000, 1_001, 1_120, 2_000];

/// The instant `millis` after the epoch.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("in range")
}

#[test]
fn test_an_instant_always_prints_three_fraction_digits_in_utc() {
    for (millis, text) in [
        (1_760_725_609_000, "2025-10-17T18:26:49.000Z"),
        (1_760_725_609_120, "2025-10-17T18:26:49.120Z"),
        (1_760_725_609_123, "2025-10-17T18:26:49.123Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
    ] {
        let written = serde_json::to_string(&Stamped {
            created_at: at(millis),
            revoked_at: None,
        })
        .unwrap();
        assert_eq!(
            written,
            format!(r#"{{"created_at":"{text}","revoked_at":null}}"#),
            "{millis}ms"
        );
    }
}

#[test]
fn test_written_instants_sort_in_time_order() {
    let texts: Vec<String> = ASCENDING_MS
        .into_iter()
        .map(|millis| serde_json::to_string(&afd_instant(at(millis))).unwrap())
        .collect();
    let mut sorted = texts.clone();
    sorted.sort();
    assert_eq!(sorted, texts, "string order is time order");
}

/// `instant` alone, as one JSON string.
fn afd_instant(instant: Timestamp) -> serde_json::Value {
    serde_json::to_value(Stamped {
        created_at: instant,
        revoked_at: None,
    })
    .unwrap()
    .get("created_at")
    .cloned()
    .unwrap()
}

#[test]
fn test_an_offset_instant_reads_as_utc_and_writes_back_with_z() {
    let read: Stamped = serde_json::from_str(
        r#"{"created_at":"2025-10-17T23:56:49.123+05:30","revoked_at":"2025-10-17T18:26:49Z"}"#,
    )
    .unwrap();
    assert_eq!(read.created_at, at(1_760_725_609_123));
    assert_eq!(read.revoked_at, Some(at(1_760_725_609_000)));
    assert_eq!(
        serde_json::to_string(&read).unwrap(),
        r#"{"created_at":"2025-10-17T18:26:49.123Z","revoked_at":"2025-10-17T18:26:49.000Z"}"#
    );
}

#[test]
fn test_a_sub_millisecond_instant_is_refused() {
    let error = serde_json::from_str::<Stamped>(
        r#"{"created_at":"2025-10-17T18:26:49.1234Z","revoked_at":null}"#,
    )
    .expect_err("finer than a millisecond");
    assert!(error.to_string().contains(SUB_MILLISECOND), "{error}");
}

#[test]
fn test_an_epoch_integer_or_an_offsetless_time_is_refused() {
    for body in [
        r#"{"created_at":1760725609123,"revoked_at":null}"#,
        r#"{"created_at":"2025-10-17T18:26:49.123","revoked_at":null}"#,
        r#"{"created_at":"not a time","revoked_at":null}"#,
    ] {
        assert!(serde_json::from_str::<Stamped>(body).is_err(), "{body}");
    }
}
