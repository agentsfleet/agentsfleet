#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::sync::Arc;

use afr_egress::fixture::{GITHUB, GRAFANA};
use afr_egress::testing::{RecordingTransport, inbound, redirected};
use afr_egress::{Outbound, Transport};
use serde_json::json;

use super::HttpRequest;
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Run, call};

const INITIAL: &str = "https://api.github.com/repos/acme/widgets/actions/jobs/7/logs";
const AUTH: &str = "Bearer ${secrets.github.token}";
const LOG: &str = "2026-10-30T08:34:00Z assertion failed: decoded title";

#[tokio::test]
async fn should_read_text_through_admitted_redirects() {
    for status in [301, 302, 303, 307, 308] {
        let run = Run::new(true);
        let (transport, sent) = RecordingTransport::answering(move |request| {
            if request.url().path().ends_with("/logs") {
                Ok(redirected(
                    status,
                    request.url(),
                    "/repos/acme/widgets/download?sig=private-signature",
                ))
            } else {
                Ok(inbound(200, LOG))
            }
        });
        let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
        let output = call(
            tool.as_ref(),
            &run.lease(),
            json!({"url": INITIAL, "headers": {"Authorization": AUTH}}),
        )
        .await;
        assert_eq!(output.error_code, None, "{output:?}");
        assert!(output.text.contains(LOG));
        assert!(output.text.contains("Redirects followed: 1"));
        assert!(!output.text.contains("private-signature"));
        assert_eq!(sent.try_iter().count(), 2);
        assert_eq!(run.mint.asked(), 1);
    }
}

#[tokio::test]
async fn should_strip_headers_across_redirect_origins() {
    let run = Run::new(true);
    let (transport, sent) = RecordingTransport::answering(|request| {
        let target = match (request.host(), request.url().path()) {
            (GITHUB, path) if path.ends_with("/logs") => {
                Some(format!("https://{GRAFANA}/download"))
            }
            (GRAFANA, _) => Some("https://api.github.com/repos/acme/widgets/returned".to_owned()),
            _ => None,
        };
        Ok(target.map_or_else(
            || inbound(200, LOG),
            |target| redirected(302, request.url(), &target),
        ))
    });
    let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
    let output = call(
        tool.as_ref(),
        &run.lease(),
        json!({"url": INITIAL,
        "headers": {"Authorization": AUTH, "X-Caller": "drop me", "Accept": "application/json"}}),
    )
    .await;
    assert_eq!(output.error_code, None, "{output:?}");
    let requests = sent.try_iter().collect::<Vec<_>>();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .first()
            .expect("initial")
            .header("authorization")
            .is_some()
    );
    for request in requests.iter().skip(1) {
        assert!(request.headers.is_empty(), "{request:?}");
    }
    assert_eq!(run.mint.asked(), 1);
}

#[tokio::test]
async fn should_refuse_redirect_before_transmission() {
    for target in [
        "https://unlisted.example/logs?sig=private-signature",
        "http://api.github.com/download",
        "https://127.0.0.1/private",
        "https://user:password@api.github.com/private",
    ] {
        let run = Run::new(true);
        let (transport, sent) = RecordingTransport::answering(move |request| {
            Ok(redirected(302, request.url(), target))
        });
        let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
        let output = call(tool.as_ref(), &run.lease(), json!({"url": INITIAL})).await;
        assert!(output.error_code.is_some());
        assert_eq!(sent.try_iter().count(), 1);
        assert!(!output.text.contains("private-signature"));
        assert!(!output.text.contains("password"));
        assert_eq!(run.mint.asked(), 0);
    }
}

#[tokio::test]
async fn should_keep_unaffected_clients_single_hop() {
    for (method, body) in [
        ("HEAD", None),
        ("GET", Some("body")),
        ("POST", Some("body")),
    ] {
        let run = Run::new(false);
        let (transport, sent) =
            RecordingTransport::answering(|request| Ok(redirected(302, request.url(), "/another")));
        let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
        let mut arguments = json!({"url": format!("https://{GRAFANA}/logs"), "method": method});
        if let Some(body) = body {
            arguments["body"] = json!(body);
        }
        let output = call(tool.as_ref(), &run.lease(), arguments).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamStatus));
        assert_eq!(sent.try_iter().count(), 1);
    }
}

#[derive(Debug)]
struct Never;

