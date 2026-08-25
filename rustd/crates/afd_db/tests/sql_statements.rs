//! The Postgres lexing rules a migration splitter has to get right.
//!
//! Every case `sql_splitter_test.zig` covers, run against the Rust scanner with
//! the same inputs — the same oracle `tests/zig_parity.rs` uses for the crypto
//! crate, and for the same reason: the Zig suite encodes what the daemon
//! already survives in production, so re-running its assertions proves more
//! than any fixture generated here could.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::sql::{SplitError, SqlStatements};

/// Splits `sql`, asserting it is structurally sound first.
fn split(sql: &str) -> Vec<&str> {
    SqlStatements::new(sql)
        .expect("input must be structurally sound")
        .collect()
}

#[test]
fn test_splits_simple_statements_on_semicolons() {
    assert_eq!(
        split("SELECT 1; SELECT 2;"),
        vec!["SELECT 1", "SELECT 2"],
        "the terminator is not part of the statement"
    );
}

/// A `;` inside a literal is data. Splitting on it truncates the statement and
/// applies the truncated half.
#[test]
fn test_semicolons_inside_literals_are_data() {
    assert_eq!(
        split("INSERT INTO t VALUES ('a;b'); SELECT 1;"),
        vec!["INSERT INTO t VALUES ('a;b')", "SELECT 1"]
    );
    assert_eq!(
        split("SELECT 'it''s here; still inside'; SELECT 2;"),
        vec!["SELECT 'it''s here; still inside'", "SELECT 2"]
    );
}

/// A function body is one statement no matter how many `;` it contains.
#[test]
fn test_dollar_quoted_bodies_are_one_statement() {
    let sql = "CREATE FUNCTION f() RETURNS int AS $$ BEGIN a; b; RETURN 1; END $$ LANGUAGE plpgsql; SELECT 1;";
    let statements = split(sql);
    assert_eq!(statements.len(), 2, "got {statements:?}");
    assert!(statements[0].contains("RETURN 1;"));
}

/// The closing delimiter must match the opener verbatim, so a different tag
/// inside the body is inert.
#[test]
fn test_a_different_tag_does_not_close_a_dollar_quote() {
    let statements = split("SELECT $outer$ contains $inner$ and ; $outer$; SELECT 2;");
    assert_eq!(statements.len(), 2, "got {statements:?}");
    assert!(statements[0].contains("$inner$"));
}

/// Postgres puts no length cap on a tag and allows high-bit bytes in it.
#[test]
fn test_tags_may_be_long_and_non_ascii() {
    let statements = split("SELECT $ünïcödé_tag$ body ; here $ünïcödé_tag$; SELECT 2;");
    assert_eq!(statements.len(), 2, "got {statements:?}");
}

/// `$1` is a bind parameter, and a lone `$` is punctuation. Neither opens a
/// quoted body — reading them as one swallows the rest of the file.
#[test]
fn test_positional_parameters_do_not_open_a_dollar_quote() {
    assert_eq!(
        split("SELECT * FROM t WHERE a = $1 AND b = $2; SELECT 2;"),
        vec!["SELECT * FROM t WHERE a = $1 AND b = $2", "SELECT 2"]
    );
}

/// Maximal munch: `a$b$` is one identifier, not `a` followed by an opener.
#[test]
fn test_a_dollar_glued_to_an_identifier_is_part_of_it() {
    assert_eq!(
        split("SELECT a$b$ ; SELECT 1;"),
        vec!["SELECT a$b$", "SELECT 1"],
        "Postgres folds a$b$ into one identifier, so the ; is a boundary"
    );
    assert_eq!(
        split("SELECT 'x'$b$ ; $b$;"),
        vec!["SELECT 'x'$b$ ; $b$"],
        "after a non-identifier byte the delimiter DOES open"
    );
}

