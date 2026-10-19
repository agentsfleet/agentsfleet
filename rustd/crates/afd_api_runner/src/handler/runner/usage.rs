//! What a renewal or report whose usage will not read is told.
//!
//! Both bodies carry the run's request lines, and both are refused, field
//! named, when they carry the cumulative totals runners sent before usage moved
//! to one line per provider request. Those totals are the one failure here that
//! names a known field: an older runner sends them every time, so naming them
//! is what tells its operator to upgrade instead of reading "malformed".

use afd_validate::Sentences;
use afd_wire::usage::{LINES_MAX, RUNNER_SEQ_MAX};
use const_format::concatcp;

/// The retired cumulative prompt-token total.
const INPUT_TOKENS: &str = "input_tokens";
/// The retired cumulative cache-read total.
const CACHED_INPUT_TOKENS: &str = "cached_input_tokens";
/// The retired cumulative completion total.
const OUTPUT_TOKENS: &str = "output_tokens";

/// What every retired-total sentence says after the field it names.
const RETIRED_TAIL: &str =
    " is a retired cumulative total; send one line per provider request in requests";

/// Each retired cumulative total, with the sentence a body carrying it earns.
const RETIRED: [(&str, &str); 3] = [
    (INPUT_TOKENS, concatcp!(INPUT_TOKENS, RETIRED_TAIL)),
    (
        CACHED_INPUT_TOKENS,
        concatcp!(CACHED_INPUT_TOKENS, RETIRED_TAIL),
    ),
    (OUTPUT_TOKENS, concatcp!(OUTPUT_TOKENS, RETIRED_TAIL)),
];

/// The refusal a report carrying more lines than one body may earns.
pub(super) const DETAIL_LINES_MAX: &str =
    concatcp!("requests must carry at most ", LINES_MAX, " lines");

/// The refusal a report line numbered in the daemon's own range earns.
pub(super) const DETAIL_SEQ_MAX: &str =
    concatcp!("requests[].seq must be at most ", RUNNER_SEQ_MAX);

/// The refusal any other report body this daemon cannot read earns.
pub(super) const DETAIL_REPORT_MALFORMED: &str = "Malformed report body";

/// The report's request lines, by the path garde names when one breaks.
///
/// `RequestLines` is validated on its own, so its length is reported at the
/// empty path and a line's number at `[].seq`.
pub(super) const REPORT_LINES: Sentences = Sentences::new(
    &[("", DETAIL_LINES_MAX), ("[].seq", DETAIL_SEQ_MAX)],
    DETAIL_REPORT_MALFORMED,
);

/// The sentence for a body refused because it carries a retired cumulative
/// total, or `None` when that is not why it would not read.
pub(super) fn retired_total(failure: &serde_json::Error) -> Option<&'static str> {
    let field = afd_core::json::unknown_field_of(failure)?;
    RETIRED
        .iter()
        .find(|(name, _sentence)| *name == field)
        .map(|&(_name, sentence)| sentence)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "test module: an unmet precondition should fail the test loudly"
    )]

    use afd_wire::report::RenewRequest;
    use afd_wire::usage::RequestLines;

    use super::{DETAIL_LINES_MAX, DETAIL_SEQ_MAX, REPORT_LINES, retired_total};

    fn refusal(body: &str) -> serde_json::Error {
        afd_http::handler::read_body::<RenewRequest>(body.as_bytes())
            .expect_err("the body is one the renew shape refuses")
    }

    fn line(seq: u64) -> serde_json::Value {
        serde_json::json!({
            "seq": seq, "at": "2026-10-19T00:00:00Z", "input_tokens": 1,
            "cached_input_tokens": 0, "cache_write_tokens": 0,
            "cache_write_5m_tokens": 0, "cache_write_1h_tokens": 0, "output_tokens": 1,
        })
    }

    fn picked(lines: Vec<serde_json::Value>) -> &'static str {
        let lines: RequestLines =
            serde_json::from_value(serde_json::Value::Array(lines)).expect("lines read");
        let report = garde::Validate::validate(&lines).expect_err("the lines break a bound");
        REPORT_LINES.pick(&report)
    }

    /// The cumulative shape an older runner sends names its first field.
    #[test]
    fn a_cumulative_body_names_its_retired_field() {
        let failure = refusal(r#"{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}"#);
        let sentence = retired_total(&failure).expect("a retired total is named");
        assert!(sentence.starts_with("input_tokens "), "{sentence}");
        assert!(sentence.contains("requests"), "{sentence}");

        for field in ["cached_input_tokens", "output_tokens"] {
            let failure = refusal(&format!(r#"{{"{field}":1}}"#));
            let sentence = retired_total(&failure).expect("a retired total is named");
            assert!(sentence.starts_with(field), "{sentence}");
        }
    }

    /// Any other unreadable body is not the retired shape, so the renewal keeps
    /// its default-safe answer.
    #[test]
    fn other_failures_are_not_the_retired_shape() {
        for body in [
            "not-json",
            r#"{"tokens":1}"#,
            r#"{"requests":"x"}"#,
            r#"{"requests":[{"input_tokens":"sk-live"}]}"#,
        ] {
            assert_eq!(retired_total(&refusal(body)), None, "{body}");
        }
    }

    /// A refused report says which bound its lines broke.
    #[test]
    fn a_report_names_the_bound_its_lines_broke() {
        let over = (0..=afd_wire::usage::LINES_MAX as u64).map(line).collect();
        assert_eq!(picked(over), DETAIL_LINES_MAX);
        assert!(DETAIL_LINES_MAX.contains(&afd_wire::usage::LINES_MAX.to_string()));

        let daemon_range = vec![line(u64::from(afd_wire::usage::DAEMON_SEQ_BASE))];
        assert_eq!(picked(daemon_range), DETAIL_SEQ_MAX);
        assert!(DETAIL_SEQ_MAX.contains(&afd_wire::usage::RUNNER_SEQ_MAX.to_string()));
    }
}
