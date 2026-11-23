//! `schedule` and the cron tools over a `LocalScheduler`.

use super::*;

/// `schedule` adds a one-off job a minute or more out and within the year;
/// every other moment is refused before the scheduler hears of it.
#[tokio::test]
async fn schedule_adds_a_one_off_job_and_refuses_a_moment_it_cannot_keep() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let tools = built(
        Tools::builder().with_scheduler(&scheduler),
        &["schedule", "cron_list"],
    );
    let soon = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(2))
        .expect("in range")
        .to_string();

    let added = run(
        &tools,
        "schedule",
        json!({"at": soon, "message": "check again"}),
        None,
    )
    .await;
    let mut refused = Vec::new();
    for at in [
        "yesterday",
        "2001-01-01T00:00:00Z",
        "2999-01-01T00:00:00Z",
        "9999-12-30T22:00:00.5Z",
    ] {
        refused.push(
            run(&tools, "schedule", json!({"at": at, "message": "x"}), None)
                .await
                .text,
        );
    }
    let listed = run(&tools, "cron_list", json!({}), None).await;

    let added: Value = serde_json::from_str(&added.text).expect("JSON");
    assert_eq!(at(&added, "/once"), true);
    assert_eq!(at(&added, "/message"), "check again");
    assert_eq!(
        refused,
        [
            "[invalid_arguments] at must be an RFC 3339 instant such as 2026-10-06T09:00:00+05:30",
            "[invalid_arguments] at must be at least a minute in the future",
            "[invalid_arguments] at must fall within the next year",
            "[invalid_arguments] at must fall within the next year",
        ]
    );
    let listed: Value = serde_json::from_str(&listed.text).expect("JSON");
    assert_eq!(at(&listed, "/schedules").as_array().map(Vec::len), Some(1));
}

/// The local scheduler and the cron tools over it, opened in `directory`.
fn cron_tools(scheduler: &LocalScheduler) -> Tools<ContextTool<'_>> {
    built(Tools::builder().with_scheduler(scheduler), &["cron_*"])
}

/// A retried `cron_add` adds nothing twice; `cron_update` changes what it
/// names and refuses an expression that never fires; `cron_remove` removes
/// once and then names the id it cannot find.
#[tokio::test]
async fn the_cron_tools_add_once_update_and_remove_a_job() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let tools = cron_tools(&scheduler);
    let weekly = json!({"cron": "0 9 * * 1", "timezone": "Asia/Kolkata", "message": "standup"});

    let added = run(&tools, "cron_add", weekly, None).await;
    let again = run(
        &tools,
        "cron_add",
        json!({"cron": "0 9 * * 1", "message": "x"}),
        None,
    )
    .await;
    let paused = json!({"schedule_id": "job-1", "message": "retro", "paused": true});
    let updated = run(&tools, "cron_update", paused, None).await;
    let never = json!({"schedule_id": "job-1", "cron": "0 0 30 2 *"});
    let never = run(&tools, "cron_update", never, None).await;
    let removed = run(&tools, "cron_remove", json!({"schedule_id": "job-1"}), None).await;
    let gone = run(&tools, "cron_remove", json!({"schedule_id": "job-1"}), None).await;

    let added: Value = serde_json::from_str(&added.text).expect("JSON");
    assert_eq!(at(&added, "/schedule_id"), "job-1");
    assert_eq!(at(&added, "/timezone"), "Asia/Kolkata");
    assert_eq!(
        again.text,
        added.to_string(),
        "a retried call adds nothing twice"
    );
    let updated: Value = serde_json::from_str(&updated.text).expect("JSON");
    assert_eq!(at(&updated, "/message"), "retro");
    assert_eq!(at(&updated, "/paused"), true);
    assert_eq!(
        never.text,
        "[invalid_arguments] cron never matches a moment"
    );
    assert_eq!(removed.text, "done");
    assert_eq!(
        gone.text,
        "[schedule_not_found] no schedule has the id job-1"
    );
}

