//! In-process body pipes retain native framing, connection binding and production middleware.

use af_stream::{Budget, OwnedFrame, Reader};
use af_wire::protocol::{self, StreamKind};
use axum::{Router, body::Body, response::Response};
use futures_util::TryStreamExt as _;
use serde_json::{Value, json};
use std::{collections::BTreeMap, pin::Pin, sync::Arc};
use tokio::{
    io::{AsyncRead, DuplexStream},
    time::Instant,
};
use tokio_util::{
    io::StreamReader,
    sync::{CancellationToken, DropGuard},
};
use tower::ServiceExt as _;

const TIMEOUT: std::time::Duration = af_core::timing::STREAM_ANSWER_TIMEOUT;
type Input = Reader<Pin<Box<dyn AsyncRead + Send>>>;

pub(crate) struct Session {
    output: Budget,
    input: Budget,
    stop: CancellationToken,
    _guard: DropGuard,
    context: Arc<afd_gateway::ConnectionContext>,
    streams: BTreeMap<StreamKind, (DuplexStream, Input)>,
}

impl Session {
    pub(crate) async fn open(router: &Router, bearer: Option<&str>) -> Result<Self, Box<Response>> {
        let mut session = Self::new();
        session.control(router, bearer).await?;
        let owner = session.confirm().await;
        session.attach(router, &owner).await;
        Ok(session)
    }

    fn new() -> Self {
        let stop = CancellationToken::new();
        let context = Arc::new(afd_gateway::ConnectionContext::new(
            af_stream::ConnectionInfo {
                transport: af_stream::Transport::H2,
                peer: ([127, 0, 0, 1], 0).into(),
                stop: stop.clone(),
                native_stats: None,
            },
        ));
        let (input, output) = Budget::duplex(af_stream::MemoryLimits::default());
        Self {
            input,
            output,
            _guard: stop.clone().drop_guard(),
            stop,
            context,
            streams: BTreeMap::new(),
        }
    }

    /// Opening refusals must arrive even when the runner has sent no hello.
    pub(crate) async fn opening(router: &Router, bearer: &str, version: &str) -> Response {
        Self::new()
            .open_stream(
                router,
                StreamKind::Control,
                Some(bearer),
                None,
                Body::empty(),
                version,
            )
            .await
    }

    async fn control(
        &mut self,
        router: &Router,
        bearer: Option<&str>,
    ) -> Result<(), Box<Response>> {
        let (mut upload, body) = af_stream::Outgoing::pipe();
        let hello = json!({"type":"hello","request_id":1,"accepted_kinds":["authoring"],"running":[],"credits":0});
        self.write_to(&mut upload, StreamKind::Control, &hello)
            .await;
        let response = self
            .open_stream(
                router,
                StreamKind::Control,
                bearer,
                None,
                Body::new(body),
                protocol::VERSION,
            )
            .await;
        if response.status() != http::StatusCode::OK {
            return Err(Box::new(response));
        }
        let reader = self.reader(response.into_body(), StreamKind::Control);
        self.streams.insert(StreamKind::Control, (upload, reader));
        Ok(())
    }

    async fn confirm(&mut self) -> String {
        let prepared = self
            .read(StreamKind::Control)
            .await
            .expect("prepared native session");
        let prepared: Value = prepared.decode().expect("prepared JSON");
        let owner = (*prepared
            .pointer("/connection_id")
            .expect("native response field"))
        .as_str()
        .expect("prepared owner")
        .to_owned();
        let confirm = json!({"type":"confirm","request_id":1,"connection_id":owner});
        self.write(StreamKind::Control, &confirm).await;
        let welcome = self
            .read(StreamKind::Control)
            .await
            .expect("welcomed native session");
        let welcome: Value = welcome.decode().expect("welcome JSON");
        assert_eq!(
            (*welcome.pointer("/type").expect("native response field")),
            "welcome"
        );
        assert_eq!(
            (*welcome
                .pointer("/connection_id")
                .expect("native response field")),
            owner
        );
        owner
    }

    async fn attach(&mut self, router: &Router, owner: &str) {
        for kind in [StreamKind::Work, StreamKind::Reports, StreamKind::Activity] {
            let (upload, body) = af_stream::Outgoing::pipe();
            let response = self
                .open_stream(
                    router,
                    kind,
                    None,
                    Some(owner),
                    Body::new(body),
                    protocol::VERSION,
                )
                .await;
            assert_eq!(
                response.status(),
                http::StatusCode::OK,
                "attach native lane"
            );
            let reader = self.reader(response.into_body(), kind);
            self.streams.insert(kind, (upload, reader));
        }
    }

    async fn open_stream(
        &self,
        router: &Router,
        kind: StreamKind,
        bearer: Option<&str>,
        owner: Option<&str>,
        body: Body,
        version: &str,
    ) -> Response {
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .version(http::Version::HTTP_2)
            .uri(af_wire::paths::RUNNER_STREAM)
            .header(protocol::VERSION_HEADER, version)
            .header(protocol::STREAM_KIND_HEADER, kind.as_str());
        if let Some(token) = bearer {
            request = request.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(owner) = owner {
            request = request.header(protocol::SESSION_HEADER, owner);
        }
        let mut request = request.body(body).expect("native stream request");
        request.extensions_mut().insert(Arc::clone(&self.context));
        request
            .extensions_mut()
            .insert(afd_http::auth::NativeRunnerConnection::new(Arc::clone(
                &self.context,
            )));
        router
            .clone()
            .oneshot(request)
            .await
            .expect("infallible native router")
    }

    fn reader(&self, body: Body, kind: StreamKind) -> Input {
        let input = StreamReader::new(body.into_data_stream().map_err(std::io::Error::other));
        Reader::new(
            Box::pin(input),
            self.input.clone(),
            kind,
            self.stop.clone(),
            TIMEOUT,
        )
    }

    async fn write_to(&self, upload: &mut DuplexStream, kind: StreamKind, message: &Value) {
        let deadline = Instant::now() + TIMEOUT;
        let frame = af_stream::encode(message, &self.output, kind, &self.stop, deadline)
            .await
            .expect("encode native frame");
        af_stream::write(upload, &frame, &self.stop, deadline)
            .await
            .expect("write native frame");
    }

    pub(crate) async fn write(&mut self, kind: StreamKind, message: &Value) {
        let deadline = Instant::now() + TIMEOUT;
        let frame = af_stream::encode(message, &self.output, kind, &self.stop, deadline)
            .await
            .expect("encode native frame");
        let (upload, _) = self.streams.get_mut(&kind).expect("attached output lane");
        af_stream::write(upload, &frame, &self.stop, deadline)
            .await
            .expect("write native frame");
    }

    pub(crate) async fn read(&mut self, kind: StreamKind) -> Option<OwnedFrame> {
        let (_, reader) = self.streams.get_mut(&kind).expect("attached input lane");
        let result = tokio::time::timeout(TIMEOUT, reader.next())
            .await
            .expect("native reply deadline");
        if result.as_ref().is_err_and(|error| {
            matches!(
                error.reason(),
                af_stream::Reason::Cancelled | af_stream::Reason::Closed
            )
        }) {
            return None;
        }
        result.expect("unexpected native read failure")
    }
}
