//! The client against a hand-driven executor: every shape of answer, and
//! every message it must survive.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use afd_core::test_util::trace::Capture;
use afr_executor::{Client, Ending, Executor as _, READ_CHUNK_BYTES, Spawn};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};

use crate::support::{
    INTERNAL_ERROR, KIB, MIB, PATH_REFUSED, PATIENCE, finish, is_lost, refused_with, scratch,
};

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
    let client = Client::connect_within(&socket, PATIENCE).await.unwrap();
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
        tokio::spawn(async move { (client.spawn(&Spawn::program("anything")).await, client) });
    started(&mut fake, 7).await;
    let (process, _client) = spawning.await.unwrap();
    let process = process.unwrap();

    for noise in [
        "not json",
        r#"{"jsonrpc":"2.0"}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":9,"stream":"stdout","data":"aGk="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/progress","params":{}}"#,
        r#"{"jsonrpc":"2.0","result":null,"id":99}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stdout","data":"%%%"}}"#,
        r#"{"jsonrpc":"2.0","method":"process/exited","params":[7]}"#,
        r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"ending":{"kind":"vanished"},"output_abandoned":false}}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stdout","data":"aGk="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":7,"stream":"stderr","data":"IQ=="}}"#,
        r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"ending":{"kind":"exited","code":2},"output_abandoned":false}}"#,
    ] {
        fake.say(noise).await;
    }
    let finished = finish(process).await;

    assert_eq!(finished.stdout, b"hi");
    assert_eq!(finished.stderr, b"!");
    assert_eq!(finished.endings, [Ending::Exited(2)]);
}

/// The decoder's sentence can quote the value it refused, and the executor
/// shares its sandbox with tenant code, so only where a message failed is
/// logged.
#[tokio::test]
async fn a_message_that_does_not_decode_is_logged_without_what_it_carried() {
    let capture = Capture::install();
    let (_scratch, client, mut fake) = connect().await;
    let spawning =
        tokio::spawn(async move { (client.spawn(&Spawn::program("anything")).await, client) });
    started(&mut fake, 7).await;
    let (process, _client) = spawning.await.unwrap();
    let process = process.unwrap();

    fake.say(r#"{"jsonrpc":"2.0","method":"process/output","params":{"process_id":"sk-live-secret","stream":"stdout","data":""}}"#).await;
    fake.say(r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"ending":{"kind":"exited","code":0},"output_abandoned":false}}"#).await;
    finish(process).await;

    let unreadable = capture.only("executor_message_unreadable");
    assert!(
        unreadable
            .fields
            .values()
            .all(|value| !value.contains("sk-live-secret")),
        "{unreadable:?}"
    );
    assert!(unreadable.field("column").is_some(), "{unreadable:?}");
}

