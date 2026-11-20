//! The built-in tools that need little or no backend: asking the person, the
//! plan, the clock, memory, a line to the conversation, a proposal and a
//! one-off schedule, each through `Tools::builder()` and `dispatch`, and a
//! handler of the embedder's own made a tool with `Typed`.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use guffbolt::agents::{Agents, Caps, SPAWN_AGENT, TOOLS, WAIT_AGENT};
use guffbolt::tools::memory::{
    Forgot, Memory, MemoryFuture, MemoryRefused, MemoryStore, Reach, Remembered,
};
use guffbolt::tools::propose::{Document, Kept, Propose, Refusal};
use guffbolt::tools::{
    Backend, Context, ContextTool, Handler, LocalChannel, LocalMemory, LocalScheduler, Schema,
    ToolsConfig, Typed, Wake, code,
};
use guffbolt::{
    CallRef, Describe, DriveState, Ending, ErrorCode, Plain, ToolOutput, ToolSpec, Tools,
};
use serde_json::{Value, json};

use crate::family::{Family, ROOT, agent, at, user};
use crate::script::{Play, Script, call, lock, results};

const AT: CallRef = CallRef {
    step: 1,
    index: 0,
    call_id: String::new(),
};

/// Runs `name` once over `tools`, lent `wake`.
async fn run(
    tools: &Tools<ContextTool<'_>>,
    name: &str,
    arguments: Value,
    wake: Option<&Wake>,
) -> ToolOutput {
    let context = Context {
        call_ref: &AT,
        run_id: "run-1",
        wake,
    };
    tools.dispatch(name, &arguments, context).await
}

fn built<'r>(builder: guffbolt::tools::ToolsBuilder<'r>, names: &[&str]) -> Tools<ContextTool<'r>> {
    builder
        .build(&ToolsConfig::new(names).expect("names"))
        .expect("built")
}

/// `ask_user` ends the root's run waiting on its question, its options
/// numbered; a child that asks is told it cannot, and its turn fails.
#[tokio::test]
async fn ask_user_waits_at_the_root_and_fails_a_childs_turn() {
    let (agents, requests) = Agents::new(Caps {
        depth: 2,
        open: 1,
        started: 1,
    });
    let mut names: Vec<&str> = TOOLS.to_vec();
    names.push("ask_user");
    let tools = built(Tools::builder().with_agents(&agents), &names);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "pick", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                call(
                    "ask_user",
                    json!({"question": "Which region?", "options": ["eu", "us"]}),
                ),
                Play::Say("eu it is"),
            ],
        )
        .plays(agent(1), [call("ask_user", json!({"question": "Which?"}))]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let mut state = DriveState::default();

    let (ending, transcript) = family
        .run(requests, vec![user("deploy")], &mut Plain, &mut state)
        .await;

    let waiting = match ending {
        Ending::Waiting(waiting) => Some(waiting),
        Ending::Answered(_) | Ending::Failed(_) | Ending::Stopped => None,
    }
    .expect("the root waits on its question");
    assert_eq!(waiting.question, "Which region?\n\n1. eu\n2. us");
    assert_eq!(
        results(&transcript).get(1).map(String::as_str),
        Some(
            r#"{"status":"failed","detail":"asked a question only the person can answer: Which?"}"#
        )
    );
    assert_eq!(state.waiting(), Some(&waiting));
}

#[tokio::test]
async fn update_plan_reads_back_the_plan_and_refuses_an_unknown_status() {
    let tools = built(Tools::builder(), &["update_plan"]);

    let recorded = run(
        &tools,
        "update_plan",
        json!({"explanation": "found the cause", "plan": [
            {"step": "read logs", "status": "completed"},
            {"step": "fix the parser", "status": "in_progress"},
            {"step": "ship", "status": "pending"}
        ]}),
        None,
    )
    .await;
    let bare = run(&tools, "update_plan", json!({"plan": []}), None).await;
    let refused = run(
        &tools,
        "update_plan",
        json!({"plan": [{"step": "x", "status": "blocked"}]}),
        None,
    )
    .await;
    let missing = run(&tools, "update_plan", json!({}), None).await;

    assert_eq!(
        recorded.text,
        "found the cause\n[completed] read logs\n[in_progress] fix the parser\n[pending] ship"
    );
    assert_eq!(bare.text, "");
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
    assert!(
        refused
            .text
            .starts_with("[invalid_arguments] unknown variant `blocked`"),
        "{}",
        refused.text
    );
    assert_eq!(missing.text, "[invalid_arguments] missing field `plan`");
}

#[tokio::test]
async fn current_time_reads_an_rfc_3339_instant_in_utc() {
    let tools = built(Tools::builder(), &["current_time"]);

    let read = run(&tools, "current_time", json!({}), None).await;
    let refused = run(&tools, "current_time", json!({"zone": "IST"}), None).await;

    let instant: jiff::Timestamp = read.text.parse().expect("RFC 3339");
    assert!(read.text.ends_with('Z'), "{}", read.text);
    assert!(instant > "2026-01-01T00:00:00Z".parse().expect("an instant"));
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
}