/// Comments contribute no boundary and no content between statements.
#[test]
fn test_comments_are_skipped_between_statements() {
    assert_eq!(
        split("-- leading; comment with 'apostrophe\nSELECT 1;"),
        vec!["SELECT 1"],
        "a leading comment is not part of the statement"
    );
    assert_eq!(
        split("/* block ; with $$ markers */ SELECT 1;"),
        vec!["SELECT 1"]
    );
    assert_eq!(
        split("/* outer /* inner ; */ still outer ; */ SELECT 1;"),
        vec!["SELECT 1"],
        "Postgres block comments nest"
    );
}

/// A comment-only or empty file yields nothing rather than an empty statement.
#[test]
fn test_nothing_to_apply_yields_no_statements() {
    for sql in ["", "   \n\t ", "-- only a comment\n", "/* only */"] {
        assert!(split(sql).is_empty(), "{sql:?} produced statements");
    }
}

/// `;;;;` is not a statement, and must not recurse — the validating scan walks
/// this same path, so a stack-hungry implementation fails on untrusted input.
#[test]
fn test_a_long_run_of_empty_statements_yields_nothing() {
    assert!(split(&";".repeat(50_000)).is_empty());
}

/// Trailing SQL with no terminator still applies; a file that forgets its last
/// `;` is not a file that silently skips its last statement.
#[test]
fn test_trailing_statement_without_a_terminator_is_returned() {
    assert_eq!(split("SELECT 1; SELECT 2"), vec!["SELECT 1", "SELECT 2"]);
}

