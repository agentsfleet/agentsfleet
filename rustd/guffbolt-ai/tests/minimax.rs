//! A model the `minimax` factory registers answers through the public API:
//! the factory, pointed at a fake provider serving real TLS, sends the turn
//! on the Messages wire with the key in `x-api-key`, and the answer streams
//! back as chunks.

#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::serve::Listener;
use futures_util::{StreamExt as _, stream};
use guffbolt_ai::testing::tls;
use guffbolt_ai::wire::transport::Reqwest;
use guffbolt_ai::{Chunk, Message, Model as _, Models, Request, TextKind, providers};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

const KEY: &str = "fixture-minimax-key";
const MODEL: &str = "MiniMax-M3.1-Flash";

/// What the fake provider saw of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    method: String,
    path: String,
    api_key: Option<String>,
    model: Option<String>,
}

/// The fake's TLS listener: a plain listener whose connections it accepts
/// through `tls`.
struct Tls {
    listener: TcpListener,
    tls: TlsAcceptor,
}

impl Listener for Tls {
    type Io = tokio_rustls::server::TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, peer) = Listener::accept(&mut self.listener).await;
            if let Ok(stream) = self.tls.accept(stream).await {
                return (stream, peer);
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Listener::local_addr(&self.listener)
    }
}

/// One server-sent event of a Messages stream.
fn event(data: &Value) -> String {
    format!(
        "event: {}\ndata: {data}\n\n",
        data["type"].as_str().unwrap()
    )
}

/// A Messages turn answering `text`.
fn answer(text: &str) -> Vec<String> {
    [
        json!({"type": "message_start", "message": {"id": "msg_1", "type": "message",
            "role": "assistant", "model": MODEL, "content": [], "stop_reason": null,
            "stop_sequence": null, "usage": {"input_tokens": 9, "output_tokens": 0}}}),
        json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": text}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn",
            "stop_sequence": null}, "usage": {"output_tokens": 3}}),
        json!({"type": "message_stop"}),
    ]
    .iter()
    .map(event)
    .collect()
}

/// A fake `MiniMax` answering every request with `events`, recording what each
/// one asked; its base URL and a client trusting only it.
async fn fake(events: Vec<String>) -> (String, reqwest::Client, Arc<Mutex<Vec<Seen>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let app = axum::Router::new().fallback(
        move |method: http::Method,
              uri: http::Uri,
              headers: http::HeaderMap,
              body: bytes::Bytes| {
            let body: Value = serde_json::from_slice(&body).unwrap_or_default();
            recorded.lock().unwrap().push(Seen {
                method: method.to_string(),
                path: uri.path().to_owned(),
                api_key: headers
                    .get("x-api-key")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                model: body.get("model").and_then(Value::as_str).map(str::to_owned),
            });
            let chunks = events.clone().into_iter().map(Ok::<_, Infallible>);
            async move {
                (
                    [(http::header::CONTENT_TYPE, "text/event-stream")],
                    axum::body::Body::from_stream(stream::iter(chunks)),
                )
            }
        },
    );
    let (root, acceptor) = tls::server().unwrap();
    let listener = Tls {
        listener: TcpListener::bind("127.0.0.1:0").await.unwrap(),
        tls: acceptor,
    };
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("https://{}:{}", tls::HOST, address.port());
    (base, tls::client(root, address).unwrap(), seen)
}

// Value: the shortest registered-provider path reaches a real socket on the
// provider's own wire, authorized as the provider expects, and answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_on_minimax_answers_through_the_messages_wire() {
    let (base, client, seen) = fake(answer("toml")).await;
    let models = Models::new().with(providers::minimax().with_base_url(base));
    let model = models
        .connect_with_key("minimax", MODEL, KEY)
        .unwrap()
        .with_transport(Reqwest::with_client(client));
    let messages = [Message::User("Which crate parses TOML in Rust?".into())];
    let request = Request {
        model: "",
        instructions: "Answer in one word.",
        messages: &messages,
        tools: &[],
        hosted: &[],
        cache_key: "",
        max_output_tokens: None,
        single_attempt: false,
    };

    let chunks: Vec<_> = model.stream(request).collect().await;

    let text: String = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            Ok(Chunk::Text {
                kind: TextKind::Answer,
                text,
            }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "toml", "{chunks:?}");
    assert!(matches!(chunks.last(), Some(Ok(Chunk::End(_)))));
    assert_eq!(
        *seen.lock().unwrap(),
        [Seen {
            method: "POST".into(),
            path: "/v1/messages".into(),
            api_key: Some(KEY.into()),
            model: Some(MODEL.into()),
        }]
    );
}