/// `sleep` waits as asked, never past ten minutes, and wakes at once when
/// input arrived first. The clock is tokio's paused one, so no test waits.
#[tokio::test(start_paused = true)]
async fn sleep_waits_as_asked_capped_at_ten_minutes_and_wakes_for_input() {
    let tools = built(Tools::builder(), &["sleep"]);
    let wake = Wake::default();

    let slept = run(&tools, "sleep", json!({"duration_ms": 1_500}), None).await;
    let capped = run(
        &tools,
        "sleep",
        json!({"duration_ms": 86_400_000}),
        Some(&wake),
    )
    .await;
    wake.input_arrived();
    let woken_early = run(&tools, "sleep", json!({"duration_ms": 60_000}), Some(&wake)).await;
    let refused = run(&tools, "sleep", json!({"duration_ms": -1}), None).await;

    assert_eq!(slept.text, "Slept 1500 ms");
    assert_eq!(capped.text, "Slept 600000 ms");
    assert_eq!(woken_early.text, "Woke after 0 ms: a new message arrived");
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
}

#[tokio::test]
async fn memory_lists_recalls_and_forgets_newest_first() {
    let memory = LocalMemory::seeded(vec![
        Memory {
            key: "deploy".to_owned(),
            content: "Fridays".to_owned(),
            category: "core".to_owned(),
            reach: Reach::Private,
        },
        Memory {
            key: "oncall".to_owned(),
            content: "deploy pager".to_owned(),
            category: "daily".to_owned(),
            reach: Reach::Shared,
        },
    ]);
    let tools = built(Tools::builder().with_memory(&memory), &["memory_*"]);

    let listed = run(&tools, "memory_list", json!({}), None).await;
    let daily = run(&tools, "memory_list", json!({"category": "daily"}), None).await;
    let none = run(&tools, "memory_list", json!({"category": "weekly"}), None).await;
    let recalled = run(
        &tools,
        "memory_recall",
        json!({"query": "deploy", "limit": 1}),
        None,
    )
    .await;
    let both = run(&tools, "memory_recall", json!({"query": "DEPLOY"}), None).await;
    let nothing = run(&tools, "memory_recall", json!({"query": "\u{1F680}"}), None).await;
    let forgot = run(&tools, "memory_forget", json!({"key": "oncall"}), None).await;
    let unknown = run(&tools, "memory_forget", json!({"key": "oncall"}), None).await;
    let stored = run(
        &tools,
        "memory_store",
        json!({"key": "k", "content": "c", "category": "daily", "visibility": "shared"}),
        None,
    )
    .await;

    assert_eq!(listed.text, "oncall (daily)\ndeploy (core)");
    assert_eq!(daily.text, "oncall (daily)");
    assert_eq!(none.text, "nothing remembered");
    assert_eq!(
        recalled.text, "deploy (core): Fridays",
        "key matches first, at most the limit"
    );
    assert_eq!(
        both.text,
        "deploy (core): Fridays\noncall (daily): deploy pager"
    );
    assert_eq!(nothing.text, "nothing remembered matches \u{1F680}");
    assert!(
        forgot
            .text
            .starts_with("oncall is forgotten for the rest of this run"),
        "{}",
        forgot.text
    );
    assert_eq!(unknown.text, "nothing remembered under oncall");
    assert_eq!(stored.text, "stored k");
    assert_eq!(
        memory
            .entries()
            .iter()
            .map(|entry| (entry.key.as_str(), entry.reach))
            .collect::<Vec<_>>(),
        [("deploy", Reach::Private), ("k", Reach::Shared)]
    );
}

/// A store that refuses every call, and names a writer on what it recalls.
#[derive(Debug)]
struct Full;

fn full() -> MemoryRefused {
    MemoryRefused {
        code: code::MEMORY_FULL,
        detail: "the store is full".to_owned(),
    }
}

impl MemoryStore for Full {
    fn store(&self, _memory: Memory) -> MemoryFuture<'_, ()> {
        Box::pin(std::future::ready(Err(full())))
    }

    fn recall<'a>(&'a self, query: &'a str, _limit: usize) -> MemoryFuture<'a, Vec<Remembered>> {
        let shared = Remembered {
            key: "team".to_owned(),
            content: query.to_owned(),
            category: "core".to_owned(),
            writer: Some("Support triage".to_owned()),
        };
        Box::pin(std::future::ready(if query.is_empty() {
            Err(full())
        } else {
            Ok(vec![shared])
        }))
    }

    fn list<'a>(&'a self, _category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>> {
        Box::pin(std::future::ready(Err(full())))
    }

    fn forget<'a>(&'a self, _key: &'a str) -> MemoryFuture<'a, Forgot> {
        Box::pin(std::future::ready(Err(full())))
    }
}

