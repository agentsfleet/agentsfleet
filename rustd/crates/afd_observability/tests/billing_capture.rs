//! The billing families as they reach the wire: what a settlement's unpriced
//! tokens and a sweep's repairs add, and what a sweep that could not read the
//! ledger leaves behind.
//!
//! In the capture binary because both read values back, which only the
//! capturing install can do. Each case reads a delta or names its own series,
//! because the install and its counters are shared with the runner cases.

use afd_observability::metrics::declared::billing::{
    BILLING_HELD_NANOS, BILLING_SWEEPER_RESETTLED_TOTAL, BILLING_UNPRICED_TOKENS_TOTAL,
    BILLING_UNSETTLED_NANOS, BILLING_UNSETTLED_OLDEST_SECONDS,
};
use afd_observability::metrics::label::billing::WorkType;
use afd_observability::producers::billing::{self, Owed};
use afd_observability::semconv;
use afd_observability::test_util::Capture;

/// A provider and model no other case prices, so its series is this case's.
const PROVIDER: &str = "billing-capture-provider";
const MODEL: &str = "billing-capture-model";

/// Tokens a settlement carried with no published price.
const UNPRICED: u64 = 1_250;

/// Works one sweep settled.
const RESETTLED: u64 = 3;

/// One sweep's reading of what is still owed, every value distinct.
const OWED: Owed = Owed {
    unsettled_nanos: [5_000, 7_000],
    oldest_seconds: 90,
    held_nanos: 11_000,
};

/// Unpriced tokens count under the provider and model that carried them, and a
/// settlement that carried none adds nothing.
#[test]
fn should_count_unpriced_tokens_under_their_model() {
    let capture = Capture::install();
    let series = [
        (semconv::ATTR_PROVIDER_NAME, PROVIDER),
        (semconv::ATTR_REQUEST_MODEL, MODEL),
    ];
    let family = BILLING_UNPRICED_TOKENS_TOTAL.wire_name();

    billing::unpriced_tokens(PROVIDER, MODEL, 0);
    let after_none = capture.sum(family, &series);
    billing::unpriced_tokens(PROVIDER, MODEL, UNPRICED);

    assert_eq!(after_none, 0, "a fully priced settlement adds no series");
    assert_eq!(capture.sum(family, &series), UNPRICED);
}

/// A sweep's repairs count under their work type; a sweep that repaired
/// nothing adds nothing.
#[test]
fn should_count_only_the_works_a_sweep_settled() {
    let capture = Capture::install();
    let series = [(
        semconv::LABEL_WORK_TYPE,
        WorkType::AuthoringAttempt.as_str(),
    )];
    let family = BILLING_SWEEPER_RESETTLED_TOTAL.wire_name();
    let before = capture.sum(family, &series);

    billing::sweeper_resettled(WorkType::AuthoringAttempt, 0);
    let after_none = capture.sum(family, &series);
    billing::sweeper_resettled(WorkType::AuthoringAttempt, RESETTLED);

    assert_eq!(after_none, before, "an empty sweep counts nothing");
    assert_eq!(capture.sum(family, &series), before + RESETTLED);
}

/// A sweep's reading is published as it was read, and a sweep whose read
/// failed withdraws it: an operator sees a gap, never a stale balance or a
/// zero nobody measured.
#[test]
fn should_withdraw_what_is_owed_when_the_read_fails() {
    let capture = Capture::install();
    let fleet = [(semconv::LABEL_WORK_TYPE, WorkType::FleetLease.as_str())];
    let attempt = [(
        semconv::LABEL_WORK_TYPE,
        WorkType::AuthoringAttempt.as_str(),
    )];
    let unsettled = BILLING_UNSETTLED_NANOS.wire_name();
    let oldest = BILLING_UNSETTLED_OLDEST_SECONDS.wire_name();
    let held = BILLING_HELD_NANOS.wire_name();

    billing::owed_observed(Some(OWED));
    let published = (
        capture.gauge(unsettled, &fleet),
        capture.gauge(unsettled, &attempt),
        capture.gauge(oldest, &[]),
        capture.gauge(held, &[]),
    );
    billing::owed_observed(None);
    let withdrawn = (
        capture.gauge(unsettled, &fleet),
        capture.gauge(unsettled, &attempt),
        capture.gauge(oldest, &[]),
        capture.gauge(held, &[]),
    );

    assert_eq!(
        published,
        (
            Some(OWED.unsettled_nanos[0]),
            Some(OWED.unsettled_nanos[1]),
            Some(OWED.oldest_seconds),
            Some(OWED.held_nanos),
        ),
        "each cell carries what the sweep read"
    );
    assert_eq!(withdrawn, (None, None, None, None));
}
