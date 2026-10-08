#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use super::Client;
use crate::api::{Ending, Executor as _, ProcessEvent, Spawn};
use crate::protocol::{METHOD_KILL, METHOD_LIST_DIR, NOTIFY_EXITED, UNKNOWN_PROCESS_CODE};

/// One mebibyte.
const MIB: usize = 1024 * 1024;
/// The program every spawn here names; the fake executor runs nothing.
const PROGRAM: &str = "sleep";
/// The process every spawn here is answered with.
const PROCESS: u64 = 7;
/// The member an answer names its call by.
const ID: &str = "id";

/// How long a call waits here: short, so a silent executor fails fast.
const SHORT: Duration = Duration::from_millis(200);
/// How long a test waits for anything that should be prompt.
const PATIENCE: Duration = Duration::from_secs(10);

/// A client whose calls wait [`SHORT`], and the executor's end of its socket.
fn silent() -> (Client, UnixStream) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    (Client::over(ours, SHORT), theirs)
}

/// The next call the client sent `executor`.
async fn next_call(executor: &mut BufReader<UnixStream>) -> Value {
    let mut request = String::new();
    executor.read_line(&mut request).await.unwrap();
    serde_json::from_str(&request).unwrap()
}

/// Sends `message` from `executor`, as the one JSON-RPC line it is.
async fn say(executor: &mut BufReader<UnixStream>, mut message: Value) {
    message["jsonrpc"] = Value::from("2.0");
    executor
        .get_mut()
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
}

/// Answers the next call on `executor` with `result`, and hands the call back.
async fn answer(executor: &mut BufReader<UnixStream>, result: Value) -> Value {
    let call = next_call(executor).await;
    say(executor, json!({ "result": result, "id": call[ID] })).await;
    call
}

/// Answers the next call on `executor` as a spawn of process `process`.
async fn answer_spawn(executor: &mut BufReader<UnixStream>, process: u64) {
    answer(executor, json!({ "process_id": process })).await;
}

/// What the fake executor answers a listing with.
fn nothing_listed() -> Value {
    json!({ "entries": [], "truncated": false })
}

/// A client whose calls wait [`PATIENCE`], and a process it started.
async fn started() -> (Client, BufReader<UnixStream>, crate::api::Process) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let client = Client::over(ours, PATIENCE);
    let mut executor = BufReader::new(theirs);
    let spawning = Spawn::program(PROGRAM);
    let (spawned, ()) = tokio::join!(
        client.spawn(&spawning),
        answer_spawn(&mut executor, PROCESS)
    );
    (client, executor, spawned.unwrap())
}

#[tokio::test]
async fn a_call_the_executor_never_answers_times_out_and_gives_up_the_link() {
    let (client, executor) = silent();
    let mut executor = BufReader::new(executor);
    let sleep = Spawn::program(PROGRAM);
    let spawning = client.spawn(&sleep);
    let (spawned, ()) = tokio::join!(spawning, answer_spawn(&mut executor, 1));
    let mut process = spawned.unwrap();

    // The executor stops answering, as a stopped process would.
    let unanswered = tokio::time::timeout(PATIENCE, client.read_file("a", 1))
        .await
        .unwrap();
    let ended = tokio::time::timeout(PATIENCE, process.events.recv())
        .await
        .unwrap();
    let after = client.list_dir(".").await.unwrap_err();

    assert!(
        unanswered
            .unwrap_err()
            .to_string()
            .contains("did not answer fs/read in time")
    );
    assert_eq!(
        ended,
        Some(ProcessEvent::Ended {
            ending: Ending::Interrupted,
            output_abandoned: false
        })
    );
    assert_eq!(process.events.recv().await, None, "ended exactly once");
    assert!(after.to_string().contains("connection closed"), "{after}");
}

#[tokio::test]
async fn a_send_into_an_executor_that_stopped_reading_is_given_up_too() {
    let (client, executor) = silent();
    let mut executor = BufReader::new(executor);
    let sleep = Spawn::program(PROGRAM);
    let spawning = client.spawn(&sleep);
    let (spawned, ()) = tokio::join!(spawning, answer_spawn(&mut executor, 1));
    let mut process = spawned.unwrap();

    // Far more than a socket buffers, so the link's send waits on a reader
    // that is never coming.
    let flood = Bytes::from(vec![b'x'; 8 * MIB]);
    let stuck = tokio::time::timeout(PATIENCE, client.write_file("big", flood))
        .await
        .unwrap();
    let ended = tokio::time::timeout(PATIENCE, process.events.recv())
        .await
        .unwrap();

    assert!(
        stuck
            .unwrap_err()
            .to_string()
            .contains("did not answer fs/write in time")
    );
    assert_eq!(
        ended,
        Some(ProcessEvent::Ended {
            ending: Ending::Interrupted,
            output_abandoned: false
        })
    );
    drop(executor);
}

/// A caller that leaves mid-command, as a cancelled tool call does, leaves
/// no command running: dropping the process kills it. The executor refuses a
/// kill of a process it no longer knows, as after a kill already sent, and
/// the client hears that refusal as no one's.
#[tokio::test]
async fn a_process_dropped_before_its_end_is_killed_and_a_refusal_of_the_kill_is_no_ones() {
    let (client, mut executor, process) = started().await;

    drop(process);
    let kill = tokio::time::timeout(PATIENCE, next_call(&mut executor))
        .await
        .unwrap();
    let refusal = json!({ "code": UNKNOWN_PROCESS_CODE, "message": "gone" });
    say(&mut executor, json!({ "error": refusal, "id": kill[ID] })).await;
    let (listed, listing) = tokio::join!(
        client.list_dir("."),
        answer(&mut executor, nothing_listed())
    );

    assert_eq!(kill["method"], METHOD_KILL);
    assert_eq!(kill["params"]["process_id"], PROCESS);
    assert_eq!(listing["method"], METHOD_LIST_DIR);
    assert!(listed.unwrap().entries.is_empty(), "the link serves on");
}

/// A process read to its end has nothing left to kill: the next line the
/// executor reads is the next call.
#[tokio::test]
async fn a_process_read_to_its_end_sends_no_kill() {
    let (client, mut executor, process) = started().await;
    let ending = json!({ "kind": "exited", "code": 0 });
    let exited = json!({ "process_id": PROCESS, "ending": ending });
    say(
        &mut executor,
        json!({ "method": NOTIFY_EXITED, "params": exited }),
    )
    .await;

    let ending = tokio::time::timeout(PATIENCE, process.ended(|_stream, _data| {}))
        .await
        .unwrap();
    let (listed, next) = tokio::join!(
        client.list_dir("."),
        answer(&mut executor, nothing_listed())
    );

    assert_eq!(ending, Some(Ending::Exited(0)));
    assert_eq!(next["method"], METHOD_LIST_DIR, "{next}");
    listed.unwrap();
}
