//! Instants on the wire: jiff's standard RFC 3339 in UTC, any offset in,
//! nothing finer than a millisecond accepted.
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

/// Instants whose fractions jiff writes at different widths.
const SAMPLE_MS: [i64; 5] = [
    1_760_725_609_000,
    1_760_725_609_100,
    1_760_725_609_120,
    1_760_725_609_123,
    -1,
];

/// The instant `millis` after the epoch.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("in range")
}

#[test]
fn test_an_instant_prints_jiff_standard_rfc3339_in_utc() {
    for (millis, text) in [
        (1_760_725_609_000, "2025-10-17T18:26:49Z"),
        (1_760_725_609_100, "2025-10-17T18:26:49.1Z"),
        (1_760_725_609_120, "2025-10-17T18:26:49.12Z"),
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
fn test_a_written_instant_matches_jiff_and_reads_back_unchanged() {
    for millis in SAMPLE_MS {
        let written = afd_instant(at(millis));
        assert_eq!(written, at(millis).to_string(), "{millis}ms");
        let read: Stamped = serde_json::from_value(serde_json::json!({
            "created_at": written,
            "revoked_at": written,
        }))
        .unwrap();
        assert_eq!(read.created_at, at(millis), "{millis}ms");
        assert_eq!(read.revoked_at, Some(at(millis)), "{millis}ms");
    }
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
        r#"{"created_at":"2025-10-17T18:26:49.123Z","revoked_at":"2025-10-17T18:26:49Z"}"#
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
