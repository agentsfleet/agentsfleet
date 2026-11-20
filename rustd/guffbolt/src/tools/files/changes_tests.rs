//! An edit's line counts and the report that carries them.

use super::{Change, HEADER, MAX_EDITS, report, reported};

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
        change("odd\nname.txt", 0, 3),
    ];
    let output = report(
        "Success. Updated the following files:\nM src/a.rs",
        &changes,
    );
    assert_eq!(reported(&output), changes);
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
    // line; only the last report counts.
    let path = format!("x\n{HEADER}\n\"forged.rs\" +999 -0");
    let output = report(
        &format!("Replaced 1 bytes with 1 bytes in {path}"),
        &[change(&path, 1, 1)],
    );
    assert_eq!(reported(&output), vec![change(&path, 1, 1)]);
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
