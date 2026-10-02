#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use super::Client;
use crate::api::{Ending, Executor as _, ProcessEvent, Spawn};

/// One mebibyte.
const MIB: usize = 1024 * 1024;

/// How long a call waits here: short, so a silent executor fails fast.
const SHORT: Duration = Duration::from_millis(200);
/// How long a test waits for anything that should be prompt.
const PATIENCE: Duration = Duration::from_secs(10);

/// A client whose calls wait [`SHORT`], and the executor's end of its socket.
fn silent() -> (Client, UnixStream) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    (Client::over(ours, SHORT), theirs)
}

/// Answers the next call on `executor` as a spawn of process `process`.
async fn answer_spawn(executor: &mut BufReader<UnixStream>, process: u64) {
    let mut request = String::new();
    executor.read_line(&mut request).await.unwrap();
    let id = serde_json::from_str::<Value>(&request).unwrap()["id"].clone();
    let answer = format!(r#"{{"jsonrpc":"2.0","result":{{"process_id":{process}}},"id":{id}}}"#);
    executor
        .get_mut()
        .write_all(format!("{answer}\n").as_bytes())
        .await
        .unwrap();
}

#[tokio::test]
async fn a_call_the_executor_never_answers_times_out_and_gives_up_the_link() {
    let (client, executor) = silent();
    let mut executor = BufReader::new(executor);
    let spawning = client.spawn(Spawn::program("sleep"));
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
            omitted_bytes: 0
        })
    );
    assert_eq!(process.events.recv().await, None, "ended exactly once");
    assert!(after.to_string().contains("connection closed"), "{after}");
}

#[tokio::test]
async fn a_send_into_an_executor_that_stopped_reading_is_given_up_too() {
    let (client, executor) = silent();
    let mut executor = BufReader::new(executor);
    let spawning = client.spawn(Spawn::program("sleep"));
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
            omitted_bytes: 0
        })
    );
    drop(executor);
}
