#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use garde::Validate as _;

use super::{LINES_MAX, RUNTIME_MS_MAX, UsageLine, UsageLines};

const AT: &str = "2026-10-19T10:00:00.123Z";
const PROMPT: u32 = 1_000;
const CACHED: u32 = 200;
const WRITE: u32 = 30;
const WRITE_5M: u32 = 40;
const WRITE_1H: u32 = 50;

fn line(seq: u32) -> UsageLine {
    UsageLine {
        seq,
        at: crate::instant::parse(AT).expect("the fixture instant parses"),
        input_tokens: PROMPT,
        cached_input_tokens: CACHED,
        cache_write_tokens: WRITE,
        cache_write_5m_tokens: WRITE_5M,
        cache_write_1h_tokens: WRITE_1H,
        output_tokens: 7,
        runtime_ms: 1_500,
    }
}

#[test]
fn the_whole_prompt_counts_cache_reads_and_every_cache_write() {
    let expected = u64::from(PROMPT + CACHED + WRITE + WRITE_5M + WRITE_1H);
    assert_eq!(line(0).whole_prompt(), expected);
}

#[test]
fn a_line_round_trips_with_its_rfc3339_instant() {
    let json = serde_json::to_value(line(3)).expect("a line serialises");
    assert_eq!(json.get("at").and_then(serde_json::Value::as_str), Some(AT));
    let back: UsageLine = serde_json::from_value(json).expect("a line deserialises");
    assert_eq!(back, line(3));
}

#[test]
fn an_unknown_field_is_refused() {
    let mut json = serde_json::to_value(line(0)).expect("a line serialises");
    json.as_object_mut()
        .expect("a line is an object")
        .insert("reasoning_tokens".to_owned(), serde_json::json!(5));
    serde_json::from_value::<UsageLine>(json).expect_err("an unknown field is refused");
}

#[test]
fn runtime_past_an_hour_is_refused() {
    let mut long = line(0);
    long.runtime_ms = RUNTIME_MS_MAX + 1;
    long.validate().expect_err("past an hour is refused");
    long.runtime_ms = RUNTIME_MS_MAX;
    long.validate().expect("exactly an hour is allowed");
}

#[test]
fn a_report_carries_one_to_the_most_lines() {
    UsageLines(Vec::new())
        .validate()
        .expect_err("an empty report is refused");
    UsageLines(vec![line(0)])
        .validate()
        .expect("one line is allowed");
    let most = (0..LINES_MAX)
        .map(|seq| line(u32::try_from(seq).expect("fits")))
        .collect();
    UsageLines(most)
        .validate()
        .expect("the most lines are allowed");
    let over = (0..=LINES_MAX)
        .map(|seq| line(u32::try_from(seq).expect("fits")))
        .collect();
    UsageLines(over)
        .validate()
        .expect_err("one past the most is refused");
}

#[test]
fn a_bad_line_inside_a_report_is_refused() {
    let mut bad = line(1);
    bad.runtime_ms = RUNTIME_MS_MAX + 1;
    UsageLines(vec![line(0), bad])
        .validate()
        .expect_err("a bad line inside a report is refused");
}
