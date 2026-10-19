#![expect(
    clippy::expect_used,
    reason = "the input is built to be refused; an acceptance is the failure"
)]

use std::error::Error as _;

/// Private text a stored row could hold; it must never render from the error.
const SECRET: &str = "sk-live-private-draft-message";

#[test]
fn an_unreadable_stored_row_never_renders_the_private_text_it_held() {
    let refusal = serde_json::from_value::<u64>(serde_json::Value::from(SECRET))
        .expect_err("a string is not a counter");
    let error = super::stored(refusal);

    let mut rendered = vec![error.to_string(), format!("{error:?}")];
    let mut cause = error.source();
    while let Some(link) = cause {
        rendered.push(link.to_string());
        rendered.push(format!("{link:?}"));
        cause = link.source();
    }

    assert!(rendered.len() > 2, "the stored cause is kept: {rendered:?}");
    for text in &rendered {
        assert!(!text.contains(SECRET), "leaked through the chain: {text}");
    }
    assert!(
        rendered
            .iter()
            .any(|text| text.contains("invalid type: string, expected u64")),
        "the expected type survives: {rendered:?}"
    );
}

#[test]
fn a_column_read_back_empty_is_a_stored_fault_naming_only_the_column() {
    let error = super::absent("review_context");

    assert_eq!(error.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    assert!(!error.is_datastore_unavailable());
    let cause = error
        .source()
        .expect("the missing column is kept as the cause");
    assert!(
        cause.to_string().contains("missing field `review_context`"),
        "the cause names the column: {cause}"
    );
}
