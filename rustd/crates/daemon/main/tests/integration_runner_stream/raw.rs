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
}

impl Native {
    pub(crate) async fn connect(run: &Scenario, transport: Transport) -> Self {
        let stop = CancellationToken::new();
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let endpoint = Endpoint::parse(&format!(
            "https://[::1]:{}",
            run.booted.runner_stream_address.port()
        ))
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
            &run.token,
            &budget,
            &stop,
            deadline,
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
        let ended = tokio::time::timeout(bound, read(&mut pipe.reader))
            .await
            .expect("silent native session closes");
        assert!(
            !matches!(ended, Ok(Some(_))),
            "unexpected frame on closed session"
        );
    }

    pub(crate) async fn close(self) {
        self.connection
            .close(Instant::now() + STREAM_ANSWER_TIMEOUT)
            .await
            .expect("native owner joins");
    }
}

fn response_reader(
    response: Response<af_stream::Incoming>,
    budget: &Budget,
    kind: StreamKind,
    stop: &CancellationToken,
) -> Reader<IncomingReader> {
    assert!(response.status().is_success());
    assert_eq!(
        response.headers()[protocol::VERSION_HEADER],
        protocol::VERSION
    );
    Reader::new(
        response.into_body().reader(),
        budget.clone(),
        kind,
        stop.clone(),
        STREAM_ANSWER_TIMEOUT,
    )
}

async fn read(reader: &mut Reader<IncomingReader>) -> af_stream::Result<Option<Value>> {
    reader.next().await?.map(|frame| frame.decode()).transpose()
}

async fn control_hello(
    connection: &mut Connection,
    endpoint: &Endpoint,
    token: &str,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
) -> Pipe {
    let (mut writer, body) = Outgoing::pipe();
    let opening = Request::post(endpoint.uri().clone())
        .header(protocol::VERSION_HEADER, protocol::VERSION)
        .header(protocol::STREAM_KIND_HEADER, StreamKind::Control.as_str())
        .header("authorization", format!("Bearer {token}"))
        .body(body)
        .expect("opening metadata");
    let hello = json!({
        "type":"hello", "request_id":1,
        "accepted_kinds":af_wire::lease::AcceptedKinds::FLEET,
        "running":[], "credits":0
    });
    let frame = af_stream::encode(&hello, budget, StreamKind::Control, stop, deadline)
        .await
        .expect("hello encodes");
    let (response, written) = tokio::join!(
        connection.open(opening, deadline),
        af_stream::write(&mut writer, &frame, stop, deadline)
    );
    written.expect("hello reaches the daemon");
    let reader = response_reader(
        response.expect("opening response"),
        budget,
        StreamKind::Control,
        stop,
    );
    Pipe { writer, reader }
}

async fn confirm(
    pipe: &mut Pipe,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
) -> String {
    let prepared = tokio::time::timeout_at(deadline, read(&mut pipe.reader))
        .await
        .expect("prepared deadline")
        .expect("prepared decodes")
        .expect("prepared frame");
    assert_eq!((*crate::wire::pointer(&prepared, "/type")), "prepared");
    let owner = (*crate::wire::pointer(&prepared, "/connection_id"))
        .as_str()
        .expect("proposed owner")
        .to_owned();
    let confirm = json!({"type":"confirm", "request_id":1, "connection_id":owner});
    let frame = af_stream::encode(&confirm, budget, StreamKind::Control, stop, deadline)
        .await
        .expect("confirmation encodes");
    af_stream::write(&mut pipe.writer, &frame, stop, deadline)
        .await
        .expect("confirm owner");
    let welcome = tokio::time::timeout_at(deadline, read(&mut pipe.reader))
        .await
        .expect("welcome deadline")
        .expect("welcome decodes")
        .expect("welcome frame");
    assert_eq!((*crate::wire::pointer(&welcome, "/type")), "welcome");
    assert_eq!((*crate::wire::pointer(&welcome, "/connection_id")), owner);
    owner
}

async fn secondary(
    connection: &mut Connection,
    endpoint: &Endpoint,
    owner: &str,
    budget: &Budget,
    stop: &CancellationToken,
    deadline: Instant,
) -> BTreeMap<StreamKind, Pipe> {
    let mut pipes = BTreeMap::new();
    for kind in [StreamKind::Work, StreamKind::Reports, StreamKind::Activity] {
        let (writer, body) = Outgoing::pipe();
        let request = Request::post(endpoint.uri().clone())
            .header(protocol::VERSION_HEADER, protocol::VERSION)
            .header(protocol::STREAM_KIND_HEADER, kind.as_str())
            .header(protocol::SESSION_HEADER, owner)
            .body(body)
            .expect("secondary metadata");
        let response = connection
            .open(request, deadline)
            .await
            .expect("secondary response");
        let reader = response_reader(response, budget, kind, stop);
        pipes.insert(kind, Pipe { writer, reader });
    }
    pipes
}
