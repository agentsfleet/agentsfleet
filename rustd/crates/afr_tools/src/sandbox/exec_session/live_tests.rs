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
use crate::runtime::ToolOutput;
use crate::testing::call_in;

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
    let mut lease = Lease::default();

    let opened = call_in(
        exec,
        &client,
        &mut lease,
        json!({"cmd": "cat", "yield_time_ms": 250}),
    )
    .await;
    let id = session_id(&opened);
    let mut echoes = Vec::new();
    for chars in ["one\n", "two\n"] {
        let arguments = json!({"session_id": id, "chars": chars, "yield_time_ms": ECHO_YIELD_MS});
        echoes.push(call_in(write, &client, &mut lease, arguments).await.text);
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
