//! The local scheduler: jobs kept in a file, handed over once each when the
//! embedder asks what is due, and every refusal named.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition or a missing key should fail the test loudly"
)]

use std::time::{Duration, SystemTime};

use jiff::Timestamp;
use serde_json::Value;

use super::{Change, Due, Job, LocalScheduler, Page, Scheduler};
use crate::CallRef;
use crate::tools::code;

/// The call that adds each case's job.
fn adding() -> CallRef {
    CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    }
}

/// A daily job at 09:00 in `timezone`.
fn daily(timezone: &str, once: bool) -> Job<'_> {
    Job {
        cron: "0 9 * * *",
        timezone: Some(timezone),
        message: "daily check",
        once,
    }
}

/// The answer's JSON.
fn read(answer: &str) -> Value {
    serde_json::from_str(answer).expect("the scheduler answers JSON")
}

/// The moment the answered job next fires.
fn next_of(answer: &Value) -> Timestamp {
    answer["next_run_at"]
        .as_str()
        .expect("a next moment")
        .parse()
        .expect("an RFC 3339 instant")
}

/// `moment`, plus `span`, as the embedder's clock reads it.
fn after(moment: Timestamp, span: Duration) -> SystemTime {
    let later = moment.saturating_add(span).expect("in range");
    SystemTime::try_from(later).expect("a system time")
}

#[tokio::test]
async fn local_scheduler_returns_a_due_job_after_reopening() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let added = {
        let scheduler = LocalScheduler::open(directory.path()).expect("an empty directory opens");
        let answer = (scheduler.add(&adding(), daily("UTC", false)).await).expect("a valid job");
        read(&answer)
    };
    let next = next_of(&added);
    let past = after(next, Duration::from_mins(1));

    let reopened = LocalScheduler::open(directory.path()).expect("the kept file opens");
    let due = reopened.due(past).expect("the file is written through");
    let again = reopened.due(past).expect("nothing more is due");
    let third = LocalScheduler::open(directory.path()).expect("the kept file opens");

    assert_eq!(
        due,
        [Due {
            id: added["schedule_id"].as_str().expect("an id").to_owned(),
            message: "daily check".to_owned(),
            at: SystemTime::try_from(next).expect("a system time"),
        }]
    );
    assert_eq!(again, [], "the job is handed over once");
    assert_eq!(third.due(past).expect("readable"), [], "the fire was kept");
}

#[tokio::test]
async fn a_job_is_not_due_before_its_moment() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let answer = (scheduler.add(&adding(), daily("UTC", false)).await).expect("a valid job");
    let next = next_of(&read(&answer));

    let before = SystemTime::try_from(
        next.saturating_sub(Duration::from_secs(1))
            .expect("in range"),
    )
    .expect("a system time");

    assert_eq!(scheduler.due(before).expect("readable"), []);
}

#[tokio::test]
async fn an_expression_is_read_in_its_zone() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");

    let answer =
        read(&(scheduler.add(&adding(), daily("Asia/Kolkata", false)).await).expect("valid"));

    let next = next_of(&answer).to_string();
    assert!(
        next.ends_with("T03:30:00Z"),
        "09:00 in Kolkata is 03:30 UTC: {next}"
    );
    assert_eq!(answer["timezone"], "Asia/Kolkata");
}

#[tokio::test]
async fn a_one_off_retires_after_it_fires() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let next = next_of(&read(
        &(scheduler.add(&adding(), daily("UTC", true)).await).expect("valid"),
    ));

    let due = scheduler.due(after(next, Duration::ZERO)).expect("written");
    let listed = read(&scheduler.list().await.expect("a list"));

    assert_eq!(due.len(), 1);
    assert_eq!(listed["schedules"], Value::Array(Vec::new()), "retired");
}

#[tokio::test]
async fn a_retried_add_keeps_one_job() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");

    let first = (scheduler.add(&adding(), daily("UTC", false)).await).expect("valid");
    let retried = (scheduler.add(&adding(), daily("UTC", false)).await).expect("valid");
    let listed = read(&scheduler.list().await.expect("a list"));

    assert_eq!(first, retried, "the retry answers the job the call added");
    assert_eq!(listed["schedules"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_paused_job_is_never_due_and_run_now_fires_once() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let added = read(&(scheduler.add(&adding(), daily("UTC", false)).await).expect("valid"));
    let id = added["schedule_id"].as_str().expect("an id");
    let paused = Change {
        paused: Some(true),
        ..Change::default()
    };

    (scheduler.update(&adding(), id, paused).await).expect("a known job");
    let while_paused = scheduler.due(after(next_of(&added), Duration::from_hours(48)));
    let resumed = Change {
        paused: Some(false),
        ..Change::default()
    };
    (scheduler.update(&adding(), id, resumed).await).expect("a known job");
    (scheduler.run_now(&adding(), id).await).expect("a known job");
    let now = SystemTime::try_from(Timestamp::now()).expect("a system time");
    let fired = scheduler.due(now).expect("written");
    let runs = read(&(scheduler.runs(id, Page::default()).await).expect("a known job"));

    assert_eq!(while_paused.expect("readable"), []);
    assert_eq!(fired.len(), 1, "fired ahead of its moment");
    assert_eq!(scheduler.due(now).expect("readable"), [], "once");
    assert_eq!(runs["runs"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_bad_expression_or_zone_is_refused_and_nothing_is_kept() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let cases = [
        Job {
            cron: "61 * * * *",
            ..daily("UTC", false)
        },
        Job {
            cron: "",
            ..daily("UTC", false)
        },
        Job {
            cron: "0 9 * *",
            ..daily("UTC", false)
        },
        daily("Mars/Olympus", false),
        daily("", false),
    ];

    for job in cases {
        let refused = (scheduler.add(&adding(), job).await).expect_err("refused");
        assert_eq!(refused.code, code::INVALID_ARGUMENTS, "{job:?}");
    }
    assert!(
        !directory.path().join("schedules.json").exists(),
        "a refused add writes nothing"
    );
}

#[tokio::test]
async fn a_call_naming_no_job_is_refused() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let call = adding();

    let refusals = [
        scheduler.update(&call, "job-9", Change::default()).await,
        scheduler.remove(&call, "job-9").await,
        scheduler.run_now(&call, "job-9").await,
        scheduler.runs("job-9", Page::default()).await,
    ];

    for refused in refusals {
        let refused = refused.expect_err("no job has the id");
        assert_eq!(refused.code, code::SCHEDULE_NOT_FOUND);
        assert_eq!(refused.detail, "no schedule has the id job-9");
    }
}

#[tokio::test]
async fn a_runs_cursor_that_is_no_moment_is_refused() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let added = read(&(scheduler.add(&adding(), daily("UTC", false)).await).expect("valid"));
    let id = added["schedule_id"].as_str().expect("an id");
    let page = Page {
        limit: None,
        starting_after: Some("yesterday"),
    };

    let refused = (scheduler.runs(id, page).await).expect_err("not a fired moment");

    assert_eq!(refused.code, code::INVALID_ARGUMENTS);
}

#[test]
fn a_file_that_holds_no_jobs_does_not_open() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(directory.path().join("schedules.json"), "not jobs").expect("a file");

    let refused = LocalScheduler::open(directory.path()).expect_err("unreadable jobs");

    assert_eq!(refused.kind(), std::io::ErrorKind::Other);
}
