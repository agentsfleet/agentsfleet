//! The billing families as they reach the wire: what a settlement's unpriced
//! tokens and a funds check add.
//!
//! In the capture binary because both read values back, which only the
//! capturing install can do. Each case reads a delta or names its own series,
//! because the install and its counters are shared with the runner cases.

use af_observability::metrics::declared::billing::{
    BILLING_FUNDS_CHECK_TOTAL, BILLING_UNPRICED_TOKENS_TOTAL,
};
use af_observability::metrics::label::billing::{FundsOutcome, WorkType};
use af_observability::producers::billing;
use af_observability::semconv;
use af_observability::test_util::Capture;

/// A provider and model no other case prices, so its series is this case's.
const PROVIDER: &str = "billing-capture-provider";
const MODEL: &str = "billing-capture-model";

/// Tokens a settlement carried with no published price.
const UNPRICED: u64 = 1_250;

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

/// Each funds check counts once under its work type and its answer.
#[test]
fn should_count_each_funds_check_under_its_answer() {
    let capture = Capture::install();
    let series = [
        (semconv::LABEL_WORK_TYPE, WorkType::FleetRun.as_str()),
        (semconv::LABEL_OUTCOME, FundsOutcome::Short.as_str()),
    ];
    let family = BILLING_FUNDS_CHECK_TOTAL.wire_name();
    let before = capture.sum(family, &series);

    billing::funds_checked(WorkType::FleetRun, FundsOutcome::Short);

    assert_eq!(capture.sum(family, &series), before + 1);
}
