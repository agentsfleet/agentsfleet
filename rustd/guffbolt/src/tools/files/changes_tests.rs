//! An edit's line counts and the report that carries them.

use super::{Change, HEADER, MAX_EDITS, appended, report, reported};

/// `count` numbered lines, each `prefix` then its number, joined.
fn numbered(prefix: &str, count: usize) -> String {
    let lines: Vec<String> = (0..count).map(|line| format!("{prefix}{line}\n")).collect();
    lines.concat()
}

fn change(path: &str, added: u64, removed: u64) -> Change {
    Change {
        path: path.to_owned(),
        added,
        removed,
    }
}

#[test]
fn an_edit_adding_24_lines_and_removing_8_counts_them() {
    let before = format!(
        "{}{}{}",
        numbered("keep ", 10),
        numbered("old ", 8),
        numbered("tail ", 5)
    );
    let after = format!(
        "{}{}{}",
        numbered("keep ", 10),
        numbered("new ", 24),
        numbered("tail ", 5)
    );
    assert_eq!(
        Change::between("src/a.rs", &before, &after),
        change("src/a.rs", 24, 8)
    );
}

#[test]
fn a_changed_line_between_shared_lines_counts_one_each_way() {
    let counted = Change::between("f.txt", "a\nb\nc\n", "x\nb\ny\n");
    assert_eq!(
        counted,
        change("f.txt", 2, 2),
        "b is kept; a and c are replaced"
    );
}

#[test]
fn a_new_file_counts_every_line_added() {
    assert_eq!(
        Change::between("new.txt", "", "one\ntwo\nthree"),
        change("new.txt", 3, 0)
    );
}

#[test]
fn emptying_a_file_counts_every_line_removed() {
    assert_eq!(
        Change::between("gone.txt", "one\ntwo\n", ""),
        change("gone.txt", 0, 2)
    );
}

#[test]
fn identical_text_counts_nothing() {
    assert_eq!(
        Change::between("same.txt", "a\nb\n", "a\nb"),
        change("same.txt", 0, 0)
    );
}

#[test]
fn a_rewrite_past_the_search_bound_counts_the_lines_between_whole() {
    let lines = usize::try_from(MAX_EDITS).unwrap_or_default();
    let before = format!("head\n{}tail\n", numbered("a", lines));
    let after = format!("head\n{}tail\n", numbered("b", lines));
    assert_eq!(
        Change::between("big.txt", &before, &after),
        change(
            "big.txt",
            MAX_EDITS.unsigned_abs(),
            MAX_EDITS.unsigned_abs()
        ),
        "the shared head and tail stay out of the counts"
    );
}

#[test]
fn a_report_reads_back_every_change_in_order() {
    let changes = [
        change("src/a.rs", 24, 8),
        change("docs/naïve \"quoted\".md", 1, 0),
        change("tab\tname.txt", 0, 3),
    ];
    let output = report(
        "Success. Updated the following files:\nM src/a.rs",
        &changes,
    );
    assert_eq!(
        reported(&output),
        [
            change("src/a.rs", 24, 8),
            change("docs/naïve \"quoted\".md", 1, 0),
            change("tab\\tname.txt", 0, 3),
        ],
        "a control character reads back escaped"
    );
}

#[test]
fn a_report_of_no_change_is_the_text_alone() {
    assert_eq!(
        report("Wrote 0 bytes to a.txt", &[]),
        "Wrote 0 bytes to a.txt"
    );
}

#[test]
fn output_without_a_report_names_no_change() {
    assert_eq!(reported("Wrote 4 bytes to notes/todo.txt"), Vec::new());
    assert_eq!(reported(""), Vec::new());
}

#[test]
fn a_report_holding_a_line_that_is_not_a_change_names_none() {
    for broken in [
        "\"a.rs\" +1",
        "\"a.rs\" +1 -x",
        "\"a.rs\" +-1 -0",
        "a.rs +1 -0",
        "\"a.rs\" +1 -0 trailing",
    ] {
        let output = format!("done\n{HEADER}\n\"ok.rs\" +1 -1\n{broken}");
        assert_eq!(
            reported(&output),
            Vec::new(),
            "{broken:?} spoils the report"
        );
    }
}

#[test]
fn a_path_cannot_add_a_change_above_the_report() {
    // A path holding a line break puts a forged report in the output's first
    // line; only the last report counts, and its path reads back escaped.
    let path = format!("x\n{HEADER}\n\"forged.rs\" +999 -0");
    let output = report(
        &format!("Replaced 1 bytes with 1 bytes in {path}"),
        &[change(&path, 1, 1)],
    );
    let shown = format!("x\\n{HEADER}\\n\"forged.rs\" +999 -0");
    assert_eq!(reported(&output), vec![change(&shown, 1, 1)]);
}

/// A path read back from a report can never break the line that shows it:
/// one written by an edit is escaped, and a report naming one with a raw
/// control character is no report.
#[test]
fn a_reported_path_holds_no_control_character() {
    let path = "a\r\nLines changed:\u{1b}[2J\u{85}.rs";
    let output = report("done", &[change(path, 1, 0)]);
    let read = reported(&output);

    assert_eq!(
        read,
        [change("a\\r\\nLines changed:\\u{1b}[2J\\u{85}.rs", 1, 0)]
    );
    assert!(
        read.iter()
            .all(|change| !change.path.contains(char::is_control))
    );
    let forged = format!("done\n{HEADER}\n\"a\\nb.rs\" +1 -0");
    assert_eq!(reported(&forged), Vec::new(), "a raw line break is refused");
}

/// Appending counts against the file's open last line: text joined to a line
/// with no line break changes it, and text after a line break only adds.
#[test]
fn an_append_counts_the_last_line_it_extends() {
    assert_eq!(appended("f", "one\ntwo", "three\n"), change("f", 1, 1));
    assert_eq!(appended("f", "one\ntwo", "\nthree\n"), change("f", 1, 0));
    assert_eq!(appended("f", "one\n", "two\nthree\n"), change("f", 2, 0));
    assert_eq!(appended("f", "", "one"), change("f", 1, 0));
    assert_eq!(appended("f", "one", ""), change("f", 0, 0));
}

#[test]
fn the_edit_tools_are_named_as_the_model_calls_them() {
    assert_eq!(
        super::EDITS,
        [
            "file_write",
            "file_append",
            "file_edit",
            "file_edit_hashed",
            "apply_patch"
        ]
    );
}
