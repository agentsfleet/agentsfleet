#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::tool_detail::{DETAIL_FIELD_MAX_BYTES, DETAIL_POST_MAX_BYTES, RawToolCallRecord};
use serde_json::json;

use super::{POST_ENVELOPE_BYTES, record};
use crate::fixture::{clean, clean_json};
use crate::trace::encoded_len;

/// Whether the daemon would keep `record`, judged by the wire's own narrow.
fn accepted(record: &afd_wire::tool_detail::ToolCallRecord<'_>) -> bool {
    let text = serde_json::to_string(record).unwrap();
    serde_json::from_str::<RawToolCallRecord<'_>>(&text)
        .unwrap()
        .narrow()
        .is_ok()
}

#[test]
fn should_keep_a_small_call_whole() {
    let kept = record(
        3,
        clean_json(json!({"url": "https://example.com"})),
        &clean("one\ntwo\n"),
    );

    assert_eq!(kept.call_number, 3);
    assert_eq!(kept.output, "one\ntwo\n");
    assert_eq!(kept.output_line_count, 2);
    assert!(!kept.truncated && !kept.truncated_arguments);
    assert!(accepted(&kept));
}

#[test]
fn should_cut_an_output_past_its_field_bound_and_count_every_line() {
    let output = "line\n".repeat(DETAIL_FIELD_MAX_BYTES / 4);

    let cut = record(1, clean_json(json!({})), &clean(&output));

    assert!(cut.truncated && cut.output.len() <= DETAIL_FIELD_MAX_BYTES);
    assert_eq!(cut.output_line_count, (DETAIL_FIELD_MAX_BYTES / 4) as u64);
    assert!(accepted(&cut));
}

#[test]
fn should_empty_arguments_past_their_field_bound() {
    let arguments = json!({"body": "x".repeat(DETAIL_FIELD_MAX_BYTES)});

    let cut = record(1, clean_json(arguments), &clean("ok"));

    assert!(cut.truncated_arguments && cut.arguments.is_empty());
    assert!(accepted(&cut));
}

#[test]
fn should_cut_an_output_that_escapes_past_one_post() {
    let output = "\u{1}".repeat(DETAIL_FIELD_MAX_BYTES);

    let cut = record(1, clean_json(json!({})), &clean(&output));

    let encoded = serde_json::to_vec(&cut).unwrap().len();
    assert!(cut.truncated && encoded + POST_ENVELOPE_BYTES <= DETAIL_POST_MAX_BYTES);
    assert_eq!(
        cut.output.len(),
        DETAIL_FIELD_MAX_BYTES / 2,
        "halved once, not emptied"
    );
    assert!(accepted(&cut));
}

/// An output whose uncut record encodes to exactly `target` bytes: control
/// characters escape to six bytes each, letters to one.
fn encoding_to(target: usize) -> String {
    let base = encoded_len(&record(1, clean_json(json!({})), &clean("")));
    let escaped = (target - base - DETAIL_FIELD_MAX_BYTES).div_ceil(5);
    let plain = target - base - 6 * escaped;
    format!("{}{}", "\u{1}".repeat(escaped), "a".repeat(plain))
}

#[test]
fn should_keep_a_record_that_exactly_fits_one_post() {
    let output = encoding_to(DETAIL_POST_MAX_BYTES - POST_ENVELOPE_BYTES);

    let kept = record(1, clean_json(json!({})), &clean(&output));

    assert!(!kept.truncated, "{} output bytes", output.len());
    assert_eq!(kept.output, output);
}

#[test]
fn should_cut_a_record_one_byte_past_one_post() {
    let output = encoding_to(DETAIL_POST_MAX_BYTES - POST_ENVELOPE_BYTES + 1);

    let cut = record(1, clean_json(json!({})), &clean(&output));

    assert!(cut.truncated && cut.output.len() < output.len());
}

#[test]
fn should_record_no_arguments_for_a_call_whose_arguments_are_not_an_object() {
    let kept = record(1, clean_json(json!("bare")), &clean("ok"));

    assert!(kept.arguments.is_empty() && !kept.truncated_arguments);
}
