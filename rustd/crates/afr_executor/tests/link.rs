//! The client against a hand-driven executor: every shape of answer, and
//! every message it must survive.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afr_executor::{Client, Ending, Executor as _, Spawn};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};

use crate::support::{finish, scratch};

/// The executor's side of the socket, driven line by line.
struct Fake {
    reader: tokio::io::Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl Fake {
    /// The next request the client sent.
    async fn request(&mut self) -> Value {
        serde_json::from_str(&self.reader.next_line().await.unwrap().unwrap()).unwrap()
    }

    /// Sends one raw line.
    async fn say(&mut self, line: &str) {
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }
}

/// A client connected to a fake executor.
async fn connect() -> (tempfile::TempDir, Client, Fake) {
    let (scratch, socket, _root) = scratch();
    let listener = UnixListener::bind(&socket).unwrap();
    let client = Client::connect(&socket).await.unwrap();
    let (stream, _peer): (UnixStream, _) = listener.accept().await.unwrap();
    let (read, writer) = stream.into_split();
    (
        scratch,
        client,
        Fake {
            reader: BufReader::new(read).lines(),
            writer,
        },
    )
}

/// Answers the next spawn with `process`.
async fn started(fake: &mut Fake, process: u64) {
    let id = fake.request().await["id"].clone();
    fake.say(&format!(
        r#"{{"jsonrpc":"2.0","result":{{"process_id":{process}}},"id":{id}}}"#
    ))
    .await;
}

#[tokio::test]
async fn answers_and_notifications_reach_their_callers_past_noise() {
    let (_scratch, client, mut fake) = connect().await;
    let spawning =
        tokio::spawn(async move { (client.spawn(Spawn::program("anything")).await, client) });
    started(&mut fake, 7).await;
    let (process, _client) = spawning.await.unwrap();
    let process = process.unwrap();

    for noise in [
        "not json",
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":9,"stream":"stdout","data":"aGk="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/progress","params":{}}"#,
        r#"{"jsonrpc":"2.0","result":null,"id":99}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stdout","data":"%%%"}}"#,
        r#"{"jsonrpc":"2.0","method":"process/exited","params":[7]}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stdout","data":"aGk="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stderr","data":"IQ=="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"exit_code":2,"signal":null,"timed_out":false,"omitted_bytes":5}}"#,
    ] {
        fake.say(noise).await;
    }
    let finished = finish(process).await;

    assert_eq!(finished.stdout, b"hi");
    assert_eq!(finished.stderr, b"!");
    assert_eq!(finished.endings, [(Ending::Exited(2), 5)]);
}

#[tokio::test]
async fn an_exit_names_a_timeout_then_a_signal_then_a_status() {
    let (_scratch, client, mut fake) = connect().await;
    let client = std::sync::Arc::new(client);
    let mut endings = Vec::new();
    for (process, exit) in [
        (1, r#""exit_code":null,"signal":15,"timed_out":true"#),
        (2, r#""exit_code":null,"signal":9,"timed_out":false"#),
        (3, r#""exit_code":null,"signal":null,"timed_out":false"#),
    ] {
        let spawner = std::sync::Arc::clone(&client);
        let spawning = tokio::spawn(async move { spawner.spawn(Spawn::program("x")).await });
        started(&mut fake, process).await;
        let started = spawning.await.unwrap().unwrap();
        fake.say(&format!(
            r#"{{"jsonrpc":"2.0","method":"process/exited","params":{{"process_id":{process},{exit},"omitted_bytes":0}}}}"#
        ))
        .await;
        endings.push(finish(started).await.endings);
    }

    assert_eq!(
        endings,
        [
            [(Ending::TimedOut, 0)],
            [(Ending::Signaled(9), 0)],
            [(Ending::Interrupted, 0)],
        ]
    );
}

#[tokio::test]
async fn an_error_answer_is_a_refusal_with_its_code() {
    let (_scratch, client, mut fake) = connect().await;
    let reading = tokio::spawn(async move { client.read_file("x", 1).await });
    let id = fake.request().await["id"].clone();
    fake.say(&format!(
        r#"{{"jsonrpc":"2.0","error":{{"code":-32010,"message":"no"}},"id":{id}}}"#
    ))
    .await;

    let refused = reading.await.unwrap().unwrap_err();

    assert!(refused.is_path_refused(), "{refused}");
}

#[tokio::test]
async fn a_spawn_answer_that_is_not_a_process_fails_the_spawn() {
    let (_scratch, client, mut fake) = connect().await;
    let spawning = tokio::spawn(async move { client.spawn(Spawn::program("x")).await });
    let id = fake.request().await["id"].clone();
    fake.say(&format!(r#"{{"jsonrpc":"2.0","result":5,"id":{id}}}"#))
        .await;

    let refused = spawning.await.unwrap().unwrap_err();

    assert!(
        !refused.is_connection_lost() && !refused.is_path_refused(),
        "{refused}"
    );
}

#[tokio::test]
async fn calls_waiting_when_the_executor_vanishes_fail_as_lost() {
    let (_scratch, client, mut fake) = connect().await;
    let client = std::sync::Arc::new(client);
    let (reader, spawner) = (
        std::sync::Arc::clone(&client),
        std::sync::Arc::clone(&client),
    );
    let reading = tokio::spawn(async move { reader.read_file("x", 1).await });
    let spawning = tokio::spawn(async move { spawner.spawn(Spawn::program("x")).await });
    fake.request().await;
    fake.request().await;

    drop(fake);

    assert!(reading.await.unwrap().unwrap_err().is_connection_lost());
    assert!(spawning.await.unwrap().unwrap_err().is_connection_lost());
}
