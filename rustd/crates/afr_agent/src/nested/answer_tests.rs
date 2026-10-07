//! A nested tool's answer never reads as an empty success.

use std::collections::BTreeMap;

use afd_core::error_code::INTERNAL_OPERATION_FAILED;
use afd_core::test_util::trace::Capture;

use super::answer::{EVENT_ANSWER_ENCODE_FAILED, json};

/// A map keyed by pairs, which JSON refuses: an object's keys are strings.
#[test]
fn test_an_answer_json_refuses_is_logged_and_reads_as_its_debug_form() {
    let capture = Capture::install();
    let refused: BTreeMap<(u8, u8), u8> = BTreeMap::from([((1, 2), 3)]);

    let output = json(&refused);

    assert_eq!(output.text, format!("{refused:?}"), "never an empty text");
    assert_eq!(output.error_code, None, "the call did what it says");
    let logged = capture.only(EVENT_ANSWER_ENCODE_FAILED);
    assert_eq!(logged.level, tracing::Level::WARN);
    assert_eq!(
        logged.field("error_code"),
        Some(INTERNAL_OPERATION_FAILED.as_str())
    );
}