#[tokio::test]
async fn a_store_that_refuses_reaches_the_model_as_its_code_and_a_writer_is_named() {
    let tools = built(Tools::builder().with_memory(&Full), &["memory_*"]);

    let refused = [
        run(
            &tools,
            "memory_store",
            json!({"key": "k", "content": "c"}),
            None,
        )
        .await,
        run(&tools, "memory_recall", json!({"query": ""}), None).await,
        run(&tools, "memory_list", json!({}), None).await,
        run(&tools, "memory_forget", json!({"key": "k"}), None).await,
    ];
    let shared = run(&tools, "memory_recall", json!({"query": "escalate"}), None).await;

    for output in &refused {
        assert_eq!(output.text, "[memory_full] the store is full");
        assert_eq!(output.error_code, Some(code::MEMORY_FULL));
    }
    assert_eq!(
        shared.text,
        "team (core, shared by Support triage): escalate"
    );
}

#[tokio::test]
async fn message_hands_the_line_to_the_channel() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&heard);
    let channel = LocalChannel::new(move |line| lock(&kept).push(line.to_owned()));
    let tools = built(Tools::builder().with_channel(&channel), &["message"]);

    let posted = run(
        &tools,
        "message",
        json!({"text": "halfway \u{2014} 3 of 6"}),
        None,
    )
    .await;
    let refused = run(&tools, "message", json!({"line": "x"}), None).await;

    assert_eq!(posted.text, "posted to the conversation");
    assert_eq!(*lock(&heard), ["halfway \u{2014} 3 of 6"]);
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
    assert_eq!(format!("{channel:?}"), "LocalChannel { .. }");
}

/// A document whose proposals must each raise its number above the last.
#[derive(Debug)]
struct Counter;

impl Document for Counter {
    type Accepted = u64;
    type Revision = &'static str;

    fn schema(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: "propose_count",
            description: "Propose a higher count.",
            parameters: &Value::Null,
        }
    }

    fn base(&self) -> &'static str {
        "rev-1"
    }

    fn apply(&self, head: Option<&u64>, proposal: &Value) -> Result<u64, Refusal> {
        let count = proposal["count"]
            .as_u64()
            .ok_or_else(|| Refusal::new(ErrorCode::new("count_missing"), "name a count"))?;
        if head.is_some_and(|head| count <= *head) {
            return Err(Refusal::new(
                ErrorCode::new("count_not_higher"),
                "the count must rise",
            ));
        }
        Ok(count)
    }
}

#[tokio::test]
async fn propose_keeps_the_last_accepted_proposal_and_refuses_the_rest() {
    let propose = Propose::new(Counter);
    let tools = built(
        Tools::builder().with_tool(propose.clone()),
        &["propose_count"],
    );
    let before = propose.kept();

    let first = run(&tools, "propose_count", json!({"count": 2}), None).await;
    let lower = run(&tools, "propose_count", json!({"count": 1}), None).await;
    let missing = run(&tools, "propose_count", json!({}), None).await;
    let not_object = run(&tools, "propose_count", json!([3]), None).await;
    let refusal = Refusal::new(ErrorCode::new("count_missing"), "name a count");

    assert_eq!(before, None);
    assert!(first.text.starts_with("Accepted."), "{}", first.text);
    assert_eq!(lower.text, "[count_not_higher] the count must rise");
    assert_eq!(missing.text, "[count_missing] name a count");
    assert_eq!(not_object.error_code, Some(code::INVALID_ARGUMENTS));
    assert_eq!(
        propose.kept(),
        Some(Kept {
            base: "rev-1",
            document: 2
        })
    );
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (ErrorCode::new("count_missing"), "name a count")
    );
    assert_eq!(
        format!("{propose:?}"),
        "Propose { document: Counter, kept: true }"
    );
}

/// A handler of the embedder's own: its arguments' type is the schema the
/// model is told, and a call that breaks it never reaches the handler.
#[derive(Debug)]
struct Greet;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Name {
    /// Who to greet.
    name: String,
}

impl Handler for Greet {
    const NAME: &'static str = "greet";
    const DESCRIPTION: &'static str = "Greets someone.";
    type Arguments = Name;

    fn run(
        &self,
        arguments: Name,
        context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let greeting = format!("hello {} from {}", arguments.name, context.run_id);
        std::future::ready(ToolOutput::succeeded(greeting))
    }
}

#[tokio::test]
async fn a_typed_handler_is_told_its_arguments_schema_and_refuses_what_breaks_it() {
    let greet = Typed::new(Greet);
    let spec_name = greet.spec().name.to_owned();
    let schema = Schema::of::<Name>(Greet::DESCRIPTION);
    let tools = built(Tools::builder().with_tool(Typed::new(Greet)), &["greet"]);

    let greeted = run(&tools, "greet", json!({"name": "Ana\u{00EF}s"}), None).await;
    let refused = run(&tools, "greet", json!({"name": 7}), None).await;

    assert_eq!(spec_name, "greet");
    assert_eq!(format!("{:?}", greet.handler()), "Greet");
    assert_eq!(schema.description(), "Greets someone.");
    assert_eq!(at(schema.parameters(), "/properties/name/type"), "string");
    assert_eq!(at(schema.parameters(), "/required"), &json!(["name"]));
    assert_eq!(greeted.text, "hello Ana\u{00EF}s from run-1");
    assert!(
        refused
            .text
            .starts_with("[invalid_arguments] invalid type: integer `7`"),
        "{}",
        refused.text
    );
}

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
