//! An opening the daemon answers out of turn, unversioned or unavailable never
//! becomes a session: each refusal names why, and only a busy daemon is
//! worth another attempt.

#![expect(clippy::unwrap_used, reason = "native test fixtures fail loudly")]

use std::{convert::Infallible, sync::Arc, time::Duration};

use af_stream::{
    Budget, Connection, Incoming, Listener, Outgoing, Reader, Transport, test_util::IdentityFixture,
};
use af_wire::protocol::{self, StreamKind};
use http::{Request, Response, StatusCode};
use serde_json::{Value, json};
use tokio::{io::DuplexStream, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use super::{Opening, Running};
use crate::{
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    downlink::reconnect::Resume,
    error::ErrorKind,
};

type Proof<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The connection the scripted daemon proposes.
const PROPOSED: &str = "019f0000-0000-7000-8000-0000000000a1";

/// How the scripted daemon answers the control stream.
#[derive(Clone, Copy, Debug)]
enum Script {
    /// Answers the opening with this status and no stream.
    Status(u16),
    /// Answers 200 without naming the protocol version it speaks.
    Unversioned,
    /// Answers the hello with something other than a preparation.
    NotPrepared,
    /// Prepares under another request's identifier.
    PreparedOutOfTurn,
    /// Prepares, then answers the confirm with something other than a welcome.
    NotWelcomed,
    /// Prepares, then welcomes a connection it never proposed.
    WelcomesAnother,
}

impl Script {
    /// What the daemon writes after the `seen`th control frame, if anything.
    fn reply(self, seen: usize) -> Option<Value> {
        let prepared = json!({"type":"prepared", "request_id":1,
            "connection_id":PROPOSED, "previous_connection_id":null});
        let pong = json!({"type":"pong", "request_id":1});
        match (self, seen) {
            (Self::NotPrepared, 1) | (Self::NotWelcomed, 2) => Some(pong),
            (Self::PreparedOutOfTurn, 1) => Some(json!({"type":"prepared", "request_id":2,
                "connection_id":PROPOSED, "previous_connection_id":null})),
            (Self::NotWelcomed | Self::WelcomesAnother, 1) => Some(prepared),
            (Self::WelcomesAnother, 2) => Some(json!({"type":"welcome", "request_id":1,
                "connection_id":"019f0000-0000-7000-8000-0000000000a2",
                "heartbeat_interval_ms":1000})),
            _ => None,
        }
    }
}

struct Daemon {
    url: String,
    identity: IdentityFixture,
    stop: CancellationToken,
    _task: AbortOnDropHandle<af_stream::Result<()>>,
}

impl Daemon {
    async fn start(script: Script) -> Proof<Self> {
        let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
        let listener = Listener::bind("[::1]:0".parse()?, identity.identity()).await?;
        let url = format!("https://localhost:{}", listener.local_addr()?.port());
        let stop = CancellationToken::new();
        let streams = stop.clone();
        let task = tokio::spawn(listener.serve(
            move |_connection| {
                let stop = streams.clone();
                tower::service_fn(move |request: Request<Incoming>| {
                    let response = open(script, request, stop.clone());
                    async move { Ok::<_, Infallible>(response) }
                })
            },
            stop.clone(),
            stop.clone(),
        ));
        Ok(Self {
            url,
            identity,
            stop,
            _task: AbortOnDropHandle::new(task),
        })
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

fn open(script: Script, request: Request<Incoming>, stop: CancellationToken) -> Response<Outgoing> {
    let (writer, body) = Outgoing::pipe();
    if let Script::Status(status) = script {
        drop(writer);
        let mut response = Response::new(body);
        *response.status_mut() = StatusCode::from_u16(status).unwrap();
        return response;
    }
    tokio::spawn(converse(script, request.into_body(), writer, stop));
    let mut response = Response::new(body);
    if !matches!(script, Script::Unversioned) {
        response.headers_mut().insert(
            protocol::VERSION_HEADER,
            http::HeaderValue::from_static(protocol::VERSION),
        );
    }
    response
}

/// Reads each control frame and answers it as `script` says.
async fn converse(
    script: Script,
    body: Incoming,
    mut writer: DuplexStream,
    stop: CancellationToken,
) {
    let budget = Budget::default();
    let mut reader = Reader::new(
        body.reader(),
        budget.clone(),
        StreamKind::Control,
        stop.clone(),
        af_core::timing::STREAM_ANSWER_TIMEOUT,
    );
    let mut seen = 0;
    while let Ok(Some(_frame)) = reader.next().await {
        seen += 1;
        let Some(reply) = script.reply(seen) else {
            continue;
        };
        let deadline = Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
        let Ok(frame) =
            af_stream::encode(&reply, &budget, StreamKind::Control, &stop, deadline).await
        else {
            return;
        };
        if af_stream::write(&mut writer, &frame, &stop, deadline)
            .await
            .is_err()
        {
            return;
        }
    }
}

/// Opens one session against `script` and returns why it was refused.
async fn refusal(script: Script) -> Proof<(crate::Error, Resume)> {
    let daemon = Daemon::start(script).await?;
    let root = daemon.identity.root();
    let config = Config::from_env(&af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, daemon.url.as_str()),
        (ENV_STREAM_CA, root.to_str().unwrap()),
        (ENV_RUNNER_TOKEN, "agt_r_opening_fixture"),
    ]))?;
    let stop = CancellationToken::new();
    let mut connection = Connection::connect(
        Transport::H2,
        &config.stream().endpoint,
        &config.stream().trust,
        &stop,
        Instant::now() + Duration::from_secs(5),
    )
    .await?;
    let (input, output) = Budget::duplex(af_stream::MemoryLimits::default());
    let running = Arc::new(Running::default());
    let authorization = http::header::HeaderValue::from_static("Bearer agt_r_opening_fixture");
    let opening = Opening {
        config: config.stream(),
        authorization: &authorization,
        running: &running,
        input: &input,
        output: &output,
        stop: &stop,
        deadline: Instant::now() + Duration::from_secs(7),
    };
    let mut resume = Resume::default();
    let attached = tokio::time::timeout(
        Duration::from_secs(6),
        opening.attach(&mut connection, &mut resume),
    )
    .await?;
    let failure = attached.err().ok_or("the scripted opening was accepted")?;
    connection
        .close(Instant::now() + Duration::from_secs(5))
        .await?;
    Ok((failure, resume))
}

