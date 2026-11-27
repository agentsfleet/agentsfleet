#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use super::Client;
use crate::api::{GuestAgent as _, ProcessEvent, Spawn};
use af_guest_wire::Ending;
use af_guest_wire::protocol::{METHOD_KILL, METHOD_LIST_DIR, NOTIFY_EXITED, UNKNOWN_PROCESS_CODE};

/// One mebibyte.
const MIB: usize = 1024 * 1024;
/// The program every spawn here names; the fake guest agent runs nothing.
const PROGRAM: &str = "sleep";
/// The process every spawn here is answered with.
const PROCESS: u64 = 7;
/// The member an answer names its call by.
const ID: &str = "id";

/// How long a call waits here: short, so a silent guest agent fails fast.
const SHORT: Duration = Duration::from_millis(200);
/// How long a test waits for anything that should be prompt.
const PATIENCE: Duration = Duration::from_secs(10);

#[tokio::test]
async fn recovery_idle_preserves_the_agents_explicit_busy_and_idle_answers() {
    let (client, socket) = silent();
    let mut agent = BufReader::new(socket);
    let requesting = async {
        assert!(!client.recovery_idle().await.unwrap());
        assert!(client.recovery_idle().await.unwrap());
    };
    let replying = async {
        for idle in [false, true] {
            let call = answer(&mut agent, json!(idle)).await;
            assert_eq!(
                call["method"],
                af_guest_wire::protocol::METHOD_RECOVERY_IDLE
            );
            assert_eq!(call["params"], json!({"proof": "file_jobs"}));
        }
    };
    tokio::join!(requesting, replying);
}

/// A client whose calls wait [`SHORT`], and the guest agent's end of its socket.
fn silent() -> (Client, UnixStream) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    (
        Client::over(ours, SHORT, af_guest_wire::Sizes::DEFAULT),
        theirs,
    )
}

/// The next call the client sent `agent`.
pub(super) async fn next_call(agent: &mut BufReader<impl AsyncRead + Unpin>) -> Value {
    let mut request = String::new();
    agent.read_line(&mut request).await.unwrap();
    serde_json::from_str(&request).unwrap()
}

/// Sends `message` from `agent`, as the one JSON-RPC line it is.
async fn say(agent: &mut BufReader<impl AsyncRead + AsyncWrite + Unpin>, mut message: Value) {
    message["jsonrpc"] = Value::from("2.0");
    agent
        .get_mut()
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
}

/// Answers the next call on `agent` with `result`, and hands the call back.
pub(super) async fn answer(
    agent: &mut BufReader<impl AsyncRead + AsyncWrite + Unpin>,
    result: Value,
) -> Value {
    let call = next_call(agent).await;
    say(agent, json!({ "result": result, "id": call[ID] })).await;
    call
}

/// Answers the next call on `agent` as a spawn of process `process`.
pub(super) async fn answer_spawn(
    agent: &mut BufReader<impl AsyncRead + AsyncWrite + Unpin>,
    process: u64,
) {
    answer(agent, json!({ "process_id": process })).await;
}

/// What the fake guest agent answers a listing with.
pub(super) fn nothing_listed() -> Value {
    json!({ "entries": [], "truncated": false })
}

/// A client whose calls wait [`PATIENCE`], and a process it started.
pub(super) async fn started() -> (Client, BufReader<UnixStream>, crate::api::Process) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let client = Client::over(ours, PATIENCE, af_guest_wire::Sizes::DEFAULT);
    let mut agent = BufReader::new(theirs);
    let spawning = Spawn::program(PROGRAM);
    let (spawned, ()) = tokio::join!(client.spawn(&spawning), answer_spawn(&mut agent, PROCESS));
    (client, agent, spawned.unwrap())
}

#[tokio::test]
async fn a_call_the_agent_never_answers_times_out_and_gives_up_the_link() {
    let (client, agent) = silent();
    let mut agent = BufReader::new(agent);
    let sleep = Spawn::program(PROGRAM);
    let spawning = client.spawn(&sleep);
    let (spawned, ()) = tokio::join!(spawning, answer_spawn(&mut agent, 1));
    let mut process = spawned.unwrap();

    // The guest agent stops answering, as a stopped process would.
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
async fn a_send_into_an_agent_that_stopped_reading_is_given_up_too() {
    let (client, agent) = silent();
    let mut agent = BufReader::new(agent);
    let sleep = Spawn::program(PROGRAM);
    let spawning = client.spawn(&sleep);
    let (spawned, ()) = tokio::join!(spawning, answer_spawn(&mut agent, 1));
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
    drop(agent);
}

/// A caller that leaves mid-command, as a cancelled tool call does, leaves
/// no command running: dropping the process kills it. The guest agent refuses a
/// kill of a process it no longer knows, as after a kill already sent, and
/// the client hears that refusal as no one's.
#[tokio::test]
async fn a_process_dropped_before_its_end_is_killed_and_a_refusal_of_the_kill_is_no_ones() {
    let (client, mut agent, process) = started().await;

    drop(process);
    let kill = tokio::time::timeout(PATIENCE, next_call(&mut agent))
        .await
        .unwrap();
    let refusal = json!({ "code": UNKNOWN_PROCESS_CODE, "message": "gone" });
    say(&mut agent, json!({ "error": refusal, "id": kill[ID] })).await;
    let (listed, listing) =
        tokio::join!(client.list_dir("."), answer(&mut agent, nothing_listed()));

    assert_eq!(kill["method"], METHOD_KILL);
    assert_eq!(kill["params"]["process_id"], PROCESS);
    assert_eq!(listing["method"], METHOD_LIST_DIR);
    assert!(listed.unwrap().entries.is_empty(), "the link serves on");
}

/// A process read to its end has nothing left to kill: the next line the
/// guest agent reads is the next call.
#[tokio::test]
async fn a_process_read_to_its_end_sends_no_kill() {
    let (client, mut agent, process) = started().await;
    let ending = json!({ "kind": "exited", "code": 0 });
    let exited = json!({ "process_id": PROCESS, "ending": ending });
    say(
        &mut agent,
        json!({ "method": NOTIFY_EXITED, "params": exited }),
    )
    .await;

    let ending = tokio::time::timeout(PATIENCE, process.ended(|_stream, _data| {}))
        .await
        .unwrap();
    let (listed, next) = tokio::join!(client.list_dir("."), answer(&mut agent, nothing_listed()));

    assert_eq!(ending, Some(Ending::Exited(0)));
    assert_eq!(next["method"], METHOD_LIST_DIR, "{next}");
    listed.unwrap();
}