/// `cron_run` fires a job at the next `due`; `cron_runs` pages what fired
/// and refuses a cursor it never gave.
#[tokio::test]
async fn cron_run_fires_at_the_next_due_and_cron_runs_pages_what_fired() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    let tools = cron_tools(&scheduler);
    let daily = json!({"cron": "0 9 * * *", "message": "daily"});
    let _added = run(&tools, "cron_add", daily, None).await;

    let fired_now = run(&tools, "cron_run", json!({"schedule_id": "job-1"}), None).await;
    let unknown = run(&tools, "cron_run", json!({"schedule_id": "job-2"}), None).await;
    let due = scheduler
        .due(SystemTime::UNIX_EPOCH + crate::schedule::FAR)
        .expect("written");
    let page = json!({"schedule_id": "job-1", "limit": 1});
    let runs = run(&tools, "cron_runs", page, None).await;
    let cursor = json!({"schedule_id": "job-1", "starting_after": "soon"});
    let bad_cursor = run(&tools, "cron_runs", cursor, None).await;
    let missing = run(&tools, "cron_runs", json!({"schedule_id": "job-9"}), None).await;

    assert_eq!(fired_now.error_code, None);
    assert_eq!(
        unknown.text,
        "[schedule_not_found] no schedule has the id job-2"
    );
    let fired: Vec<&str> = due.iter().map(|due| due.message.as_str()).collect();
    assert_eq!(fired, ["daily"]);
    let runs: Value = serde_json::from_str(&runs.text).expect("JSON");
    assert_eq!(at(&runs, "/runs").as_array().map(Vec::len), Some(1));
    assert!(
        at(&runs, "/next_cursor").is_string(),
        "a full page names where the next starts"
    );
    assert!(
        bad_cursor
            .text
            .starts_with("[invalid_arguments] starting_after must be a next_cursor"),
        "{}",
        bad_cursor.text
    );
    assert_eq!(
        missing.text,
        "[schedule_not_found] no schedule has the id job-9"
    );
}

/// The scheduler writes every change through a scratch file it then
/// renames; a scratch path it cannot write refuses the change and keeps
/// nothing.
#[tokio::test]
async fn a_change_the_scheduler_cannot_save_is_refused_and_not_kept() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("opens");
    std::fs::create_dir(directory.path().join("schedules.json.partial"))
        .expect("a directory in the way");
    let tools = built(
        Tools::builder().with_scheduler(&scheduler),
        &["cron_add", "cron_list"],
    );

    let refused = run(
        &tools,
        "cron_add",
        json!({"cron": "0 9 * * *", "message": "x"}),
        None,
    )
    .await;
    let listed = run(&tools, "cron_list", json!({}), None).await;

    assert_eq!(refused.error_code, Some(code::SCHEDULE_UNSAVED));
    assert_eq!(listed.text, r#"{"schedules":[]}"#);
}

#[test]
fn a_scheduler_whose_file_is_unreadable_refuses_to_open() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    std::fs::create_dir(directory.path().join("schedules.json"))
        .expect("a directory where the file goes");

    let refused = LocalScheduler::open(directory.path()).expect_err("a directory is no file");

    assert_ne!(refused.kind(), std::io::ErrorKind::NotFound);
}

/// A kept job whose expression no longer parses, as a hand-edited file
/// leaves it, stops `due` before anything is handed over.
#[test]
fn due_refuses_a_kept_job_whose_expression_no_longer_reads() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let book = json!({"next_id": 1, "jobs": [{
        "id": "job-1", "cron": "not a cron", "timezone": "UTC", "message": "x", "once": false,
        "paused": false, "next": "2026-01-01T09:00:00Z", "fire_now": false,
        "added_by": {"step": 1, "index": 0, "call_id": "c"}, "fired": []
    }]});
    std::fs::write(directory.path().join("schedules.json"), book.to_string()).expect("written");
    let scheduler = LocalScheduler::open(directory.path()).expect("the file reads");

    let refused = scheduler
        .due(SystemTime::UNIX_EPOCH + crate::schedule::FAR)
        .expect_err("unreadable");

    assert_eq!(refused.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        refused.to_string().starts_with("cron must be five fields"),
        "{refused}"
    );
}

#[test]
fn every_backend_has_the_name_a_refusal_reads() {
    let names: Vec<String> = [
        Backend::Env,
        Backend::Sessions,
        Backend::Outbound,
        Backend::Memory,
        Backend::Scheduler,
        Backend::Channel,
        Backend::Notifier,
        Backend::Agents,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    assert_eq!(
        names,
        [
            "env",
            "sessions",
            "outbound",
            "memory",
            "scheduler",
            "channel",
            "notifier",
            "agents"
        ]
    );
}
