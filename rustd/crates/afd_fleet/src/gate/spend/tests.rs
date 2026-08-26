//! Every way a write mint is refused, decided against a row literal.
//!
//! No datastore takes part: the transaction's job is to hold the row still, and
//! the VERDICT is [`Locked::examine`]'s, which is why it is a pure function.
//! What the integration lane then has to prove is only that two concurrent
//! spends cannot both succeed — not what each refusal means.
use afd_fleet_runtime::config::{Access, RepositoryBinding};

use super::{Locked, WriteApproval};
use crate::gate::decision::Status;
use crate::gate::detail::REPOSITORY_WRITE_SPEND_CEILING;

/// When the card would have lapsed.
const TIMEOUT_AT: i64 = 1_760_000_060_000;

/// An answer that landed comfortably inside the deadline.
const ANSWERED_AT: i64 = 1_760_000_030_000;

/// The reach this fleet declares, as the config states it.
fn declared() -> RepositoryBinding {
    RepositoryBinding::from_parts(
        vec!["acme/payments".into()],
        Access::Write,
        Some("main".into()),
    )
}

/// The reach the card recorded, as `Recorded` serialises it.
const STATED: &str = r#"{"repositories":["acme/payments"],"access":"write","base":"main"}"#;

/// A row that passes every check, which each case below then breaks one of.
fn approved() -> Locked {
    Locked {
        id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1f01".to_owned(),
        status: Status::Approved.as_str().to_owned(),
        stated_binding: Some(STATED.to_owned()),
        timeout_at: TIMEOUT_AT,
        answered_at: Some(ANSWERED_AT),
        spend_count: Some(0),
        spend_ceiling: Some(REPOSITORY_WRITE_SPEND_CEILING),
    }
}

#[test]
fn an_approved_unchanged_gate_with_allowance_left_is_spendable() {
    let row = approved();
    assert_eq!(row.examine(&declared()), Ok(row.id.as_str()));
}

#[test]
fn a_gate_no_human_has_approved_is_unapproved() {
    // Pending and denied are one answer here: neither is a yes, and a caller
    // able to tell them apart would be invited to treat pending as a maybe.
    for status in [Status::Pending, Status::Denied] {
        let row = Locked {
            status: status.as_str().to_owned(),
            ..approved()
        };
        assert_eq!(row.examine(&declared()), Err(WriteApproval::Unapproved));
    }
}

#[test]
fn an_answer_that_arrived_after_the_deadline_is_not_an_answer() {
    // A human answering a card that had already lapsed. The row says approved
    // and the approval is not one this mint may spend.
    let late = Locked {
        answered_at: Some(TIMEOUT_AT + 1),
        ..approved()
    };
    assert_eq!(late.examine(&declared()), Err(WriteApproval::Unapproved));

    // And a row claiming approval with no answer stamped at all is not a row
    // this daemon writes — refused rather than read generously.
    let unstamped = Locked {
        answered_at: None,
        ..approved()
    };
    assert_eq!(
        unstamped.examine(&declared()),
        Err(WriteApproval::Unapproved)
    );
}

#[test]
fn a_reach_the_fleet_no_longer_declares_is_drift() {
    // The approval-to-mint drift this check exists for: both the gate rules and
    // the binding ride `config_json`, PATCHable under the scope that wakes the
    // fleet, so a repository added after the answer must not be written to.
    for drifted in [
        // Another repository entirely.
        r#"{"repositories":["acme/other"],"access":"write","base":"main"}"#,
        // The same repository at a narrower reach than the fleet now declares.
        r#"{"repositories":["acme/payments"],"access":"read"}"#,
        // A second repository the human was never shown.
        r#"{"repositories":["acme/payments","acme/other"],"access":"write","base":"main"}"#,
    ] {
        let row = Locked {
            stated_binding: Some(drifted.to_owned()),
            ..approved()
        };
        assert_eq!(
            row.examine(&declared()),
            Err(WriteApproval::BindingDrift),
            "{drifted}"
        );
    }
}

#[test]
fn an_unrecorded_reach_authorises_nothing() {
    // There is nothing to compare against, and unknown reach must never be the
    // permissive branch — so it fails as drift rather than falling through.
    let unrecorded = Locked {
        stated_binding: None,
        ..approved()
    };
    assert_eq!(
        unrecorded.examine(&declared()),
        Err(WriteApproval::BindingDrift)
    );
}

#[test]
fn a_spent_allowance_is_exhausted_and_a_missing_one_is_not_an_allowance() {
    let spent = Locked {
        spend_count: Some(REPOSITORY_WRITE_SPEND_CEILING),
        ..approved()
    };
    assert_eq!(spent.examine(&declared()), Err(WriteApproval::Exhausted));

    // One request left is still one request.
    let last = Locked {
        spend_count: Some(REPOSITORY_WRITE_SPEND_CEILING - 1),
        ..approved()
    };
    assert_eq!(last.examine(&declared()), Ok(last.id.as_str()));

    // A row raised with no allowance columns at all is not an allowance of
    // zero — it is a row this build did not raise, and it is not spendable.
    for absent in [
        (None, Some(REPOSITORY_WRITE_SPEND_CEILING)),
        (Some(0), None),
    ] {
        let row = Locked {
            spend_count: absent.0,
            spend_ceiling: absent.1,
            ..approved()
        };
        assert_eq!(row.examine(&declared()), Err(WriteApproval::Unapproved));
    }
}

#[test]
fn the_reach_is_checked_before_the_allowance() {
    // Order matters: a gate approved for a reach the fleet no longer declares
    // must be refused as DRIFT even when it has requests left, because the
    // remedy is a human's and not a wait. The reverse order would tell an
    // operator to re-approve an allowance that was never the problem.
    let drifted_and_spent = Locked {
        stated_binding: Some(r#"{"repositories":["acme/other"],"access":"write"}"#.to_owned()),
        spend_count: Some(REPOSITORY_WRITE_SPEND_CEILING),
        ..approved()
    };
    assert_eq!(
        drifted_and_spent.examine(&declared()),
        Err(WriteApproval::BindingDrift)
    );
}
