#![expect(
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afr_executor::{Ending, Spawn};
use serde_json::json;

use super::Shell;
use crate::handler::Typed;
use crate::lease::Lease;
use crate::runtime::{Tool, ToolErrorCode};
use crate::sandbox::oneshot::{EVENT_TIMED_OUT, TIMEOUT_MS_DEFAULT, TIMEOUT_MS_MAX};
use crate::sandbox::{ScriptedExecutor, ScriptedProcess};
use crate::testing::{call, call_in};

fn shell() -> Box<dyn Tool> {
    Typed::boxed(Shell)
}

/// The one spawn `executor` was asked for.
fn only_spawn(executor: &ScriptedExecutor) -> Spawn {
    match executor.spawned().as_slice() {
        [spawn] => spawn.clone(),
        other => panic!("one spawn, got {other:?}"),
    }
}

#[tokio::test]
async fn test_shell_runs_through_the_executor_and_reports_its_exit_code() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("hi\n", 3)]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "echo hi; exit 3"}),
    )
    .await;

    assert_eq!(output.text, "hi\nProcess exited with code 3");
    assert_eq!(output.exit_code, Some(3));
    assert_eq!(
        output.error_code, None,
        "the exit code alone fails the call"
    );
    let spawn = only_spawn(&executor);
    assert_eq!(spawn.argv(), ["/bin/sh", "-c", "echo hi; exit 3"]);
    assert_eq!(
        spawn.time_limit(),
        Some(Duration::from_millis(TIMEOUT_MS_DEFAULT))
    );
    assert!(!spawn.on_terminal(), "a command runs on pipes");
    assert_eq!(spawn.working_directory(), None, "in the workspace root");
    let environment = spawn.environment();
    assert_eq!(environment.len(), 13, "{environment:?}");
    assert_eq!(
        environment.get("GIT_PAGER").map(String::as_str),
        Some("cat")
    );
    assert_eq!(environment.get("TERM").map(String::as_str), Some("dumb"));
    assert_eq!(
        environment.get("LC_ALL").map(String::as_str),
        Some("C.UTF-8")
    );
    assert_eq!(
        environment.get("GIT_AUTHOR_EMAIL").map(String::as_str),
        Some("noreply@agentsfleet.net"),
        "a commit made in the sandbox has an author with no HOME to read one from"
    );
}

#[tokio::test]
async fn should_answer_with_the_output_alone_when_the_command_succeeds() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("ok\n", 0)]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "true"}),
    )
    .await;

    assert_eq!(output.text, "ok\n");
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(output.error_code, None);
}

#[tokio::test]
async fn should_hold_the_timeout_the_model_names_under_the_ceiling() {
    let executor =
        ScriptedExecutor::new([ScriptedProcess::exits("", 0), ScriptedProcess::exits("", 0)]);
    let lease = Lease::default();

    for timeout_ms in [1_500, 86_400_000] {
        let arguments = json!({"command": "true", "timeout_ms": timeout_ms});
        call_in(&*shell(), &executor, &lease, arguments).await;
    }

    let limits: Vec<_> = executor.spawned().iter().map(Spawn::time_limit).collect();
    assert_eq!(
        limits,
        [
            Some(Duration::from_millis(1_500)),
            Some(Duration::from_millis(TIMEOUT_MS_MAX)),
        ]
    );
}

#[tokio::test]
async fn test_shell_timeout_reports_timed_out_and_logs_it() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([ScriptedProcess::ends("started\n", Ending::TimedOut)]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "sleep 60 & sleep 60", "timeout_ms": 500}),
    )
    .await;

    assert_eq!(output.text, "started\nProcess timed out after 500 ms");
    assert_eq!(output.error_code, Some(ToolErrorCode::TimedOut));
    assert_eq!(output.exit_code, None);
    let logged = capture.only(EVENT_TIMED_OUT);
    assert_eq!(logged.level, tracing::Level::WARN);
    assert_eq!(logged.field("timeout_ms"), Some("500"));
    assert_eq!(logged.field("error_code"), Some("timed_out"));
    assert!(logged.field("lease_id").is_some(), "{logged:?}");
}

#[tokio::test]
async fn should_report_a_signal_and_a_lost_ending() {
    let executor = ScriptedExecutor::new([
        ScriptedProcess::ends("", Ending::Signaled(9)),
        ScriptedProcess::vanishes("partial"),
    ]);
    let lease = Lease::default();

    let killed = call_in(
        &*shell(),
        &executor,
        &lease,
        json!({"command": "kill -9 $$"}),
    )
    .await;
    let lost = call_in(&*shell(), &executor, &lease, json!({"command": "sleep 1"})).await;

    assert_eq!(killed.text, "Process killed by signal 9");
    assert_eq!(killed.exit_code, Some(137));
    assert_eq!(killed.error_code, None);
    assert_eq!(
        lost.text,
        "partial\nProcess interrupted before its ending arrived"
    );
    assert_eq!(lost.error_code, Some(ToolErrorCode::Interrupted));
    assert_eq!(lost.exit_code, None);
}

#[tokio::test]
async fn should_cut_a_long_output_to_the_model_budget() {
    let long = "x".repeat(50_000);
    let executor = ScriptedExecutor::new([ScriptedProcess::exits(&long, 0)]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "yes x | head -c 50000"}),
    )
    .await;

    // pin test: literal is the contract
    let marker = "\n... 10000 bytes omitted ...\n";
    assert_eq!(output.text.len(), 40_000 + marker.len());
    assert!(output.text.contains(marker));
}

#[tokio::test]
async fn should_refuse_without_a_sandbox_or_when_the_executor_refuses() {
    let lease = Lease::default();

    let unsandboxed = call(&*shell(), &lease, json!({"command": "true"})).await;
    let refused = call_in(
        &*shell(),
        &ScriptedExecutor::new([]),
        &lease,
        json!({"command": "true"}),
    )
    .await;

    assert_eq!(
        unsandboxed.text,
        "[sandbox_unavailable] this call has no sandbox to run in"
    );
    assert_eq!(refused.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert!(
        refused.text.ends_with(": no scripted process left"),
        "the executor's own words, not this host's code: {}",
        refused.text
    );
    assert!(!refused.text.contains("UZ-"), "{}", refused.text);
}

#[tokio::test]
async fn should_refuse_an_argument_it_does_not_take_and_run_nothing() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("", 0)]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "true", "cwd": "/"}),
    )
    .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(executor.spawned().is_empty());
}

/// A command whose output was still open when it ended says so, after its
/// output, in place of the status a clean exit leaves out.
#[tokio::test]
async fn should_say_the_output_was_still_open_after_the_output() {
    let executor =
        ScriptedExecutor::new([ScriptedProcess::ends_abandoned("ok\n", Ending::Exited(0))]);

    let output = call_in(
        &*shell(),
        &executor,
        &Lease::default(),
        json!({"command": "true"}),
    )
    .await;

    assert_eq!(
        output.text,
        "ok\n... the process ended with its output still open; what was written after is not shown ..."
    );
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(output.error_code, None);
}
