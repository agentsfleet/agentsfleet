//! What the declared bounds actually refuse.
//!
//! **The bounds are enumerable, so they are tested exactly.** Every
//! `garde(length)` and `garde(range)` in this crate names a constant. A value
//! at the limit must be accepted and a value one past it refused, and those two
//! points are the entire question — a generator drawing random strings would
//! have to be astronomically lucky to land on either, so random search is the
//! weaker tool here, not the stronger one. Each row below is that pair.
//!
//! The steer request's rows live in `validation_steer.rs`, and the parser's
//! generated half — what a malformed body does — in `validation_mutation.rs`.
//!
//! # What this does NOT cover
//!
//! `garde` runs at the service boundary, not in this crate — `afd_wire` is the
//! definition and validation belongs where a request arrives. These rows prove
//! the DECLARATION rejects what it says it rejects. They do not prove any
//! handler calls `validate()` on its way in; that is the call site's own test.
//! For the runner path that call site is `afd_runner::bounds::accept`, reached
//! from `heartbeat.rs`.

use std::borrow::Cow;

use afd_wire::event::{OPERATION_ID_MAX_BYTES, STEER_MESSAGE_MAX_BYTES};
use afd_wire::runner::{
    CHECK_DETAIL_MAX_BYTES, CHECK_NAME_MAX_BYTES, SELFTEST_CHECKS_MAX, SELFTEST_POLICY_MAX_BYTES,
    SelftestCheck, SelftestReport,
};
use garde::Validate as _;

/// A string of exactly `bytes` ASCII characters.
///
/// ASCII deliberately: `garde(length(bytes, ...))` counts BYTES, so building
/// the probe from multi-byte characters would make the boundary arithmetic a
/// second thing to get right and a failure ambiguous between the two.
pub(crate) fn of_len(bytes: usize) -> String {
    "a".repeat(bytes)
}

fn check_named(name: String) -> SelftestCheck<'static> {
    SelftestCheck {
        name: Cow::Owned(name),
        ok: true,
        detail: Cow::Borrowed("ok"),
    }
}

fn report_with(checks: Vec<SelftestCheck<'static>>) -> SelftestReport<'static> {
    SelftestReport {
        checks,
        all_ok: true,
        sandbox_tier: Cow::Borrowed("landlock_full"),
        network_policy: Cow::Borrowed("allow_list_egress"),
    }
}

/// The bounds themselves, pinned to their values.
///
/// Every other row in this file builds its probe from the same constant it
/// asserts against, so the pair moves together: widen `CHECK_NAME_MAX_BYTES` to
/// a megabyte and those rows stay green while the cap they were written to
/// defend is gone. Proven, not assumed — loosening that constant to 100000 left
/// the whole file passing before this row existed.
///
/// So the numbers are written out once, here. A bound may absolutely be moved;
/// this makes moving it a line in a diff someone reviews rather than a silent
/// widening.
#[test]
fn the_declared_bounds_are_the_numbers_this_wire_was_designed_around() {
    assert_eq!(CHECK_NAME_MAX_BYTES, 128, "CHECK_NAME_MAX_BYTES");
    assert_eq!(CHECK_DETAIL_MAX_BYTES, 256, "CHECK_DETAIL_MAX_BYTES");
    assert_eq!(SELFTEST_CHECKS_MAX, 32, "SELFTEST_CHECKS_MAX");
    assert_eq!(SELFTEST_POLICY_MAX_BYTES, 64, "SELFTEST_POLICY_MAX_BYTES");
    assert_eq!(STEER_MESSAGE_MAX_BYTES, 8192, "STEER_MESSAGE_MAX_BYTES");
    assert_eq!(OPERATION_ID_MAX_BYTES, 200, "OPERATION_ID_MAX_BYTES");
}

/// A check whose every field sits exactly at its limit is accepted, and one
/// byte past any of them is refused.
///
/// `SelftestCheck` is the type a compromised or buggy host has the most direct
/// reach into: the name and detail are prose it chooses, and they land in an
/// operator's log. The bound is what stops a host writing a megabyte there.
#[test]
fn a_selftest_check_accepts_its_limits_and_refuses_one_byte_past_them() {
    let at_limit = SelftestCheck {
        name: Cow::Owned(of_len(CHECK_NAME_MAX_BYTES)),
        ok: true,
        detail: Cow::Owned(of_len(CHECK_DETAIL_MAX_BYTES)),
    };
    assert!(at_limit.validate().is_ok(), "a check at its limits");

    let long_name = SelftestCheck {
        name: Cow::Owned(of_len(CHECK_NAME_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(long_name.validate().is_err(), "name one byte past its cap");

    let long_detail = SelftestCheck {
        detail: Cow::Owned(of_len(CHECK_DETAIL_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(
        long_detail.validate().is_err(),
        "detail one byte past its cap"
    );
}

/// The `min = 1` half, which is a different failure from the cap.
///
/// An empty name is not a short name, it is a check that names nothing — the
/// operator reading the log learns which check failed from this field and
/// nowhere else.
#[test]
fn a_selftest_check_refuses_an_empty_name_or_detail() {
    let empty_name = check_named(String::new());
    assert!(empty_name.validate().is_err(), "an empty name");

    let empty_detail = SelftestCheck {
        name: Cow::Borrowed("landlock"),
        ok: true,
        detail: Cow::Borrowed(""),
    };
    assert!(empty_detail.validate().is_err(), "an empty detail");
}

/// A report carrying its maximum number of checks is accepted; one more is not.
#[test]
fn a_selftest_report_accepts_its_full_roster_and_refuses_one_more() {
    let full = report_with(
        (0..SELFTEST_CHECKS_MAX)
            .map(|index| check_named(format!("check_{index}")))
            .collect(),
    );
    assert!(full.validate().is_ok(), "a full roster");

    let over = report_with(
        (0..=SELFTEST_CHECKS_MAX)
            .map(|index| check_named(format!("check_{index}")))
            .collect(),
    );
    assert!(over.validate().is_err(), "one check past the roster cap");
}

/// `dive` reaches inside the roster, so a bad check fails the whole report.
///
/// This is the row that proves `dive` is doing something. Without it a report
/// could carry a check with a megabyte name and pass, because the outer type
/// only counts the roster.
#[test]
fn one_oversized_check_inside_a_legal_roster_still_fails_the_report() {
    let smuggled = report_with(vec![
        check_named("fine".to_owned()),
        check_named(of_len(CHECK_NAME_MAX_BYTES + 1)),
    ]);
    assert!(
        smuggled.validate().is_err(),
        "an oversized check inside a roster within its count"
    );
}

/// The report's own policy strings carry the same at-limit / one-past pair.
#[test]
fn a_selftest_report_bounds_the_policy_strings_it_echoes() {
    let at_limit = SelftestReport {
        checks: vec![check_named("landlock".to_owned())],
        all_ok: true,
        sandbox_tier: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES)),
        network_policy: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES)),
    };
    assert!(at_limit.validate().is_ok(), "policy strings at their limit");

    let over = SelftestReport {
        sandbox_tier: Cow::Owned(of_len(SELFTEST_POLICY_MAX_BYTES + 1)),
        ..at_limit.clone()
    };
    assert!(over.validate().is_err(), "a policy string past its cap");

    let empty = SelftestReport {
        network_policy: Cow::Borrowed(""),
        ..at_limit
    };
    assert!(empty.validate().is_err(), "an empty policy string");
}
