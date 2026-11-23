//! The bounds a connected model holds: only the reply bound its embedder set,
//! no redirect, no wait past the ceiling, and the idle deadline.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A chat completion's first text, its usage and its end, as a provider
/// streams them.
const TEXT: &str = "data: {\"id\":\"r\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi 🌿\"},\"finish_reason\":null}]}\n\n";
const USAGE: &str = "data: {\"id\":\"r\",\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n";
const STOP: &str = "data: {\"id\":\"r\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
/// 32 MiB, the largest reply bound an embedder of this crate is known to
/// set; this crate sets none.
const FRAME: usize = 32 * 1024 * 1024;
const MIB: usize = 1024 * 1024;
/// One event's worth of bytes the reader passes over: an event-stream
/// comment, which carries nothing visible and no spend.
const PADDING: usize = 64 * 1024;

/// A comment line of [`PADDING`] bytes.
fn padding() -> Bytes {
    let mut line = vec![b':'];
    line.resize(PADDING - 1, b' ');
    line.push(b'\n');
    Bytes::from(line)
}

/// Padding past `bytes`, then the answer.
fn reply_past(bytes: usize) -> Vec<Bytes> {
    let mut chunks: Vec<_> = std::iter::repeat_with(padding)
        .take(bytes / PADDING + 1)
        .collect();
    chunks.extend([TEXT, USAGE, STOP].map(|event| Bytes::from_static(event.as_bytes())));
    chunks
}

/// A chat provider on a loopback port answering every request with
/// `chunks`, counting the requests.
async fn chat_provider(chunks: Vec<Bytes>) -> (SocketAddr, Arc<AtomicUsize>) {
    let sends = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&sends);
    let app = axum::Router::new().fallback(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        let body = stream::iter(chunks.clone().into_iter().map(Ok::<_, Infallible>));
        async move {
            (
                [(http::header::CONTENT_TYPE, "text/event-stream")],
                axum::body::Body::from_stream(body),
            )
        }
    });
    (serve(app).await, sends)
}

async fn serve(app: axum::Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    address
}

/// `MODEL` over `api` at `address`, through the default transport.
fn connected(api: Api, address: SocketAddr) -> Connected {
    let route = Route::new(api, &format!("http://{address}/v1"), None, 37, 128_000).unwrap();
    Connected::new(
        route,
        MODEL,
        KEY,
        crate::wire::transport::Reqwest::new().unwrap(),
    )
}

fn asked(messages: &[Message]) -> Request<'_> {
    Request {
        single_attempt: false,
        ..request(messages)
    }
}

// Value: guffbolt-ai has no reply bound of its own, so a reply past the
// largest bound an embedder is known to set is read whole.
#[tokio::test(flavor = "multi_thread")]
async fn with_no_bound_a_reply_past_32_mib_is_read() {
    let (address, sends) = chat_provider(reply_past(FRAME + MIB)).await;
    let model = connected(Api::Chat, address);
    let messages = [Message::User("hello".into())];

    let chunks: Vec<_> = model.stream(asked(&messages)).collect().await;

    assert!(chunks.iter().all(Result::is_ok), "{chunks:?}");
    assert_eq!(answered(&chunks), "Hi 🌿");
    assert!(matches!(chunks.last(), Some(Ok(Chunk::End(_)))));
    assert_eq!(sends.load(Ordering::SeqCst), 1);
}

// Value: the bound an embedder sets ends the read at its first byte past,
// and a reply that long is not opened again.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_past_the_bound_the_embedder_set_is_refused() {
    let (address, sends) = chat_provider(reply_past(MIB)).await;
    let model = connected(Api::Chat, address).with_reply_bound(MIB);
    let messages = [Message::User("hello".into())];

    let chunks: Vec<_> = model.stream(asked(&messages)).collect().await;

    let refused = failure(chunks);
    assert!(refused.is_reply_too_large(), "{refused:?}");
    assert_eq!(sends.load(Ordering::SeqCst), 1, "not reopened");
}

