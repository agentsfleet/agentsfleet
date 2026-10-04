use std::sync::Arc;

use afr_egress::testing::{RecordingTransport, Sent, inbound};
use serde_json::{Value, json};

use super::{DEFAULT_MAX_CHARS, WebFetch};
use crate::egress::SharedTransport;
use crate::handler::Typed;
use crate::http_request::HttpRequest;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{MINTED, Run, call, replying};

const PAGE: &str = "https://demo.es.example/docs";

async fn fetch(transport: SharedTransport, arguments: Value) -> ToolOutput {
    let run = Run::new(false);
    let tool = Typed::boxed(WebFetch::new(transport));
    let mut lease = run.lease();
    call(tool.as_ref(), &mut lease, arguments).await
}

async fn fetched(arguments: Value) -> (ToolOutput, Vec<Sent>) {
    let (transport, sent) = replying(200, "plain text");
    let output = fetch(transport, arguments).await;
    (output, sent.try_iter().collect())
}

#[tokio::test]
async fn test_web_fetch_is_get_only_and_credential_free() {
    for refused in [
        json!({"url": PAGE, "method": "POST"}),
        json!({"url": PAGE, "headers": {"Authorization": "${secrets.grafana.token}"}}),
    ] {
        let (output, sent) = fetched(refused).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
        assert_eq!(sent, Vec::new());
    }

    let (output, sent) = fetched(json!({"url": "https://${secrets.grafana.host}/"})).await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::CredentialPlacementNotAllowed)
    );
    assert_eq!(sent, Vec::new());

    let (output, sent) = fetched(json!({"url": PAGE})).await;
    assert_eq!(output.text, "plain text");
    assert_eq!(sent.len(), 1);
    assert!(sent.iter().all(|request| request.method == "GET"
        && request.headers.is_empty()
        && request.body.is_none()));

    let long = "x".repeat(2 << 20);
    let (transport, _sent) = replying(200, &long);
    let cut = fetch(transport, json!({"url": PAGE})).await;
    assert_eq!(
        cut.text,
        format!(
            "{}\n\n[Content truncated at {DEFAULT_MAX_CHARS} chars, total {} chars]",
            "x".repeat(DEFAULT_MAX_CHARS),
            2 << 20
        )
    );
}

#[tokio::test]
async fn should_read_a_page_as_its_text() {
    let (transport, _sent) = RecordingTransport::answering(|_outbound| {
        let mut page = inbound(
            200,
            "<html><head><script>steal()</script><style>p{}</style></head>\
             <body><h1>Runbook</h1><p>Restart the worker.</p></body></html>",
        );
        page.content_type = Some("text/html; charset=utf-8".to_owned());
        Ok(page)
    });

    let output = fetch(Arc::new(transport), json!({"url": PAGE, "max_chars": 500})).await;

    assert!(output.text.contains("Runbook"), "{}", output.text);
    assert!(
        output.text.contains("Restart the worker."),
        "{}",
        output.text
    );
    assert!(!output.text.contains('<'), "{}", output.text);
    assert!(!output.text.contains("steal()"), "{}", output.text);
}

#[tokio::test]
async fn should_mask_a_minted_token_a_page_spells_in_entities() {
    let run = Run::new(false);
    let mut lease = run.lease();
    let (minting, _sent) = replying(200, "ok");
    let request = Typed::boxed(HttpRequest::new(minting));
    let read = json!({
        "url": "https://api.github.com/repos/acme/widgets/",
        "headers": {"Authorization": "Bearer ${secrets.github.token}"},
    });
    assert_eq!(
        call(request.as_ref(), &mut lease, read).await.error_code,
        None
    );
    let (page, _sent) = RecordingTransport::answering(|_outbound| {
        let mut page = inbound(200, &format!("<p>&#103;{}</p>", &MINTED[1..]));
        page.content_type = Some("text/html".to_owned());
        Ok(page)
    });
    let fetch = Typed::boxed(WebFetch::new(Arc::new(page)));

    let output = call(fetch.as_ref(), &mut lease, json!({"url": PAGE})).await;

    assert!(
        output.text.contains("«secret:github.token»"),
        "{}",
        output.text
    );
    assert!(!output.text.contains(MINTED), "{}", output.text);
}
