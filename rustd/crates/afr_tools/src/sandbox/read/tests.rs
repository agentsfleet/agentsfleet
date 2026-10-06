#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fmt::Write as _;

use serde_json::{Value, json};

use super::{LINES_DEFAULT, Page};
use crate::catalog::FILE_READ;
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::sandbox::output;
use crate::testing::{Live, call_in, hosted, offered};

/// The file the live reads page through.
const NUMBERS: &str = "numbers.txt";

/// `count` lines, each naming its own number.
fn numbered(count: usize) -> String {
    let mut lines = String::new();
    for number in 1..=count {
        writeln!(lines, "line {number}").unwrap();
    }
    lines
}

/// `file_read` called on `live` with `arguments`.
async fn read(live: &Live, arguments: Value) -> ToolOutput {
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[FILE_READ.name()]).unwrap();
    call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &mut Lease::default(),
        arguments,
    )
    .await
}

#[test]
fn a_file_that_fits_is_read_whole_and_says_nothing_more() {
    let page = Page::of("a\nb\n", 1, LINES_DEFAULT, 100).unwrap();

    assert_eq!(page.text, "a\nb\n");
    assert_eq!(page.next, None);
    assert_eq!(page.read_back(false), "a\nb\n");
}

#[test]
fn a_window_starts_at_its_offset_and_stops_at_its_limit() {
    let page = Page::of(&numbered(5), 2, 2, 100).unwrap();

    assert_eq!(page.text, "line 2\nline 3\n");
    assert_eq!(
        page.read_back(false),
        "line 2\nline 3\n... the file continues at line 4; read on with offset 4"
    );
}

/// Whatever the budget and the limit, reading on from where each page says
/// gives back the whole file, every line once, in order.
#[test]
fn paging_from_where_each_page_says_reads_the_whole_file_once() {
    let file = numbered(300);
    // Every line fits the smallest budget, so none is cut; under the real
    // one, the limit is what stops each page.
    let in_use = output::budget(None);
    for (budget, limit) in [(64, LINES_DEFAULT), (in_use, 7), (9, 3), (4096, 1)] {
        let mut read = String::new();
        let mut first = 1;
        loop {
            let page = Page::of(&file, first, limit, budget).unwrap();
            assert!(page.text.len() <= budget, "{budget}/{limit}");
            read.push_str(&page.text);
            match page.next {
                Some(next) => {
                    assert!(next > first, "a page always moves on");
                    first = next;
                }
                None => break,
            }
        }
        assert_eq!(read, file, "{budget}/{limit}");
    }
}

/// A line alone past the budget is cut on a character boundary and says so;
/// the next read starts at the line after it.
#[test]
fn a_line_longer_than_the_budget_is_cut_and_says_so() {
    let wide = format!("{}é\nnext\n", "x".repeat(9));

    let page = Page::of(&wide, 1, LINES_DEFAULT, 10).unwrap();

    assert_eq!(page.text, "x".repeat(9), "é would cross the budget");
    assert_eq!(page.next, Some(2));
    assert_eq!(
        page.read_back(false),
        format!(
            "{}\n... line 1 was cut at 9 of its 12 bytes\n... the file continues at line 2; read on with offset 2",
            "x".repeat(9)
        )
    );
}

/// A line exactly as long as the budget fits it: kept whole, with no note
/// that it was cut.
#[test]
fn a_line_exactly_the_budget_is_kept_whole_and_says_nothing_of_a_cut() {
    let exact = format!("{}\nnext\n", "x".repeat(9));

    let page = Page::of(&exact, 1, LINES_DEFAULT, 10).unwrap();

    assert_eq!(page.text, format!("{}\n", "x".repeat(9)));
    assert_eq!(page.cut, None);
    assert_eq!(page.next, Some(2));
}

/// A cut line is alone on its page: a character straddling the budget moves
/// the cut back a few bytes, and no short next line may take that room, or
/// the model would read line 2 as the end of line 1 and the next offset
/// would skip it.
#[test]
fn a_cut_line_is_alone_on_its_page() {
    let wide = format!("{}😀\n}}\nnext\n", "x".repeat(7));

    let page = Page::of(&wide, 1, LINES_DEFAULT, 10).unwrap();

    assert_eq!(page.text, "x".repeat(7));
    assert_eq!(page.cut, Some((1, 7, 12)));
    assert_eq!(page.next, Some(2), "line 2 is read on its own page");
}

#[test]
fn an_offset_past_the_last_line_is_none_and_an_empty_file_reads_empty() {
    assert_eq!(Page::of("a\n", 2, 1, 100), None);
    assert_eq!(Page::of("", 2, 1, 100), None);
    assert_eq!(Page::of("", 1, 1, 100).unwrap().read_back(false), "");
}

#[tokio::test]
async fn test_file_read_pages_a_large_file_within_the_budget() {
    let live = Live::start().await;
    let file = numbered(20_000);
    std::fs::write(live.root.join(NUMBERS), &file).unwrap();

    let whole = read(&live, json!({"path": NUMBERS})).await;
    let window = read(
        &live,
        json!({"path": NUMBERS, "offset": 19_999, "limit": 5}),
    )
    .await;

    assert_eq!(whole.error_code, None, "{whole:?}");
    assert!(whole.text.len() < 41_000, "{} bytes", whole.text.len());
    assert!(
        whole
            .text
            .ends_with("... the file continues at line 2001; read on with offset 2001"),
        "the default limit of 2000 lines fits the budget: {}",
        whole.text.lines().last().unwrap()
    );
    assert_eq!(window.text, "line 19999\nline 20000\n");
    live.stop().await;
}

#[tokio::test]
async fn an_offset_or_limit_that_is_zero_or_past_the_end_is_refused() {
    let live = Live::start().await;
    std::fs::write(live.root.join(NUMBERS), numbered(3)).unwrap();

    let past = read(&live, json!({"path": NUMBERS, "offset": 4})).await;
    let zero_offset = read(&live, json!({"path": NUMBERS, "offset": 0})).await;
    let zero_limit = read(&live, json!({"path": NUMBERS, "limit": 0})).await;

    assert_eq!(
        past.text,
        "[invalid_arguments] offset 4 is past the end of numbers.txt, which has 3 lines"
    );
    for refused in [&zero_offset, &zero_limit] {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{refused:?}"
        );
    }
    live.stop().await;
}