#[tokio::test]
async fn a_daemon_too_busy_to_open_a_stream_is_worth_another_attempt() -> Proof {
    for status in [503, 429] {
        let (failure, resume) = refusal(Script::Status(status)).await?;

        assert!(
            matches!(failure.kind(), ErrorKind::Unavailable { status: seen, .. } if *seen == status),
            "{status}: {failure:?}"
        );
        assert!(failure.is_retryable(), "{status}");
        assert!(resume.unconfirmed().is_none(), "{status}: nothing proposed");
    }
    Ok(())
}

#[tokio::test]
async fn an_opening_answered_out_of_turn_or_unversioned_is_a_protocol_refusal() -> Proof {
    for script in [
        Script::Unversioned,
        Script::NotPrepared,
        Script::PreparedOutOfTurn,
    ] {
        let (failure, resume) = refusal(script).await?;

        assert!(
            matches!(failure.kind(), ErrorKind::StreamProtocol),
            "{script:?}: {failure:?}"
        );
        assert!(
            resume.unconfirmed().is_none(),
            "{script:?}: nothing proposed"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_proposal_welcomed_out_of_turn_or_under_another_id_is_never_confirmed() -> Proof {
    for script in [Script::NotWelcomed, Script::WelcomesAnother] {
        let (failure, resume) = refusal(script).await?;

        assert!(
            matches!(failure.kind(), ErrorKind::StreamProtocol),
            "{script:?}: {failure:?}"
        );
        assert_eq!(
            resume.unconfirmed().map(af_core::id::Uuid7::as_str),
            Some(PROPOSED),
            "{script:?}: the proposal stays unconfirmed"
        );
    }
    Ok(())
}
