//! HTTP opening refusals terminate production recovery without another carrier attempt.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_core::{
    env::MapEnv,
    error_code::{self, ErrorCode},
};
use af_stream::{Incoming, Listener, Outgoing, Transport, test_util::IdentityFixture};
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt as _, Full};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use super::Downlink;
use crate::{
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    error::ErrorKind,
    halt::Halt,
};

type Proof<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Default)]
struct Census {
    http3: AtomicUsize,
    http2: AtomicUsize,
    control: AtomicUsize,
}

impl Census {
    fn connected(&self, transport: Transport) {
        match transport {
            Transport::H3 => self.http3.fetch_add(1, Ordering::SeqCst),
            Transport::H2 => self.http2.fetch_add(1, Ordering::SeqCst),
        };
    }

    fn assert_one_refused_opening(&self) {
        assert_eq!(self.http3.load(Ordering::SeqCst), 1);
        assert_eq!(self.http2.load(Ordering::SeqCst), 0);
        assert_eq!(self.control.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn an_invalid_runner_credential_stops_after_one_native_opening() -> Proof {
    refused(
        StatusCode::UNAUTHORIZED,
        error_code::RUN_INVALID_RUNNER_TOKEN,
    )
    .await
}

#[tokio::test]
async fn an_unsupported_protocol_stops_after_one_native_opening() -> Proof {
    refused(
        StatusCode::BAD_REQUEST,
        error_code::RUN_PROTOCOL_UNSUPPORTED,
    )
    .await
}

async fn refused(status: StatusCode, code: ErrorCode) -> Proof {
    let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
    let listener = Listener::bind("[::1]:0".parse()?, identity.identity()).await?;
    let origin = format!("https://localhost:{}", listener.local_addr()?.port());
    let census = Arc::new(Census::default());
    let stop = CancellationToken::new();
    let _stop_on_exit = stop.clone().drop_guard();
    let mut server = serve(listener, status, code, Arc::clone(&census), &stop)?;
    let config = configured(&origin, &identity.root())?;
    let (_handle, downlink, _offers) = Downlink::new(&config)?;
    let halt = Halt::new(CancellationToken::new());
    let result = tokio::time::timeout(Duration::from_secs(8), downlink.run(&halt)).await?;
    let failure = result.err().ok_or("the refused opening was accepted")?;
    assert!(matches!(failure.kind(), ErrorKind::Refused { .. }));
    assert_eq!(failure.refusal_status(), Some(status.as_u16()));
    assert_eq!(failure.refusal_code(), Some(code));
    assert_eq!(failure.code(), code);
    assert!(!failure.is_retryable());
    assert_eq!(halt.token_refused(), status == StatusCode::UNAUTHORIZED);
    assert!(halt.serving().is_cancelled() && halt.running().is_cancelled());
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), &mut server).await???;
    census.assert_one_refused_opening();
    Ok(())
}

fn serve(
    listener: Listener,
    status: StatusCode,
    code: ErrorCode,
    census: Arc<Census>,
    stop: &CancellationToken,
) -> Proof<AbortOnDropHandle<af_stream::Result<()>>> {
    let payload = Bytes::from(serde_json::to_vec(&serde_json::json!({
        "error_code":code.as_str()
    }))?);
    let serving = listener.serve(
        move |info| {
            census.connected(info.transport);
            let census = Arc::clone(&census);
            let payload = payload.clone();
            tower::service_fn(move |request: Request<Incoming>| {
                if request.headers().get(af_wire::protocol::STREAM_KIND_HEADER)
                    == Some(&http::HeaderValue::from_static("control"))
                {
                    census.control.fetch_add(1, Ordering::SeqCst);
                }
                let response = problem(request, status, payload.clone());
                async move { Ok::<_, Infallible>(response) }
            })
        },
        stop.clone(),
        stop.clone(),
    );
    Ok(AbortOnDropHandle::new(tokio::spawn(serving)))
}

fn problem(request: Request<Incoming>, status: StatusCode, payload: Bytes) -> Response<Outgoing> {
    // Keep the unread hello upload alive until the finite refusal body has been delivered.
    let body = Full::new(payload).map_frame(move |frame| {
        let _request = &request;
        frame
    });
    let mut response = Response::new(Outgoing::new(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/problem+json"),
    );
    response
}

fn configured(origin: &str, root: &std::path::Path) -> Proof<Config> {
    Ok(Config::from_env(&MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, origin),
        (
            ENV_STREAM_CA,
            root.to_str().ok_or("fixture root is not UTF-8")?,
        ),
        (ENV_RUNNER_TOKEN, "agt_r_terminal_fixture"),
    ]))?)
}
