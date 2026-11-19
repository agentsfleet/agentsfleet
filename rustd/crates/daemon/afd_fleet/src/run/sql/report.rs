//! The terminal report's run read, for a report whose stream session kept
//! no facts from the offer. The report commits in one statement,
//! [`super::report_statement::COMMIT_REPORT`].

/// The run a report is about, scoped to the runner presenting it.
///
/// The `runner_id` predicate IS the ownership check, and it is why a foreign
/// run id and an unknown one answer the same 404: this statement cannot see
/// another runner's row, so the handler has nothing to leak. An id-only lookup
/// with an ownership comparison afterwards would put the two facts in different
/// places and make the endpoint an oracle for which run ids are live.
///
/// `status` is deliberately ABSENT from the predicate. The report statement
/// applies it, and reading it here as well would let a run flip between the
/// two statements and answer 404 for a row the statement would have fenced —
/// two different refusals for one race, decided by timing.
///
/// `$1` run, `$2` runner.
pub const SELECT_RUN_FOR_REPORT: &str = include_str!("select_run_for_report.sql");