/// A quoted identifier makes `;`, `--` and `'` inert, exactly as a literal does.
#[test]
fn test_quoted_identifiers_make_everything_inert() {
    let statements = split(r#"SELECT "weird;name--with'quote" FROM t; SELECT 2;"#);
    assert_eq!(statements.len(), 2, "got {statements:?}");
    assert!(statements[0].contains("weird;name--with'quote"));
}

/// `E'…'` honours backslash escapes and a plain literal does not, so `\'`
/// closes one and not the other (`standard_conforming_strings`).
#[test]
fn test_escape_strings_honour_backslashes_and_plain_strings_do_not() {
    // E'…' — the `\'` stays inside, so the `;` between them is not a boundary.
    assert_eq!(
        split(r"INSERT INTO t VALUES (E'a\'; b'); SELECT 1;"),
        vec![r"INSERT INTO t VALUES (E'a\'; b')", "SELECT 1"]
    );
    assert_eq!(
        split(r"SELECT e'x\'y';"),
        vec![r"SELECT e'x\'y'"],
        "the prefix is case-insensitive"
    );

    // A plain literal treats the backslash literally, so `'a\'` is a complete
    // string and the `;` after it IS a boundary. Same input, different split —
    // which is the whole reason the two states are tracked apart.
    assert_eq!(
        split(r"SELECT 'a\'; SELECT 'b';"),
        vec![r"SELECT 'a\'", "SELECT 'b'"]
    );
}

/// A standalone `E` opens an escape string; an `E` that ends an identifier does
/// not — `CASE 'x'` is not an escape string.
#[test]
fn test_only_a_standalone_e_opens_an_escape_string() {
    let statements = split(r"SELECT CASE 'a\' WHEN 1 THEN 2 END; SELECT 2;");
    assert_eq!(statements.len(), 2, "got {statements:?}");
}

/// Every unterminated region is named, and named distinctly — an operator
/// reading the failure has to know which one to go and close.
#[test]
fn test_every_unterminated_region_is_named() {
    for (sql, expected) in [
        ("SELECT 'unterminated", SplitError::UnterminatedString),
        (
            r#"SELECT "unterminated"#,
            SplitError::UnterminatedQuotedIdentifier,
        ),
        (
            "SELECT $$ unterminated",
            SplitError::UnterminatedDollarQuote,
        ),
        (
            "SELECT 1; /* unterminated",
            SplitError::UnterminatedBlockComment,
        ),
    ] {
        let error = SqlStatements::new(sql).expect_err("must be refused");
        assert_eq!(error, expected, "{sql:?}");
    }
}

/// Terminated quoting in every supported form is accepted.
#[test]
fn test_terminated_quoting_is_accepted_in_every_form() {
    for sql in [
        "SELECT 'closed';",
        r#"SELECT "closed";"#,
        "SELECT $$closed$$;",
        "SELECT $tag$closed$tag$;",
        "/* closed */ SELECT 1;",
    ] {
        assert!(SqlStatements::new(sql).is_ok(), "{sql:?} was refused");
    }
}

/// The constructor is the gate: a malformed file cannot become a statement
/// list at all, so no caller can apply half of one by forgetting to validate.
#[test]
fn test_a_malformed_file_never_becomes_a_statement_list() {
    let truncated = "INSERT INTO t VALUES ('a; DROP TABLE t;";
    assert!(
        SqlStatements::new(truncated).is_err(),
        "a truncated literal must not split into applicable statements"
    );
}

/// An inline comment after the SQL rides along with its statement, which is
/// what makes a migration's own annotations survive to the server log.
#[test]
fn test_an_inline_trailing_comment_stays_with_its_statement() {
    let statements = split("SELECT 1 -- why\n; SELECT 2;");
    assert_eq!(statements.len(), 2, "got {statements:?}");
    assert!(statements[0].contains("-- why"));
}

/// A file that opens on a literal must not read the byte before the file.
///
/// `E'…'` detection looks one byte back to tell a standalone `E` from the tail
/// of an identifier such as `CASE`. At offset zero there is no byte to look at,
/// and the guard that says so is the difference between a wrong answer and a
/// panic on input the splitter does not control.
#[test]
fn test_a_literal_at_the_start_of_the_file_is_not_an_escape_string() {
    assert_eq!(split(r"SELECT 1;"), vec!["SELECT 1"]);
    assert_eq!(
        split(r"'\'; SELECT 2;"),
        vec![r"'\'", "SELECT 2"],
        "with no byte before it, the opening quote is a plain literal, so the \
         backslash is inert and the literal closes at the next quote"
    );
}

/// `""` inside a quoted identifier is an escaped quote, not the end of one.
///
/// Reading it as the end leaves the scanner in SQL mode inside an identifier,
/// where the next `;` becomes a boundary and the statement is applied in half.
#[test]
fn test_doubled_quotes_inside_a_quoted_identifier_do_not_close_it() {
    let statements = split(r#"SELECT "we""ird;name" FROM t; SELECT 2;"#);
    assert_eq!(statements.len(), 2, "got {statements:?}");
    assert!(
        statements[0].contains(r#""we""ird;name""#),
        "got {statements:?}"
    );
}

/// A comment INSIDE a statement is skipped by the same rules as one between
/// statements — a different code path, and the one a real migration hits.
#[test]
fn test_comments_inside_a_statement_are_skipped() {
    assert_eq!(
        split("SELECT 1 /* ; not a boundary */ + 2; SELECT 3;"),
        vec!["SELECT 1 /* ; not a boundary */ + 2", "SELECT 3"]
    );
    assert_eq!(
        split("SELECT 1 -- ; not a boundary\n + 2; SELECT 3;"),
        vec!["SELECT 1 -- ; not a boundary\n + 2", "SELECT 3"]
    );
}

/// A `$tag` the file never closes is not a delimiter.
///
/// The delimiter scan runs off the end of the input looking for the closing
/// `$`. Treating what it found as a tag would put the scanner into dollar-quote
/// mode forever and swallow every remaining statement.
#[test]
fn test_an_unclosed_dollar_tag_at_end_of_input_is_not_a_delimiter() {
    assert_eq!(
        split("SELECT 1; SELECT $tag"),
        vec!["SELECT 1", "SELECT $tag"],
        "the trailing statement is returned, not swallowed as a quote body"
    );
}
