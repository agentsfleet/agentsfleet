//! Native frames keep wire order visible without automatic reconnect or heartbeat writers.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions fail loudly"
)]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::{Budget, Connection, Endpoint, IncomingReader, Outgoing, Reader, Transport};
use af_wire::protocol::{self, StreamKind};
use axum::http::{Request, Response};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tokio::{io::DuplexStream, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::e2e::Scenario;

mod opening;
use opening::{confirm, control_hello, read, secondary};

struct Pipe {
    writer: DuplexStream,
    reader: Reader<IncomingReader>,
}

pub(crate) struct Native {
    connection: Connection,
    pipes: BTreeMap<StreamKind, Pipe>,
    budget: Budget,
    stop: CancellationToken,
    pub(crate) owner: String,
    pub(crate) cancelled: Vec<Value>,
}

impl Native {
    pub(crate) async fn connect(run: &Scenario, transport: Transport) -> Self {
        Self::connect_at(run.booted.runner_stream_address, &run.token, transport).await
    }

    pub(crate) async fn connect_at(
        address: std::net::SocketAddr,
        token: &str,
        transport: Transport,
    ) -> Self {
        Self::connect_running(address, token, transport, &[]).await
    }

    pub(crate) async fn connect_running(
        address: std::net::SocketAddr,
        token: &str,
        transport: Transport,
        running: &[af_core::id::Uuid7],
    ) -> Self {
        let stop = CancellationToken::new();
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let endpoint = Endpoint::parse(&format!("https://localhost:{}", address.port()))
            .expect("native fixture origin");
        let mut connection = Connection::connect(
            transport,
            &endpoint,
            &crate::support::runner_stream::trust(),
            &stop,
            deadline,
        )
        .await
        .expect("native transport connects");
        let budget = Budget::default();
        let mut control = control_hello(
            &mut connection,
            &endpoint,
            token,
            &budget,
            &stop,
            deadline,
            running,
        )
        .await;
        let owner = confirm(&mut control, &budget, &stop, deadline).await;
        let mut pipes =
            secondary(&mut connection, &endpoint, &owner, &budget, &stop, deadline).await;
        pipes.insert(StreamKind::Control, control);
        let mut native = Self {
            connection,
            pipes,
            budget,
            stop,
            owner,
            cancelled: Vec::new(),
        };
        native
            .send(StreamKind::Control, &json!({"type":"ping", "request_id":2}))
            .await;
        let pong = native.expect(StreamKind::Control, "pong").await;
        assert_eq!((*crate::wire::pointer(&pong, "/request_id")), 2);
        native
    }

    pub(crate) async fn send(&mut self, kind: StreamKind, value: &Value) {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let frame = af_stream::encode(value, &self.budget, kind, &self.stop, deadline)
            .await
            .expect("native request encodes");
        let pipe = self
            .pipes
            .get_mut(&kind)
            .expect("all four streams attached");
        af_stream::write(&mut pipe.writer, &frame, &self.stop, deadline)
            .await
            .expect("native request flushed");
    }

    pub(crate) async fn next(&mut self, kind: StreamKind) -> af_stream::Result<Option<Value>> {
        let pipe = self
            .pipes
            .get_mut(&kind)
            .expect("all four streams attached");
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, read(&mut pipe.reader))
            .await
            .expect("native answer deadline")
    }

    pub(crate) async fn expect(&mut self, kind: StreamKind, expected: &str) -> Value {
        for _ in 0..af_wire::protocol::PENDING_MAX {
            let value = self
                .next(kind)
                .await
                .expect("native reply decodes")
                .expect("native reply exists");
            if (*crate::wire::pointer(&value, "/type")) == expected {
                return value;
            }
            if kind == StreamKind::Control && crate::wire::pointer(&value, "/type") == "cancel" {
                self.cancelled.push(value);
                continue;
            }
            assert_eq!(
                (*crate::wire::pointer(&value, "/type")),
                "assignment",
                "unexpected native frame"
            );
        }
        panic!("native reply was hidden behind too many assignments")
    }

    pub(crate) async fn heartbeat(&mut self) {
        self.send(
            StreamKind::Control,
            &json!({
                "type":"heartbeat", "request_id":3, "request":crate::wire::capable_beat()
            }),
        )
        .await;
        let beat = self.expect(StreamKind::Control, "heartbeat").await;
        assert_eq!((*crate::wire::pointer(&beat, "/answer/status")), "ok");
    }

    pub(crate) async fn ready(&mut self) {
        self.send(
            StreamKind::Control,
            &json!({
                "type":"ready", "credits":1, "revision":1, "observed_offers":0
            }),
        )
        .await;
    }

    pub(crate) async fn ended(&mut self, bound: std::time::Duration) {
        let pipe = self
            .pipes
            .get_mut(&StreamKind::Control)
            .expect("control attached");
        // A policy recheck already in flight can precede the terminal read.
        tokio::time::timeout(bound, async {
            while let Ok(Some(frame)) = read(&mut pipe.reader).await {
                assert_eq!(
                    crate::wire::pointer(&frame, "/type"),
                    "assignment",
                    "unexpected frame before native close: {frame:?}"
                );
            }
        })
        .await
        .expect("silent native session closes");
    }

    pub(crate) async fn close(self) {
        self.connection
            .close(Instant::now() + STREAM_ANSWER_TIMEOUT)
            .await
            .expect("native owner joins");
    }
}
