//! Authoring fixtures send native frames through the production router and connection guard.

mod session;
mod stores;

use af_core::id::Uuid7;
use axum::{Router, body::Body, response::Response};
use http::StatusCode;
use serde_json::{Value, json};

pub(crate) use session::Session;

#[derive(Clone, Debug)]
pub(crate) enum Operation {
    Activity(Uuid7),
    Model(Uuid7),
    Renew(Uuid7),
    Report(Uuid7),
}

impl Operation {
    fn parts(&self) -> (&'static str, af_wire::protocol::StreamKind, &Uuid7) {
        use af_wire::protocol::StreamKind;
        match self {
            Self::Activity(lease) => ("authoring_activity", StreamKind::Work, lease),
            Self::Model(lease) => ("authoring_model", StreamKind::Work, lease),
            Self::Renew(lease) => ("authoring_renew", StreamKind::Control, lease),
            Self::Report(lease) => ("authoring_report", StreamKind::Reports, lease),
        }
    }

    fn reply_kind(&self) -> &'static str {
        match self {
            Self::Activity(_) => "authoring_activity_ack",
            Self::Model(_) => "authoring_model_result",
            Self::Renew(_) => "authoring_renewed",
            Self::Report(_) => "authoring_report_ack",
        }
    }

    fn success(&self) -> StatusCode {
        match self {
            Self::Activity(_) => StatusCode::ACCEPTED,
            Self::Model(_) | Self::Renew(_) | Self::Report(_) => StatusCode::OK,
        }
    }
}

/// The response wrapper lets existing state assertions read the actual native answer body.
/// HTTP authentication refusals are returned unchanged from the opening guard.
pub(crate) async fn send(
    router: &Router,
    operation: &Operation,
    bearer: Option<&str>,
    body: &str,
) -> Response {
    let response = match exchange(router, operation, bearer, body).await {
        Ok(response) => Some(response),
        Err(Failed::Opening(response)) => Some(*response),
        Err(Failed::Closed) => None,
    };
    response.expect("a valid authoring operation receives its native answer")
}

/// Unknown native fields close the session; they no longer have an HTTP operation to answer.
pub(crate) async fn assert_closed(
    router: &Router,
    operation: &Operation,
    bearer: Option<&str>,
    body: &str,
) {
    assert!(matches!(
        exchange(router, operation, bearer, body).await,
        Err(Failed::Closed)
    ));
}

enum Failed {
    Opening(Box<Response>),
    Closed,
}

async fn exchange(
    router: &Router,
    operation: &Operation,
    bearer: Option<&str>,
    body: &str,
) -> Result<Response, Failed> {
    let mut session = Session::open(router, bearer)
        .await
        .map_err(Failed::Opening)?;
    let (kind, stream, lease) = operation.parts();
    let message = json!({"type":kind,"request_id":2,"lease_id":lease,
        "request":serde_json::from_str::<Value>(body).expect("fixture operation JSON")});
    session.write(stream, &message).await;
    let reply = loop {
        let Some(frame) = session.read(stream).await else {
            return Err(Failed::Closed);
        };
        let reply: Value = frame.decode().expect("native answer JSON");
        if reply.get("request_id") == Some(&json!(2)) {
            break reply;
        }
        assert!(
            matches!(
                (*reply.pointer("/type").expect("native response field")).as_str(),
                Some("assignment" | "cancel")
            ),
            "unexpected unsolicited native frame: {reply}"
        );
    };
    assert_eq!(
        (*reply.pointer("/type").expect("native response field")),
        operation.reply_kind()
    );
    assert_eq!(
        (*reply.pointer("/lease_id").expect("native response field")),
        lease.as_str()
    );
    Ok(render(
        operation,
        reply.pointer("/answer").expect("native response field"),
    ))
}

fn render(operation: &Operation, answer: &Value) -> Response {
    let status = answer
        .get("status")
        .and_then(Value::as_str)
        .expect("native answer status");
    assert!(
        matches!(status, "ok" | "refused"),
        "unexpected native status: {status}"
    );
    let (status, body) = if status == "ok" {
        (
            operation.success(),
            answer.get("response").expect("successful response").clone(),
        )
    } else {
        refusal(answer)
    };
    let mut response = Response::new(Body::from(body.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    response
}

fn refusal(answer: &Value) -> (StatusCode, Value) {
    let code = answer
        .get("error_code")
        .and_then(Value::as_str)
        .expect("native refusal code");
    let code = af_core::error_code::REGISTRY
        .iter()
        .find(|entry| entry.as_str() == code)
        .expect("native code is registered");
    let status = StatusCode::from_u16(af_core::problem::Problem::of(*code).status())
        .expect("registered HTTP classification");
    (
        status,
        json!({"error_code":code.as_str(),"detail":answer.get("detail").expect("refusal detail")}),
    )
}
