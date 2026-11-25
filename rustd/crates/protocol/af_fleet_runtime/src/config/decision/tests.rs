#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{CompletionStatement, MAX_STATEMENT_ID_BYTES};
use crate::error::ErrorKind;

const ONE: &str = r#"[{"id": "pr_green", "question": "Are PR 88's checks green?", "criteria": {"true": "Every required check passed.", "false": "A required check failed or is pending."}}]"#;

fn refused(document: &str) -> crate::Error {
    CompletionStatement::parse_all(document).expect_err("the statements are refused")
}

#[test]
fn a_statement_reads_its_id_question_and_both_criteria() {
    let statements = CompletionStatement::parse_all(ONE).expect("one complete statement");

    assert_eq!(statements.len(), 1);
    let statement = statements.first().expect("one statement");
    assert_eq!(statement.id(), "pr_green");
    assert_eq!(statement.question().text(), "Are PR 88's checks green?");
    assert_eq!(
        statement.question().criterion("true"),
        Some("Every required check passed.")
    );
    assert_eq!(statement.question().criteria().len(), 2);
    assert_eq!(statement.question().criterion("DONE"), None);
}

#[test]
fn a_statement_serializes_in_the_shape_it_was_read() {
    let statements = CompletionStatement::parse_all(ONE).expect("one complete statement");
    let written = serde_json::to_string(&statements).expect("statements serialize");

    assert_eq!(
        CompletionStatement::parse_all(&written).expect("its own output reads back"),
        statements
    );
}

#[test]
fn an_empty_list_is_refused() {
    assert!(matches!(
        refused("[]").kind(),
        ErrorKind::OutOfBounds { .. }
    ));
}

#[test]
fn a_statement_that_is_not_an_object_is_refused() {
    for document in [
        r#"[["pr_green", "q", {}]]"#,
        r#"["pr_green"]"#,
        "{}",
        "null",
    ] {
        assert!(
            matches!(refused(document).kind(), ErrorKind::InvalidFieldType { .. }),
            "{document}"
        );
    }
}

#[test]
fn a_missing_field_is_named() {
    for (document, field) in [
        (
            r#"[{"question": "q", "criteria": {"true": "y", "false": "n"}}]"#,
            "id",
        ),
        (
            r#"[{"id": "a", "criteria": {"true": "y", "false": "n"}}]"#,
            "question",
        ),
        (r#"[{"id": "a", "question": "q"}]"#, "criteria"),
    ] {
        let failure = refused(document);
        assert!(
            matches!(failure.kind(), ErrorKind::MissingRequiredField { field: named } if *named == field),
            "{document}: {failure:?}"
        );
    }
}

#[test]
fn criteria_other_than_true_and_false_are_refused() {
    let failure =
        refused(r#"[{"id": "a", "question": "q", "criteria": {"true": "y", "DONE": "n"}}]"#);
    assert!(matches!(failure.kind(), ErrorKind::InvalidStatement { .. }));

    let three = refused(
        r#"[{"id": "a", "question": "q", "criteria": {"true": "y", "false": "n", "x": "z"}}]"#,
    );
    assert!(matches!(three.kind(), ErrorKind::OutOfBounds { .. }));
}

#[test]
fn two_statements_with_one_id_are_refused() {
    let statement = r#"{"id": "a", "question": "q", "criteria": {"true": "y", "false": "n"}}"#;
    let failure = refused(&format!("[{statement}, {statement}]"));

    assert!(matches!(failure.kind(), ErrorKind::InvalidStatement { .. }));
}

#[test]
fn blank_nul_or_unknown_text_is_refused() {
    for document in [
        r#"[{"id": "a", "question": "   ", "criteria": {"true": "y", "false": "n"}}]"#,
        r#"[{"id": "a", "question": "q\u0000", "criteria": {"true": "y", "false": "n"}}]"#,
        r#"[{"id": "a", "question": "q", "criteria": {"true": " ", "false": "n"}}]"#,
        r#"[{"id": "a b", "question": "q", "criteria": {"true": "y", "false": "n"}}]"#,
        r#"[{"id": "ünï", "question": "q", "criteria": {"true": "y", "false": "n"}}]"#,
    ] {
        assert!(
            matches!(refused(document).kind(), ErrorKind::OutOfBounds { .. }),
            "{document}"
        );
    }
}

#[test]
fn an_id_at_its_ceiling_reads_and_one_past_it_is_refused() {
    let statement = |id: &str| {
        format!(r#"[{{"id": "{id}", "question": "q", "criteria": {{"true": "y", "false": "n"}}}}]"#)
    };
    let at = "a".repeat(MAX_STATEMENT_ID_BYTES);
    let past = "a".repeat(MAX_STATEMENT_ID_BYTES + 1);

    CompletionStatement::parse_all(&statement(&at)).expect("an id at its ceiling reads");
    assert!(matches!(
        refused(&statement(&past)).kind(),
        ErrorKind::OutOfBounds { .. }
    ));
}

#[test]
fn an_unknown_key_is_refused_by_its_path() {
    let failure = refused(
        r#"[{"id": "a", "question": "q", "criteria": {"true": "y", "false": "n"}, "weight": 2}]"#,
    );
    assert!(
        matches!(failure.kind(), ErrorKind::UnsupportedKey { path } if path.ends_with("weight")),
        "{failure:?}"
    );
}
