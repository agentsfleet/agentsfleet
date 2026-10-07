//! A session against the real executor, served in-process on a scratch
//! socket, so the handlers cross the socket a sandbox's executor answers on.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test module: a harness that cannot start should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afr_egress::testing::RecordingTransport;
use afr_executor::Client;
use serde_json::json;

use crate::catalog::{Catalog, EXEC_COMMAND, WRITE_STDIN};
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{Live, call_in, hosted, offered};

/// How long the suite waits on the executor before failing.
const PATIENCE: Duration = Duration::from_secs(20);
/// How long each write waits for `cat` to echo: ample on a loaded machine.
const ECHO_YIELD_MS: u64 = 1_000;

/// The session id an answer names while its process runs.
fn session_id(output: &ToolOutput) -> u64 {
    output
        .text
        .rsplit(' ')
        .next()
        .and_then(|id| id.parse().ok())
        .expect("a running session's answer ends with its id")
}

#[tokio::test]
async fn test_exec_session_survives_across_calls() {
    let scratch = tempfile::tempdir().unwrap();
    let socket = scratch.path().join("executor.sock");
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let server = tokio::spawn({
        let (socket, root) = (socket.clone(), root.clone());
        async move { afr_executor::serve(&socket, &root).await }
    });
    let client = Client::connect_within(&socket, PATIENCE).await.unwrap();
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let catalog = Catalog::hosted(Arc::new(transport));
    let names = [EXEC_COMMAND.name(), WRITE_STDIN.name()];
    let selection = catalog.select(&names).unwrap();
    let (exec, write) = (
        selection.tool(EXEC_COMMAND.name()).unwrap(),
        selection.tool(WRITE_STDIN.name()).unwrap(),
    );
    let lease = Lease::default();

    let opened = call_in(
        exec,
        &client,
        &lease,
        json!({"cmd": "cat", "yield_time_ms": 250}),
    )
    .await;
    let id = session_id(&opened);
    let mut echoes = Vec::new();
    for chars in ["one\n", "two\n"] {
        let arguments = json!({"session_id": id, "chars": chars, "yield_time_ms": ECHO_YIELD_MS});
        echoes.push(call_in(write, &client, &lease, arguments).await.text);
    }

    let running = format!("Process running with session ID {id}");
    assert_eq!(opened.text, running);
    assert_eq!(
        echoes,
        [format!("one\n{running}"), format!("two\n{running}")]
    );
    assert_eq!(
        lease.sessions.close_all(&client).await,
        1,
        "the run's end kills cat"
    );
    drop(client);
    tokio::time::timeout(PATIENCE, server)
        .await
        .expect("the executor stops once its client hangs up")
        .unwrap()
        .unwrap();
}

/// A working directory the workspace does not have is the model's mistake,
/// and reads back as one: `file_not_found`, never the sandbox being gone. A
/// link out of the workspace reads as the path refused.
#[tokio::test]
async fn a_missing_or_escaping_workdir_reads_as_the_callers_mistake() {
    let live = Live::start().await;
    std::os::unix::fs::symlink(live.outside(), live.root.join("out")).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[EXEC_COMMAND.name()]).unwrap();
    let exec = offered(&selection, &EXEC_COMMAND);
    let lease = Lease::default();

    let missing = call_in(
        exec,
        &live.client,
        &lease,
        json!({"cmd": "true", "workdir": "gone"}),
    )
    .await;
    let escaping = call_in(
        exec,
        &live.client,
        &lease,
        json!({"cmd": "true", "workdir": "out"}),
    )
    .await;

    assert_eq!(
        missing.error_code,
        Some(ToolErrorCode::FileNotFound),
        "{missing:?}"
    );
    assert_eq!(
        escaping.error_code,
        Some(ToolErrorCode::PathNotAllowed),
        "{escaping:?}"
    );
    live.stop().await;
}

/// A session that has printed past the half mebibyte the executor once sent
/// live is still heard: what it says next reaches the next call, not only
/// its end.
#[tokio::test]
async fn test_a_session_is_heard_past_its_first_half_mebibyte() {
    let live = Live::start().await;
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[EXEC_COMMAND.name(), WRITE_STDIN.name()])
        .unwrap();
    let (exec, write) = (
        offered(&selection, &EXEC_COMMAND),
        offered(&selection, &WRITE_STDIN),
    );
    let lease = Lease::default();
    let cmd = "yes | head -c 700000; read word; echo said-$word; sleep 30";

    let opened = call_in(
        exec,
        &live.client,
        &lease,
        json!({"cmd": cmd, "yield_time_ms": ECHO_YIELD_MS}),
    )
    .await;
    let arguments = json!({
        "session_id": session_id(&opened),
        "chars": "go\n",
        "yield_time_ms": ECHO_YIELD_MS,
    });
    let answered = call_in(write, &live.client, &lease, arguments).await;

    assert!(answered.text.contains("said-go\n"), "{}", answered.text);
    assert_eq!(lease.sessions.close_all(&live.client).await, 1);
    live.stop().await;
}

/// A write the real executor refuses, because the process closed its input,
/// reads as the fixed sentence and the session running on: the executor's
/// words stay on the host.
#[tokio::test]
async fn test_a_write_the_process_will_not_take_says_so_and_runs_on() {
    let live = Live::start().await;
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[EXEC_COMMAND.name(), WRITE_STDIN.name()])
        .unwrap();
    let (exec, write) = (
        offered(&selection, &EXEC_COMMAND),
        offered(&selection, &WRITE_STDIN),
    );
    let lease = Lease::default();
    let opened = call_in(
        exec,
        &live.client,
        &lease,
        json!({"cmd": "exec 0<&-; sleep 30", "yield_time_ms": 250}),
    )
    .await;
    let id = session_id(&opened);

    // The first writes may land before the shell closes its input.
    let mut refused = None;
    for _ in 0..50 {
        let arguments = json!({"session_id": id, "chars": "x\n", "yield_time_ms": 250});
        let output = call_in(write, &live.client, &lease, arguments).await;
        if output.text.starts_with(super::INPUT_REFUSED) {
            refused = Some(output);
            break;
        }
    }

    let output = refused.expect("the closed input refuses a write");
    assert_eq!(output.error_code, None);
    assert_eq!(
        output.text,
        format!(
            "{}\nProcess running with session ID {id}",
            super::INPUT_REFUSED
        )
    );
    assert_eq!(lease.sessions.close_all(&live.client).await, 1);
    live.stop().await;
}
