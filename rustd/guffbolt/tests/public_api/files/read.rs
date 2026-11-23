//! `file_read`: windows, long lines and the one-read budget.

use super::*;

#[tokio::test]
async fn file_read_pages_a_window_and_says_where_to_read_on() {
    let desk = Desk::new();
    desk.put("r.txt", "a\nb\nc\nd\ne\n");
    let tools = desk.tools(false);

    let window = call(
        &tools,
        "file_read",
        json!({"path": "r.txt", "offset": 2, "limit": 2}),
    )
    .await;
    let rest = call(&tools, "file_read", json!({"path": "r.txt", "offset": 4})).await;
    let whole = call(&tools, "file_read", json!({"path": "r.txt"})).await;

    assert_eq!(
        window.text,
        "b\nc\n... the file continues at line 4; read on with offset 4"
    );
    assert_eq!(rest.text, "d\ne\n");
    assert_eq!(whole.text, "a\nb\nc\nd\ne\n");
}

#[tokio::test]
async fn file_read_refuses_an_offset_past_the_end_or_zero() {
    let desk = Desk::new();
    desk.put("r.txt", "a\nb\n");
    desk.put("empty.txt", "");
    let tools = desk.tools(false);

    let past = call(&tools, "file_read", json!({"path": "r.txt", "offset": 9})).await;
    let past_empty = call(
        &tools,
        "file_read",
        json!({"path": "empty.txt", "offset": 2}),
    )
    .await;
    let first_of_empty = call(&tools, "file_read", json!({"path": "empty.txt"})).await;
    let zero = call(&tools, "file_read", json!({"path": "r.txt", "offset": 0})).await;
    let missing = call(&tools, "file_read", json!({"path": "nowhere.txt"})).await;

    assert_eq!(
        past.text,
        "[invalid_arguments] offset 9 is past the end of r.txt, which has 2 lines"
    );
    assert_eq!(
        past_empty.text,
        "[invalid_arguments] offset 2 is past the end of empty.txt, which has 0 lines"
    );
    assert_eq!(first_of_empty.text, "");
    assert_eq!(code(&first_of_empty), None);
    assert!(
        zero.text
            .starts_with("[invalid_arguments] invalid value: integer `0`"),
        "{}",
        zero.text
    );
    assert_eq!(code(&missing), Some("file_not_found"));
}

/// A line longer than the whole output budget is cut on a character
/// boundary, said so, and the read still moves on.
#[tokio::test]
async fn file_read_cuts_a_line_longer_than_the_budget_on_a_character_boundary() {
    let desk = Desk::new();
    // 13,334 three-byte characters: 40,002 bytes, past the 40,000-byte budget.
    let long = "\u{20ac}".repeat(13_334);
    desk.put("wide.txt", format!("{long}\ntail\n"));
    let tools = desk.tools(false);

    let output = call(&tools, "file_read", json!({"path": "wide.txt"})).await;

    let kept = "\u{20ac}".repeat(13_333);
    assert_eq!(
        output.text,
        format!(
            "{kept}\n... line 1 was cut at 39999 of its 40003 bytes\n... the file continues at \
             line 2; read on with offset 2"
        )
    );
}

/// A file longer than one read is paged up to there, and its last line
/// names `shell` for the rest.
#[tokio::test]
async fn file_read_past_one_read_names_shell_for_the_rest() {
    let desk = Desk::new();
    // 4,194,304 two-byte lines fill one read exactly; one more line is past it.
    desk.put("big.txt", format!("{}more\n", "x\n".repeat(ONE_READ / 2)));
    let tools = desk.tools(false);

    let last = call(
        &tools,
        "file_read",
        json!({"path": "big.txt", "offset": 4_194_304}),
    )
    .await;
    let past = call(
        &tools,
        "file_read",
        json!({"path": "big.txt", "offset": 4_194_305}),
    )
    .await;

    assert_eq!(
        last.text,
        "x\n... the file continues past 8388608 bytes; read the rest with shell"
    );
    assert_eq!(
        past.text,
        "[invalid_arguments] offset 4194305 is past the 4194304 lines of the first 8388608 \
         bytes of big.txt; read the rest with shell"
    );
}
