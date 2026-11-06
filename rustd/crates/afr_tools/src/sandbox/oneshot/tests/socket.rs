//! Notify establishes spawn completion; a real RPC orders the partial output.
#![expect(
    clippy::expect_used,
    reason = "socket fixture preconditions fail loudly"
)]

use std::time::Duration;

use afr_executor::{Client, Ending, Executor, Listing, Process, ProcessId, Spawn};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Notify;

pub(super) const PATIENCE: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct Witness {
    pub(super) client: Client,
    started: Notify,
}

#[async_trait::async_trait]
impl Executor for Witness {
    async fn spawn(&self, spawn: &Spawn) -> afr_executor::Result<Process> {
        let process = self.client.spawn(spawn).await?;
        // The tool polls through this notification to its ending wait.
        self.started.notify_one();
        Ok(process)
    }

    async fn write(&self, process: ProcessId, data: Bytes) -> afr_executor::Result<()> {
        self.client.write(process, data).await
    }

    async fn kill(&self, process: ProcessId) -> afr_executor::Result<()> {
        self.client.kill(process).await
    }
}

#[derive(Debug)]
pub(super) struct Peer(BufReader<UnixStream>);

pub(super) fn pair() -> (Witness, Peer) {
    let (ours, theirs) = UnixStream::pair().expect("socket pair");
    let client = Client::connected(ours.into_std().expect("native socket")).expect("native client");
    (
        Witness {
            client,
            started: Notify::new(),
        },
        Peer(BufReader::new(theirs)),
    )
}

impl Peer {
    async fn next(&mut self) -> Value {
        let mut line = String::new();
        let bytes = tokio::time::timeout(PATIENCE, self.0.read_line(&mut line))
            .await
            .expect("runner sends the expected request before the test deadline")
            .expect("peer reads request");
        assert!(bytes > 0, "connection remains open");
        serde_json::from_str(&line).expect("native JSON-RPC request")
    }

    async fn say(&mut self, mut value: Value) {
        value
            .as_object_mut()
            .expect("fixture object")
            .insert("jsonrpc".to_owned(), json!("2.0"));
        self.0
            .get_mut()
            .write_all(format!("{value}\n").as_bytes())
            .await
            .expect("peer writes response");
    }

    pub(super) async fn spawn(&mut self, witness: &Witness) -> Value {
        let request = self.next().await;
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some("process/spawn")
        );
        self.say(
            json!({"id":request.get("id").expect("native call id"), "result":{"process_id":7}}),
        )
        .await;
        self.say(json!({"method":"process/output", "params":{"process_id":7,
            "stream":"stdout", "data":"cGFydGlhbAo="}}))
            .await;
        witness.started.notified().await;
        // Socket order puts the output before this RPC's reply.
        self.responsive(&witness.client).await;
        request
    }

    pub(super) async fn responsive(&mut self, client: &Client) {
        let answer = async {
            let request = self.next().await;
            assert_eq!(
                request.get("method").and_then(Value::as_str),
                Some("fs/list"),
                "no duplicate spawn or kill"
            );
            self.say(json!({"id":request.get("id").expect("native call id"), "result":{"entries":[], "truncated":false}}))
                .await;
        };
        let (listing, ()) = tokio::join!(client.list_dir("."), answer);
        let listing = listing.expect("connection remains reusable");
        assert_eq!(
            listing,
            Listing {
                entries: vec![],
                truncated: false
            }
        );
    }

    pub(super) async fn killed(&mut self) {
        let request = self.next().await;
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some("process/kill")
        );
        assert_eq!(request.get("params"), Some(&json!({"process_id":7})));
        self.say(json!({"id":request.get("id").expect("native call id"), "result":null}))
            .await;
    }

    pub(super) async fn closed(&mut self) {
        let mut line = String::new();
        let bytes = tokio::time::timeout(PATIENCE, self.0.read_line(&mut line))
            .await
            .expect("lost link closes promptly")
            .expect("read peer closure");
        assert_eq!(
            (bytes, line.as_str()),
            (0, ""),
            "no further spawn or futile kill"
        );
    }

    pub(super) async fn ended(&mut self, ending: Ending) {
        self.say(json!({"method":"process/exited", "params":{"process_id":7,
            "ending":ending, "output_abandoned":false}}))
            .await;
    }
}
