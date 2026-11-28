//! Families whose values a test reads back through the capturing exporter.
//!
//! # Why its own binary
//!
//! `producers::install` and `Capture::install` both write process-wide
//! `OnceLock`s, and `test_util` says a binary installing its own provider may
//! not share with a suite that installs another. `producers_drive.rs` installs
//! a plain provider and asserts no value; this one installs the capture and
//! asserts values, so the two cannot be the same binary.
#![cfg(feature = "test-util")]

#[path = "billing_capture.rs"]
mod billing_capture;

#[path = "durable_task_capture.rs"]
mod durable_task_capture;

#[test]
fn web_dispatches_are_counted_by_vendor_operation_and_outcome() {
    use af_observability::{
        metrics::declared::fleet::WEB_CALLS_TOTAL, producers::fleet::web, semconv,
        test_util::Capture,
    };

    let capture = Capture::install();
    for (vendor, operation, outcome) in [
        ("parallel", "search", "failure"),
        ("exa", "search", "success"),
        ("parallel", "extract", "success"),
    ] {
        web::dispatched(vendor, operation, outcome);
        let labels = [
            (semconv::LABEL_VENDOR, vendor),
            (semconv::LABEL_OPERATION, operation),
            (semconv::LABEL_OUTCOME, outcome),
        ];
        assert_eq!(capture.sum(WEB_CALLS_TOTAL.wire_name(), &labels), 1);
    }
}
