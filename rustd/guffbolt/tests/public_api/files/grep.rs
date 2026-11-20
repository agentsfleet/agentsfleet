//! `grep` over a small tree.

use super::*;

#[tokio::test]
async fn grep_reads_back_each_matching_line_with_its_place() {
    let desk = Desk::new();
    tree(&desk);
    desk.put("bin.dat", [0xff_u8, b'T', b'O', b'D', b'O']);
    let tools = desk.tools(false);

    let regex = call(&tools, "grep", json!({"pattern": "fn (alpha|beta)"})).await;
    let literal = call(
        &tools,
        "grep",
        json!({"pattern": "todo!()", "literal": true}),
    )
    .await;
    let only_text = call(&tools, "grep", json!({"pattern": "TODO", "glob": "*.txt"})).await;
    let one_file = call(
        &tools,
        "grep",
        json!({"pattern": "TODO", "path": "src/a.rs"}),
    )
    .await;
    let below = call(&tools, "grep", json!({"pattern": "fn", "path": "src/deep"})).await;
    let accented = call(&tools, "grep", json!({"pattern": "Caf\u{00e9}"})).await;
    let limited = call(&tools, "grep", json!({"pattern": "fn", "limit": 1})).await;

    assert_eq!(
        regex.text,
        "src/a.rs:1: fn alpha() {}\nsrc/b.rs:1: fn beta() {}"
    );
    assert_eq!(literal.text, "src/deep/c.rs:1: fn gamma() { todo!() }");
    assert_eq!(only_text.text, "docs/guide.txt:2: TODO later");
    assert_eq!(one_file.text, "src/a.rs:2: // TODO: tidy");
    assert_eq!(
        below.text, "c.rs:1: fn gamma() { todo!() }",
        "relative to the path searched"
    );
    assert_eq!(accented.text, "docs/guide.txt:1: Caf\u{00e9} guide");
    assert_eq!(
        limited.text.lines().count(),
        2,
        "one line and the note: {}",
        limited.text
    );
    assert!(limited.text.ends_with(LIMIT_REACHED));
}

/// `ignore_case` matches a pattern in any case, as the tool's schema says.
#[tokio::test]
async fn grep_with_ignore_case_matches_any_case() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let output = call(
        &tools,
        "grep",
        json!({"pattern": "todo", "ignore_case": true, "glob": "*.txt"}),
    )
    .await;

    assert_eq!(output.text, "docs/guide.txt:2: TODO later");
}

/// The pattern is a regular expression, so a class such as `\\d` matches a
/// digit.
#[tokio::test]
async fn grep_reads_a_digit_class() {
    let desk = Desk::new();
    desk.put("n.txt", "none\nport 8080\n");
    let tools = desk.tools(false);

    let output = call(&tools, "grep", json!({"pattern": "\\d+"})).await;

    assert_eq!(output.text, "n.txt:2: port 8080");
}

#[tokio::test]
async fn grep_cuts_a_long_line_and_stops_at_the_output_budget() {
    let desk = Desk::new();
    let long = format!("hit {}", "\u{00e9}".repeat(600));
    desk.put("long.txt", format!("{long}\n").repeat(100));
    let tools = desk.tools(false);

    let output = call(&tools, "grep", json!({"pattern": "hit"})).await;

    let shown: String = long.chars().take(500).collect();
    let first = output.text.lines().next().expect("a line");
    assert_eq!(first, format!("long.txt:1: {shown}"));
    let lines = output.text.lines().count();
    assert!(
        lines < 100,
        "the 40,000-byte budget stops it first: {lines} lines"
    );
    assert!(output.text.ends_with(LIMIT_REACHED));
}

#[tokio::test]
async fn grep_refuses_a_pattern_or_path_it_cannot_use() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let bad_regex = call(&tools, "grep", json!({"pattern": "fn ("})).await;
    let outside = call(&tools, "grep", json!({"pattern": "x", "path": "../etc"})).await;
    let missing_file = call(
        &tools,
        "grep",
        json!({"pattern": "x", "path": "nowhere.rs"}),
    )
    .await;
    let empty = call(&tools, "grep", json!({"pattern": "nothing matches this"})).await;

    assert!(
        bad_regex
            .text
            .starts_with("[invalid_arguments] regex parse error"),
        "{}",
        bad_regex.text
    );
    assert_eq!(outside.text, format!("[path_not_allowed] ../etc {LEAVES}"));
    assert_eq!(
        missing_file.text, "",
        "a file that cannot be read holds no line"
    );
    assert_eq!(empty.text, "");
}