// Value: the key reaches only the host the route names; a redirect would
// carry it to another.
#[tokio::test(flavor = "multi_thread")]
async fn a_redirect_is_not_followed_with_the_key() {
    let elsewhere = Arc::new(AtomicUsize::new(0));
    let reached = Arc::clone(&elsewhere);
    let second = serve(axum::Router::new().fallback(move || {
        reached.fetch_add(1, Ordering::SeqCst);
        async { http::StatusCode::OK }
    }))
    .await;
    let location = format!("http://{second}/v1/messages");
    let first = serve(axum::Router::new().fallback(move || {
        let location = location.clone();
        async move {
            (
                http::StatusCode::TEMPORARY_REDIRECT,
                [(http::header::LOCATION, location)],
            )
        }
    }))
    .await;
    let model = connected(Api::Messages, first);
    let messages = [Message::User("hello".into())];

    let chunks: Vec<_> = model.stream(asked(&messages)).collect().await;

    let failed = failure(chunks);
    assert_eq!(failed.status(), Some(307), "{failed:?}");
    assert_eq!(elsewhere.load(Ordering::SeqCst), 0);
}

/// A provider answering every send with `status` and `retry_after`, and its
/// sends counted.
struct Limited {
    sends: AtomicUsize,
    status: http::StatusCode,
    retry_after: &'static str,
}

impl Send for Limited {
    fn send(&self, _: http::Request<Bytes>) -> Sent {
        self.sends.fetch_add(1, Ordering::SeqCst);
        let response = http::Response::builder()
            .status(self.status)
            .header(http::header::RETRY_AFTER, self.retry_after)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(stream::empty().boxed())
            .unwrap();
        Box::pin(async move { Ok(response) })
    }
}

// Value: a provider asking for a wait past 60 s is not slept on; the turn
// ends with its status instead.
#[tokio::test(start_paused = true)]
async fn a_retry_after_past_sixty_seconds_ends_the_turn() {
    let provider = Arc::new(Limited {
        sends: AtomicUsize::new(0),
        status: http::StatusCode::TOO_MANY_REQUESTS,
        retry_after: "120",
    });
    let route = Route::new(Api::Chat, "https://limited.example/v1", None, 37, 128_000).unwrap();
    let model = Connected::new(route, MODEL, KEY, Arc::clone(&provider));
    let messages = [Message::User("hello".into())];
    let started = tokio::time::Instant::now();

    let chunks: Vec<_> = model.stream(asked(&messages)).collect().await;

    let limited = failure(chunks);
    assert_eq!(limited.status(), Some(429), "{limited:?}");
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert!(started.elapsed() < Duration::from_secs(60), "nothing slept");
}

/// A provider that sends its headers and then nothing.
struct Silent;

impl Send for Silent {
    fn send(&self, _: http::Request<Bytes>) -> Sent {
        let response = http::Response::builder()
            .header(http::header::CONTENT_TYPE, "text/event-stream")
            .body(stream::pending().boxed())
            .unwrap();
        Box::pin(async move { Ok(response) })
    }
}

// Value: a provider that stops sending cannot hold a turn past the idle
// timeout, 300 s by default.
#[tokio::test(start_paused = true)]
async fn a_silent_provider_ends_at_the_idle_timeout() {
    let route = Route::new(Api::Chat, "https://silent.example/v1", None, 37, 128_000).unwrap();
    let recorder = Recorder::new();
    let model =
        Connected::new(route, MODEL, KEY, Silent).with_observer(Arc::<Recorder>::clone(&recorder));
    let messages = [Message::User("hello".into())];
    let started = tokio::time::Instant::now();

    let chunks: Vec<_> = model.stream(request(&messages)).collect().await;

    let failed = failure(chunks);
    assert_eq!(started.elapsed(), Duration::from_secs(300), "{failed:?}");
    let ended: Vec<_> = recorder.ended().iter().map(|turn| turn.cause).collect();
    assert_eq!(ended, [Some(FailureCause::Deadline)]);
}
