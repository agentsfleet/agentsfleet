//! The metadata-only wire rejects text, caller digests and unbounded outcomes.

use super::{accounting, report};
use afd_wire::authoring::Report;
use serde_json::json;

#[test]
fn report_wire_is_flat_and_accounting_has_no_answer_or_caller_digest() {
    let request = report();
    let valid = serde_json::to_value(&request).expect("terminal wire");
    let decoded: Report = serde_json::from_value(valid).expect("flat terminal fields parse");
    assert_eq!(
        decoded.accounting().report_id,
        request.accounting().report_id
    );
    let base = serde_json::to_value(accounting(&request)).expect("accounting wire");
    let _: Report = serde_json::from_value(base.clone()).expect("metadata form parses");
    for field in [
        "answer",
        "accounting_digest",
        "content_digest",
        "tools",
        "url",
        "diagnostic",
    ] {
        let mut forged = base.clone();
        forged
            .as_object_mut()
            .expect("report object")
            .insert(field.into(), json!("private"));
        assert!(serde_json::from_value::<Report>(forged).is_err(), "{field}");
    }
}
