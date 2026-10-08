#![expect(
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afr_egress::fixture::REPOSITORY;
use afr_executor::Spawn;
use serde_json::{Value, json};

use super::{EVENT_REFUSED, GIT_REFUSED_SUBCOMMANDS, Git, subcommand};
use crate::handler::Typed;
use crate::lease::Lease;
use crate::runtime::{Tool, ToolErrorCode};
use crate::sandbox::{Checkout, ScriptedExecutor, ScriptedProcess};
use crate::testing::{Live, call, call_in};

/// The directory the fixture repository is checked out in.
const WIDGETS: &str = "widgets";

fn git() -> Box<dyn Tool> {
    Typed::boxed(Git)
}

/// `git`'s arguments for `args`.
fn running(args: &[&str]) -> Value {
    json!({"args": args})
}

/// The one spawn `executor` was asked for.
fn only_spawn(executor: &ScriptedExecutor) -> Spawn {
    match executor.spawned().as_slice() {
        [spawn] => spawn.clone(),
        other => panic!("one spawn, got {other:?}"),
    }
}

/// A lease whose workspace holds `checkouts`.
fn holding(checkouts: Vec<Checkout<'static>>) -> Lease<'static> {
    Lease::default().with_checkouts(checkouts)
}

/// The fixture repository, checked out under its name.
fn widgets() -> Checkout<'static> {
    Checkout {
        repository: REPOSITORY,
        owner: "acme",
        name: WIDGETS,
        base: "dev",
    }
}

#[tokio::test]
async fn test_git_tool_runs_in_the_one_checked_out_repository() {
    let executor = ScriptedExecutor::new([ScriptedProcess::exits("On branch dev\n", 0)]);
    let lease = holding(vec![widgets()]);

    let output = call_in(&*git(), &executor, &lease, running(&["status"])).await;

    assert_eq!(output.text, "On branch dev\n");
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
    let spawn = only_spawn(&executor);
    assert_eq!(spawn.argv(), ["git", "status"]);
    assert_eq!(spawn.working_directory(), Some(WIDGETS));
    assert_eq!(
        spawn
            .environment()
            .get("GIT_COMMITTER_NAME")
            .map(String::as_str),
        Some("agentsfleet")
    );
}

#[tokio::test]
async fn should_run_in_the_workspace_root_with_no_or_several_checkouts() {
    let executor =
        ScriptedExecutor::new([ScriptedProcess::exits("", 0), ScriptedProcess::exits("", 0)]);
    let other = Checkout {
        name: "gadgets",
        ..widgets()
    };

    call_in(&*git(), &executor, &holding(Vec::new()), running(&["log"])).await;
    call_in(
        &*git(),
        &executor,
        &holding(vec![widgets(), other]),
        running(&["-C", WIDGETS, "log"]),
    )
    .await;

    let directories: Vec<_> = executor
        .spawned()
        .iter()
        .map(|spawn| spawn.working_directory().map(str::to_owned))
        .collect();
    assert_eq!(directories, [None, None]);
}

#[tokio::test]
async fn test_git_tool_refuses_network_subcommands() {
    let capture = Capture::install();
    let executor = ScriptedExecutor::new([]);
    let lease = holding(vec![widgets()]);

    for refused in GIT_REFUSED_SUBCOMMANDS {
        let output = call_in(&*git(), &executor, &lease, running(&[refused, "origin"])).await;

        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::SubcommandNotAllowed),
            "{refused}"
        );
        assert!(
            output.text.starts_with(&format!(
                "[subcommand_not_allowed] git {refused} reaches a remote"
            )) && output.text.ends_with("propose_change"),
            "{}",
            output.text
        );
    }
    assert!(executor.spawned().is_empty(), "nothing ran");
    let logged: Vec<String> = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(EVENT_REFUSED))
        .filter_map(|event| event.field("subcommand").map(str::to_owned))
        .collect();
    assert_eq!(logged, GIT_REFUSED_SUBCOMMANDS);
}

#[tokio::test]
async fn should_find_the_subcommand_after_global_options() {
    let executor = ScriptedExecutor::new([]);
    let lease = holding(vec![widgets()]);

    let output = call_in(
        &*git(),
        &executor,
        &lease,
        running(&["-c", "user.name=x", "--no-pager", "-C", WIDGETS, "push"]),
    )
    .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::SubcommandNotAllowed));
}

#[test]
fn should_name_no_subcommand_when_only_options_are_given() {
    let only_options: Vec<String> = ["--version", "-C", "push"].map(str::to_owned).into();
    let none: Vec<String> = Vec::new();

    assert_eq!(
        subcommand(&only_options),
        None,
        "-C takes push as its value"
    );
    assert_eq!(subcommand(&none), None);
}

/// Every global option git reads a separate value for is skipped with its
/// value, so the subcommand after it is the one judged.
#[test]
fn should_skip_each_valued_global_option_with_its_value() {
    for option in [
        "-C",
        "-c",
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--config-env",
        "--attr-source",
    ] {
        let args: Vec<String> = [option, "push", "fetch"].map(str::to_owned).into();
        assert_eq!(subcommand(&args), Some("fetch"), "{option}");
    }
}

#[tokio::test]
async fn should_refuse_without_a_sandbox_and_on_a_refused_spawn() {
    let lease = holding(vec![widgets()]);

    let unsandboxed = call(&*git(), &lease, running(&["status"])).await;
    let refused = call_in(
        &*git(),
        &ScriptedExecutor::new([]),
        &lease,
        running(&["status"]),
    )
    .await;

    assert_eq!(
        unsandboxed.error_code,
        Some(ToolErrorCode::SandboxUnavailable)
    );
    assert!(
        refused.text.ends_with(": no scripted process left"),
        "{}",
        refused.text
    );
}

#[tokio::test]
async fn should_refuse_arguments_that_are_not_a_list_of_strings() {
    let executor = ScriptedExecutor::new([]);

    let output = call_in(
        &*git(),
        &executor,
        &Lease::default(),
        json!({"args": "status"}),
    )
    .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    let spawned = executor.spawned();
    assert!(spawned.is_empty(), "{spawned:?}");
}

/// git runs in the checkout's directory; one the model removed is its own
/// mistake and reads `file_not_found`, never the sandbox being gone.
#[tokio::test]
async fn should_read_file_not_found_when_the_checkout_is_gone() {
    let live = Live::start().await;
    let lease = holding(vec![widgets()]);

    let missing = call_in(&*git(), &live.client, &lease, running(&["status"])).await;

    assert_eq!(
        missing.error_code,
        Some(ToolErrorCode::FileNotFound),
        "{missing:?}"
    );
    live.stop().await;
}
