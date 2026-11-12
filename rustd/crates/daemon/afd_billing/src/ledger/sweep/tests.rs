use af_core::id::{ENTROPY_LEN, Uuid7};
use af_wire::usage::LineKind;
use jiff::Timestamp;

use super::{grouped, row::UnsettledRow};
use crate::biller::WorkKey;
use crate::sql::work;

fn id(byte: u8) -> Uuid7 {
    Uuid7::encode(Timestamp::UNIX_EPOCH, [byte; ENTROPY_LEN])
        .unwrap_or_else(|_unencodable| unreachable!("the epoch encodes"))
}

/// One unsettled setup-chat line of `work`, of `kind` and number `seq`.
fn row(work: u8, work_type: &str, kind: &str, seq: i64) -> UnsettledRow {
    UnsettledRow {
        work_type: work_type.to_owned(),
        work_id: id(work),
        kind: kind.to_owned(),
        seq,
        tenant_id: id(2),
        workspace_id: id(3),
        fleet_id: None,
        fleet_name: None,
        event_id: None,
        provider: "anthropic".to_owned(),
        model: "claude-opus-5-5".to_owned(),
        posture: "platform".to_owned(),
        requested_at: Timestamp::UNIX_EPOCH,
        input_token_count: 0,
        cached_input_token_count: 0,
        cache_write_token_count: 0,
        cache_write_5m_token_count: 0,
        cache_write_1h_token_count: 0,
        output_token_count: 0,
        runtime_ms: 0,
        price_id: None,
        unpriced_token_count: 0,
        charged_nanos: 1,
        logged_at: Timestamp::UNIX_EPOCH,
        line_id: id(9),
    }
}

/// A model line and a runtime line may share a number; their kinds keep
/// them apart, and the settlement carries each as the kind it is.
#[test]
fn a_work_settles_its_lines_by_kind_and_number() {
    let settlements = grouped(vec![
        row(1, work::AUTHORING_ATTEMPT, LineKind::Runtime.as_str(), 0),
        row(1, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0),
    ]);
    let [settlement] = settlements.as_slice() else {
        unreachable!("one work, one settlement: {settlements:?}");
    };
    assert_eq!(settlement.work, WorkKey::AuthoringAttempt(id(1)));
    let lines: Vec<(LineKind, u32)> = settlement
        .lines
        .iter()
        .map(|line| (line.usage.kind, line.usage.seq))
        .collect();
    assert_eq!(lines, [(LineKind::Model, 0), (LineKind::Runtime, 0)]);
}

#[test]
fn a_work_whose_line_kind_this_build_cannot_name_is_left_whole() {
    let settlements = grouped(vec![
        row(1, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0),
        row(1, work::AUTHORING_ATTEMPT, "storage", 1),
        row(4, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0),
    ]);
    let works: Vec<&WorkKey> = settlements
        .iter()
        .map(|settlement| &settlement.work)
        .collect();
    assert_eq!(
        works,
        [&WorkKey::AuthoringAttempt(id(4))],
        "the work with an unnamed kind stays unsettled, whole: settling only \
         its known lines would undercharge it"
    );
}

#[test]
fn a_work_type_this_build_cannot_name_stays_unsettled() {
    let settlements = grouped(vec![
        row(1, "future_work", LineKind::Model.as_str(), 0),
        row(4, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0),
    ]);
    assert_eq!(settlements.len(), 1, "{settlements:?}");
}

/// A posture this build cannot name is not priced as the platform's: a
/// guess would settle a tenant's own-key usage at the platform's rates.
#[test]
fn a_posture_this_build_cannot_name_stays_unsettled_rather_than_settling_as_platform() {
    let mut unreadable = row(1, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0);
    "self-managed-by-another-name".clone_into(&mut unreadable.posture);
    let settlements = grouped(vec![
        unreadable,
        row(4, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0),
    ]);
    let works: Vec<&WorkKey> = settlements
        .iter()
        .map(|settlement| &settlement.work)
        .collect();
    assert_eq!(works, [&WorkKey::AuthoringAttempt(id(4))]);
}

/// Skipping is never silent: each distinct unreadable spelling is named,
/// and a later pass over the same rows adds no second line.
#[cfg(feature = "test-util")]
#[test]
fn what_the_relay_skips_is_named_once() {
    use af_core::test_util::trace::Capture;

    let capture = Capture::install();
    let rows = || {
        let mut bad_posture = row(1, work::AUTHORING_ATTEMPT, LineKind::Model.as_str(), 0);
        "posture-named-by-the-sweep-test".clone_into(&mut bad_posture.posture);
        vec![
            bad_posture,
            row(
                3,
                work::AUTHORING_ATTEMPT,
                "kind-named-by-the-sweep-test",
                0,
            ),
            row(
                2,
                "work-type-named-by-the-sweep-test",
                LineKind::Model.as_str(),
                0,
            ),
        ]
    };

    for _pass in 0..3 {
        assert_eq!(
            grouped(rows()).len(),
            0,
            "an unreadable group settles nothing, pass after pass"
        );
    }

    let mut spellings: Vec<String> = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("billing_relay_skipped"))
        .filter_map(|event| event.field("spelling").map(str::to_owned))
        .collect();
    spellings.sort();
    assert_eq!(
        spellings,
        [
            "kind-named-by-the-sweep-test",
            "posture-named-by-the-sweep-test",
            "work-type-named-by-the-sweep-test",
        ],
        "three spellings, each named once across three passes"
    );
}
