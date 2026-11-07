//! An OTLP/HTTP receiver standing in for the runner collector, and the
//! readers that pull spans back out of what it was sent.

use std::sync::{Arc, Mutex, PoisonError};

use super::TRACES;

/// One request a collector was sent.
#[derive(Debug, Clone)]
pub(super) struct Posted {
    pub(super) path: String,
    pub(super) authorized: bool,
    pub(super) body: serde_json::Value,
}

/// An OTLP/HTTP receiver standing in for the runner collector, and what it
/// was sent.
pub(super) async fn collector() -> (String, Arc<Mutex<Vec<Posted>>>) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&received);
    let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
        let kept = Arc::clone(&kept);
        async move {
            let path = request.uri().path().to_owned();
            let authorized = request.headers().contains_key(http_authorization());
            let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
                .await
                .unwrap_or_default();
            let body = serde_json::from_slice(&bytes).unwrap_or_default();
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Posted {
                    path,
                    authorized,
                    body,
                });
            ""
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), received)
}

/// The header a credential would ride in.
fn http_authorization() -> axum::http::HeaderName {
    axum::http::header::AUTHORIZATION
}

/// One span an OTLP/JSON trace export carried.
#[derive(Debug)]
pub(super) struct Exported {
    pub(super) name: String,
    pub(super) trace: String,
    attributes: Vec<(String, String)>,
}

impl Exported {
    /// The span `span` encodes.
    fn of(span: &serde_json::Value) -> Self {
        let text = |key: &str| {
            span.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let attributes = array(span, "attributes")
            .iter()
            .map(|attribute| {
                let key = attribute.get("key").and_then(serde_json::Value::as_str);
                let value = attribute
                    .pointer("/value/stringValue")
                    .and_then(serde_json::Value::as_str);
                (
                    key.unwrap_or_default().to_owned(),
                    value.unwrap_or_default().to_owned(),
                )
            })
            .collect();
        Self {
            name: text("name"),
            trace: text("traceId"),
            attributes,
        }
    }

    /// The string value of attribute `key`.
    pub(super) fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _value)| name == key)
            .map(|(_name, value)| value.as_str())
    }
}

/// The array under `key`, or none.
fn array(value: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Every span the trace exports carried.
pub(super) fn spans(posted: &[Posted]) -> Vec<Exported> {
    posted
        .iter()
        .filter(|post| post.path == TRACES)
        .flat_map(|export| array(&export.body, "resourceSpans"))
        .flat_map(|resource| array(&resource, "scopeSpans"))
        .flat_map(|scope| array(&scope, "spans"))
        .map(|span| Exported::of(&span))
        .collect()
}