#[tokio::test]
async fn every_ending_the_wire_spells_reaches_the_caller_as_itself() {
    let (_scratch, client, mut fake) = connect().await;
    let client = std::sync::Arc::new(client);
    let mut endings = Vec::new();
    for (process, ending) in [
        (1, r#"{"kind":"timed_out"}"#),
        (2, r#"{"kind":"signaled","code":9}"#),
        (3, r#"{"kind":"interrupted"}"#),
    ] {
        let spawner = std::sync::Arc::clone(&client);
        let spawning = tokio::spawn(async move { spawner.spawn(&Spawn::program("x")).await });
        started(&mut fake, process).await;
        let started = spawning.await.unwrap().unwrap();
        fake.say(&format!(
            r#"{{"jsonrpc":"2.0","method":"process/exited","params":{{"process_id":{process},"ending":{ending},"output_abandoned":false}}}}"#
        ))
        .await;
        endings.push(finish(started).await.endings);
    }

    assert_eq!(
        endings,
        [
            [Ending::TimedOut],
            [Ending::Signaled(9)],
            [Ending::Interrupted],
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

    assert!(refused_with(&refused, PATH_REFUSED), "{refused}");
}

#[tokio::test]
async fn a_spawn_answer_that_is_not_a_process_fails_the_spawn() {
    let (_scratch, client, mut fake) = connect().await;
    let spawning = tokio::spawn(async move { client.spawn(&Spawn::program("x")).await });
    let id = fake.request().await["id"].clone();
    fake.say(&format!(r#"{{"jsonrpc":"2.0","result":5,"id":{id}}}"#))
        .await;

    let refused = spawning.await.unwrap().unwrap_err();

    assert!(refused.to_string().contains("did not decode"), "{refused}");
    assert!(!is_lost(&refused));
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
    let spawning = tokio::spawn(async move { spawner.spawn(&Spawn::program("x")).await });
    fake.request().await;
    fake.request().await;

    drop(fake);

    assert!(is_lost(&reading.await.unwrap().unwrap_err()));
    assert!(is_lost(&spawning.await.unwrap().unwrap_err()));
}

#[tokio::test]
async fn a_process_whose_caller_left_before_it_started_is_killed() {
    let (_scratch, client, mut fake) = connect().await;
    let client = std::sync::Arc::new(client);
    let spawner = std::sync::Arc::clone(&client);
    let spawning = tokio::spawn(async move { spawner.spawn(&Spawn::program("x")).await });
    let id = fake.request().await["id"].clone();

    spawning.abort();
    let _cancelled = spawning.await;
    fake.say(&format!(
        r#"{{"jsonrpc":"2.0","result":{{"process_id":4}},"id":{id}}}"#
    ))
    .await;
    let follow_up = tokio::time::timeout(PATIENCE, fake.request())
        .await
        .unwrap();

    assert_eq!(follow_up["method"], "process/kill");
    assert_eq!(follow_up["params"]["process_id"], 4);
}

/// A chunk longer than one read is not something this executor sends: it is
/// dropped where it would otherwise pin its whole allocation, and the
/// process's other output and its ending still arrive.
#[tokio::test]
async fn a_chunk_longer_than_one_read_is_dropped_and_the_rest_still_arrives() {
    let (_scratch, client, mut fake) = connect().await;
    let spawning =
        tokio::spawn(async move { (client.spawn(&Spawn::program("anything")).await, client) });
    started(&mut fake, 7).await;
    let (process, _client) = spawning.await.unwrap();
    let process = process.unwrap();
    let oversized = BASE64_STANDARD.encode(vec![b'x'; READ_CHUNK_BYTES + 1]);
    let whole = BASE64_STANDARD.encode(vec![b'y'; READ_CHUNK_BYTES]);

    for data in [&oversized, &whole] {
        fake.say(&format!(
            r#"{{"jsonrpc":"2.0","method":"process/output","params":{{"process_id":7,"stream":"stdout","data":"{data}"}}}}"#
        ))
        .await;
    }
    fake.say(r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"ending":{"kind":"exited","code":0},"output_abandoned":false}}"#).await;
    let finished = finish(process).await;

    assert_eq!(finished.stdout, vec![b'y'; READ_CHUNK_BYTES]);
    assert_eq!(finished.endings, [Ending::Exited(0)]);
}

/// A sandbox that keeps sending chunks past one read earns one warning per
/// connection; the rest are a whisper, so it cannot flood the journal.
#[tokio::test]
async fn oversized_chunks_warn_once_per_connection_then_whisper() {
    let capture = Capture::install();
    let (_scratch, client, mut fake) = connect().await;
    let spawning =
        tokio::spawn(async move { (client.spawn(&Spawn::program("anything")).await, client) });
    started(&mut fake, 7).await;
    let (process, _client) = spawning.await.unwrap();
    let process = process.unwrap();
    let oversized = BASE64_STANDARD.encode(vec![b'x'; READ_CHUNK_BYTES + 1]);

    for _ in 0..3 {
        fake.say(&format!(
            r#"{{"jsonrpc":"2.0","method":"process/output","params":{{"process_id":7,"stream":"stdout","data":"{oversized}"}}}}"#
        ))
        .await;
    }
    fake.say(r#"{"jsonrpc":"2.0","method":"process/exited","params":{"process_id":7,"ending":{"kind":"exited","code":0},"output_abandoned":false}}"#).await;
    finish(process).await;

    let logged: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("executor_output_oversized"))
        .collect();
    assert_eq!(logged.len(), 3, "{logged:?}");
    assert_eq!(
        logged
            .iter()
            .filter(|event| event.level == tracing::Level::WARN)
            .count(),
        1,
        "one warning per connection"
    );
}

/// A refusal whose message runs to a mebibyte reaches the caller cut to the
/// cap: the executor shares its sandbox with tenant code, and what it says
/// about a failed call is read by a model.
#[tokio::test]
async fn a_refusals_message_longer_than_the_cap_is_cut() {
    let (_scratch, client, mut fake) = connect().await;
    let reading = tokio::spawn(async move { client.read_file("x", 1).await });
    let id = fake.request().await["id"].clone();
    let flood = "x".repeat(MIB);
    fake.say(&format!(
        r#"{{"jsonrpc":"2.0","error":{{"code":{INTERNAL_ERROR},"message":"{flood}"}},"id":{id}}}"#
    ))
    .await;

    let refused = reading.await.unwrap().unwrap_err();

    assert!(refused_with(&refused, INTERNAL_ERROR), "{refused}");
    assert_eq!(refused.wire_message().len(), 4 * KIB);
}

/// A result that puts a mebibyte where a boolean belongs reaches the caller
/// as a decode failure cut to the cap: the decoder echoes the value it
/// refused, and that value is the sandbox's.
#[tokio::test]
async fn a_result_that_does_not_decode_echoes_at_most_the_cap() {
    let (_scratch, client, mut fake) = connect().await;
    let reading = tokio::spawn(async move { client.read_file("x", 1).await });
    let id = fake.request().await["id"].clone();
    let flood = "x".repeat(MIB);
    fake.say(&format!(
        r#"{{"jsonrpc":"2.0","result":{{"content":"","truncated":"{flood}"}},"id":{id}}}"#
    ))
    .await;

    let refused = reading.await.unwrap().unwrap_err();

    assert!(refused.to_string().contains("did not decode"), "{refused}");
    assert_eq!(refused.wire_message().len(), 4 * KIB);
}
