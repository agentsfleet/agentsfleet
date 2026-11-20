//! `LocalScheduler` behind the schedule tools: a job a run adds through
//! `cron_add` is kept in its directory, and a scheduler opened over that
//! directory later hands it over once.

use std::io;
use std::time::{Duration, SystemTime};

use guffbolt::agents::{Agents, Caps};
use guffbolt::tools::schedule::Due;
use guffbolt::tools::{LocalScheduler, ToolsConfig};
use guffbolt::{DriveState, Plain, Tools};
use serde_json::{Value, json};

use crate::family::{Family, ROOT, answered, at, user};
use crate::script::{Play, Script, call, results};

const CAPS: Caps = Caps {
    depth: 1,
    open: 1,
    started: 1,
};

/// Jan 01, 2100, 00:00 UTC: later than any moment a daily job added today
/// next fires at, so every such job is due.
pub(crate) const FAR: Duration = Duration::from_hours(1_139_568);

/// Seconds in a day, and 09:00 within one.
const DAY: u64 = 86_400;
const NINE: u64 = 9 * 3_600;

/// Adds a daily job through a run over the scheduler opened at `directory`,
/// and answers what the run's tools read back.
async fn add_through_a_run(directory: &std::path::Path) -> Vec<String> {
    let scheduler = LocalScheduler::open(directory).expect("an empty directory opens");
    let config = ToolsConfig::new(["cron_*"]).expect("a pattern");
    let tools = Tools::builder()
        .with_scheduler(&scheduler)
        .build(&config)
        .expect("built");
    let (agents, requests) = Agents::new(CAPS);
    let script = Script::default().plays(
        ROOT,
        [
            call(
                "cron_add",
                json!({"cron": "0 9 * * *", "message": "daily check"}),
            ),
            Play::Call(vec![
                ("cron_add", json!({"cron": "every day", "message": "x"})),
                (
                    "cron_add",
                    json!({"cron": "0 9 * * *", "timezone": "Mars/Olympus", "message": "x"}),
                ),
                ("cron_remove", json!({"schedule_id": "job-9"})),
            ]),
            call("cron_list", json!({})),
            Play::Say("scheduled"),
        ],
    );
    let family = Family {
        agents,
        script,
        tools: &tools,
    };

    let (ending, transcript) = family
        .run(
            requests,
            vec![user("check daily")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(answered(&ending), Some("scheduled"));
    results(&transcript)
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("the scheduler answers JSON")
}

#[tokio::test]
async fn a_job_added_through_a_run_is_due_once_after_the_scheduler_reopens() {
    let directory = tempfile::tempdir().expect("a scratch directory");

    let read = add_through_a_run(directory.path()).await;
    let reopened = LocalScheduler::open(directory.path()).expect("the kept file opens");
    let far = SystemTime::UNIX_EPOCH + FAR;
    let due = reopened.due(far).expect("the file is written through");
    let again = reopened.due(far).expect("nothing more is due");
    let third = LocalScheduler::open(directory.path()).expect("the kept file opens");
    let after_the_third = third.due(far).expect("written through");

    let added = json(read.first().expect("cron_add answered"));
    assert_eq!(at(&added, "/schedule_id"), "job-1");
    assert_eq!(at(&added, "/timezone"), "UTC");
    assert_eq!(at(&added, "/paused"), false);
    assert!(
        read.get(1).is_some_and(|refusal| refusal.starts_with(
            "[invalid_arguments] cron must be five fields: minute, hour, day of month, month, day of week"
        )),
        "{read:?}"
    );
    assert_eq!(
        read.get(2).map(String::as_str),
        Some("[invalid_arguments] timezone is not an IANA zone: Mars/Olympus")
    );
    assert_eq!(
        read.get(3).map(String::as_str),
        Some("[schedule_not_found] no schedule has the id job-9")
    );
    let listed = json(read.get(4).expect("cron_list answered"));
    assert_eq!(
        at(&listed, "/schedules").as_array().map(Vec::len),
        Some(1),
        "only the valid job is kept"
    );

    assert_eq!(due.len(), 1, "one job is due: {due:?}");
    let Due {
        id,
        message,
        at: fired,
    } = due.first().expect("one job");
    assert_eq!((id.as_str(), message.as_str()), ("job-1", "daily check"));
    let seconds = fired
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs();
    assert_eq!(seconds % DAY, NINE, "it was due at 09:00 UTC");
    assert_eq!(
        again,
        Vec::new(),
        "a recurring job moved past `far`, so it fires once"
    );
    assert_eq!(after_the_third, Vec::new(), "the move was written through");
}

#[test]
fn a_scheduler_over_a_file_that_holds_no_jobs_refuses_to_open() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(directory.path().join("schedules.json"), b"{\"jobs\": 3}").expect("written");

    let refused = LocalScheduler::open(directory.path()).expect_err("not a job book");

    assert_eq!(refused.kind(), io::ErrorKind::Other);
    assert!(
        refused.to_string().starts_with("invalid type: integer `3`"),
        "{refused}"
    );
}

#[test]
fn a_scheduler_with_nothing_kept_has_nothing_due() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path().join("nested")).expect("made and opened");

    let due = scheduler
        .due(SystemTime::UNIX_EPOCH + FAR)
        .expect("nothing to write");

    assert_eq!(due, Vec::new());
    assert!(
        !directory.path().join("nested/schedules.json").exists(),
        "nothing was written"
    );
}
