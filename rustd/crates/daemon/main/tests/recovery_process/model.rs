//! A controlled `OpenAI` stream on pinned TLS; every accepted request is observed by the parent.

use super::Checked;
use af_stream::{Incoming, Outgoing, test_util::IdentityFixture};
use axum::{Json, Router, body::Body, extract::State, http, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::{net::SocketAddr, path::PathBuf};
use tokio::sync::{mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use tower::ServiceExt as _;

const HOST: &str = af_providers::testing::tls::HOST;

pub(super) struct Asked {
    pub body: Value,
    answer: oneshot::Sender<String>,
}

impl Asked {
    pub(super) fn answer(self, text: &str) -> Checked {
        self.answer
            .send(text.to_owned())
            .map_err(|_answer| std::io::Error::other("model request dropped"))?;
        Ok(())
    }
}

pub(super) struct Model {
    identity: IdentityFixture,
    pub address: SocketAddr,
    requests: mpsc::Receiver<Asked>,
    stop: CancellationToken,
    serving: AbortOnDropHandle<af_stream::Result<()>>,
}

impl Model {
    pub(super) async fn start() -> Checked<Self> {
        let identity = IdentityFixture::new(vec![HOST.to_owned()])?;
        let listener =
            af_stream::Listener::bind(([127, 0, 0, 1], 0).into(), identity.identity()).await?;
        let address = listener.local_addr()?;
        let (sender, requests) = mpsc::channel(8);
        let router = Router::new()
            .route("/v1/chat/completions", post(answer))
            .with_state(sender);
        let stop = CancellationToken::new();
        let serving = tokio::spawn(listener.serve(
            move |_info| {
                let router = router.clone();
                tower::service_fn(move |request: http::Request<Incoming>| {
                    let router = router.clone();
                    async move {
                        let response = router.oneshot(request.map(Body::new)).await?;
                        Ok::<_, std::convert::Infallible>(response.map(Outgoing::new))
                    }
                })
            },
            stop.clone(),
            stop.clone(),
        ));
        Ok(Self {
            identity,
            address,
            requests,
            stop,
            serving: AbortOnDropHandle::new(serving),
        })
    }

    pub(super) fn base(&self) -> String {
        format!("https://{HOST}:{}/v1", self.address.port())
    }

    pub(super) fn root(&self) -> PathBuf {
        self.identity.root()
    }

    pub(super) async fn next(&mut self) -> Checked<Asked> {
        tokio::time::timeout(std::time::Duration::from_secs(75), self.requests.recv())
            .await?
            .ok_or_else(|| std::io::Error::other("model listener stopped").into())
    }

    pub(super) async fn finish(self) -> Checked {
        self.stop.cancel();
        self.serving.await??;
        Ok(())
    }
}

async fn answer(
    State(sender): State<mpsc::Sender<Asked>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let (answer, received) = oneshot::channel();
    if sender.send(Asked { body, answer }).await.is_err() {
        return http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Ok(text) = received.await else {
        return http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let id = afd_db::test_util::mint_id();
    let chunk = json!({"id":id,"object":"chat.completion.chunk","created":0,"model":"fixture",
        "choices":[{"index":0,"delta":{"role":"assistant","content":text},"finish_reason":null}]});
    let end = json!({"id":id,"object":"chat.completion.chunk","created":0,"model":"fixture",
        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":321,"completion_tokens":123,"total_tokens":444}});
    (
        [(http::header::CONTENT_TYPE, "text/event-stream")],
        format!("data: {chunk}\n\ndata: {end}\n\ndata: [DONE]\n\n"),
    )
        .into_response()
}