#[async_trait::async_trait]
impl Transport for Never {
    async fn send(&self, _request: Outbound) -> afr_egress::Result<afr_egress::Inbound> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn should_bound_the_whole_http_read_deadline() {
    let run = Run::new(true);
    let tool = Typed::boxed(HttpRequest::new(Arc::new(Never)));
    let lease = run.lease();
    let output = call(tool.as_ref(), &lease, json!({"url": INITIAL})).await;
    assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamUnreachable));
    assert!(output.text.contains("deadline exceeded"));
    assert!(lease.egress.try_lock().is_ok());
}

#[tokio::test]
async fn should_follow_mixed_case_get_and_mask_success_and_error_echoes() {
    const TARGET: &str = "https://demo-grafana.internal/download?sig=a*b~c&job=9";
    for (method, status) in [("get", 200), ("Get", 403), ("GET", 500)] {
        let capture = afd_core::test_util::trace::Capture::install();
        let run = Run::new(true);
        let (transport, sent) = RecordingTransport::answering(move |request| {
            if request.host() == GITHUB {
                Ok(redirected(302, request.url(), TARGET))
            } else {
                let mut response = inbound(
                    status,
                    &format!(
                        "{LOG}\necho={TARGET}\ncomponent=a%2Ab~c\njson={}\njob=9",
                        TARGET.replace('/', "\\/")
                    ),
                );
                response.content_type = Some("text/plain; debug=a%2ab~c".to_owned());
                Ok(response)
            }
        });
        let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
        let output = call(
            tool.as_ref(),
            &run.lease(),
            json!({"url":INITIAL,"method":method}),
        )
        .await;
        assert_eq!(
            output.error_code,
            (status != 200).then_some(ToolErrorCode::UpstreamStatus)
        );
        assert!(output.text.contains(LOG) && output.text.contains("job=9"));
        for private in [TARGET, "a*b~c", "a%2Ab~c", "a%2ab~c", "/download?sig="] {
            assert!(!output.text.contains(private), "protected text survived");
            assert!(!format!("{:?}", capture.events()).contains(private));
        }
        assert_eq!(sent.try_iter().count(), 2);
        let _started = capture.only("http_redirect_started");
        let ending = if status == 200 {
            "http_redirect_completed"
        } else {
            "http_redirect_failed"
        };
        assert_eq!(capture.only(ending).field("hop_count"), Some("1"));
    }
}

#[tokio::test]
async fn should_retain_safe_context_when_redirects_stop() {
    for (case, reason, sends) in [
        (0, "valid bounded Location", 1),
        (1, "loop refused", 1),
        (2, "limit exceeded", 4),
    ] {
        let run = Run::new(true);
        let (transport, sent) = RecordingTransport::answering(move |request| {
            if case == 0 {
                return Ok(inbound(302, ""));
            }
            let target = if case == 1 {
                INITIAL.to_owned()
            } else {
                format!(
                    "https://{GRAFANA}{}/next?sig=private-signature",
                    request.url().path()
                )
            };
            Ok(redirected(302, request.url(), &target))
        });
        let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
        let output = call(tool.as_ref(), &run.lease(), json!({"url": INITIAL})).await;
        let code = if case == 0 {
            ToolErrorCode::InvalidArguments
        } else {
            ToolErrorCode::UpstreamUnreachable
        };
        assert_eq!(output.error_code, Some(code));
        assert!(output.text.contains("Status: 302") && output.text.contains(reason));
        assert!(output.text.contains("Redirects followed:"));
        assert!(!output.text.contains("private-signature"));
        assert_eq!(sent.try_iter().count(), sends);
    }
}

#[derive(Debug)]
struct SlowHops;

#[async_trait::async_trait]
impl Transport for SlowHops {
    async fn send(&self, request: Outbound) -> afr_egress::Result<afr_egress::Inbound> {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        Ok(redirected(
            302,
            request.url(),
            &format!("{}/next", request.url().path()),
        ))
    }
}

#[tokio::test(start_paused = true)]
async fn should_share_one_deadline_across_redirect_hops() {
    let run = Run::new(true);
    let tool = Typed::boxed(HttpRequest::new(Arc::new(SlowHops)));
    let began = tokio::time::Instant::now();
    let output = call(tool.as_ref(), &run.lease(), json!({"url": INITIAL})).await;
    assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamUnreachable));
    assert!(output.text.contains("deadline exceeded"));
    assert_eq!(began.elapsed(), afr_egress::REQUEST_TIMEOUT);
}
