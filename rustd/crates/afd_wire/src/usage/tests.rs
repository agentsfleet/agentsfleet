#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use garde::Validate as _;

use super::{
    DAEMON_SEQ_BASE, LINES_MAX, RUNNER_SEQ_MAX, RUNTIME_MS_MAX, RequestLines, RequestUsage, UsageLine,
};

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

fn request(seq: u32) -> RequestUsage {
    let full = line(seq);
    RequestUsage {
        seq,
        at: full.at,
        input_tokens: full.input_tokens,
        cached_input_tokens: full.cached_input_tokens,
        cache_write_tokens: full.cache_write_tokens,
        cache_write_5m_tokens: full.cache_write_5m_tokens,
        cache_write_1h_tokens: full.cache_write_1h_tokens,
        output_tokens: full.output_tokens,
    }
}

#[test]
fn a_batch_carries_none_to_the_most_requests() {
    for (count, valid) in [(0, true), (1, true), (LINES_MAX, true), (LINES_MAX + 1, false)] {
        let batch = RequestLines(
            (0..count)
                .map(|seq| request(u32::try_from(seq).expect("fits")))
                .collect(),
        );
        assert_eq!(batch.validate().is_ok(), valid, "{count} requests");
    }
}

#[test]
fn a_runner_numbers_below_the_daemons_range() {
    request(RUNNER_SEQ_MAX).validate().expect("the top of the runner range");
    RequestLines(vec![request(0), request(DAEMON_SEQ_BASE)])
        .validate()
        .expect_err("a request in the daemon's range is refused");
}

#[test]
fn a_request_claims_no_runtime_and_a_runtime_line_no_tokens() {
    let request_line = request(4).line();
    assert_eq!(request_line.runtime_ms, 0);
    assert_eq!(request_line.whole_prompt(), line(4).whole_prompt());
    let runtime = UsageLine::runtime(DAEMON_SEQ_BASE, line(0).at, 2_000);
    assert_eq!((runtime.whole_prompt(), runtime.output_tokens), (0, 0));
    assert_eq!(runtime.runtime_ms, 2_000);
}

#[test]
fn a_request_carrying_runtime_is_refused() {
    let mut json = serde_json::to_value(request(0)).expect("a request serialises");
    json.as_object_mut()
        .expect("a request is an object")
        .insert("runtime_ms".to_owned(), serde_json::json!(5));
    serde_json::from_value::<RequestUsage>(json).expect_err("the daemon measures runtime");
}
