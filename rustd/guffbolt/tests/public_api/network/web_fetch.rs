//! `web_fetch` over a canned hook: a page read as text, cut and masked.

use super::*;

/// A page is read as its text, and masked again once decoded: an
/// entity-encoded secret only reads as the secret after the markup is gone.
#[tokio::test]
async fn web_fetch_reads_a_page_as_text_and_masks_what_decoding_reveals() {
    let hook = Canned::ok(Response {
        content_type: Some("text/html; charset=utf-8".to_owned()),
        ..response(
            200,
            "<html><body><h1>Docs</h1><p>key tok&#45;123 here</p></body></html>",
        )
    });
    let tools = web(&hook, &["web_fetch"]);

    let output = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/page"}),
    )
    .await;

    assert_eq!(output.error_code, None);
    assert!(output.text.contains("Docs"), "{}", output.text);
    assert!(output.text.contains("key [masked] here"), "{}", output.text);
    assert!(
        !output.text.contains("tok-123") && !output.text.contains("<p>"),
        "{}",
        output.text
    );
    assert_eq!(hook.masked.load(Ordering::SeqCst), 2);
    assert_eq!(
        hook.sent(),
        [(
            "web_fetch".to_owned(),
            Request {
                method: "GET".to_owned(),
                url: "https://docs.test/page".to_owned(),
                headers: Vec::new(),
                body: None,
                placement: Placement::Nowhere,
                redirects: Redirects::Follow,
            }
        )],
        "a fetch carries no header, no body and no placeholder, and re-admits each redirect"
    );
}

/// A body that is not a page is read as it came, unmasked again, and cut at
/// `max_chars` characters, never inside one.
#[tokio::test]
async fn web_fetch_cuts_a_plain_body_at_max_chars_by_character() {
    let hook = Canned::ok(Response {
        content_type: Some("text/plain".to_owned()),
        ..response(200, "h\u{e9}llo w\u{f6}rld")
    });
    let tools = web(&hook, &["web_fetch"]);

    let cut = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 4}),
    )
    .await;
    let whole = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 11}),
    )
    .await;
    let none = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": 0}),
    )
    .await;

    assert_eq!(
        cut.text,
        "h\u{e9}ll\n\n[Content truncated at 4 chars, total 11 chars]"
    );
    assert_eq!(whole.text, "h\u{e9}llo w\u{f6}rld");
    assert_eq!(
        none.text,
        "\n\n[Content truncated at 0 chars, total 11 chars]"
    );
    assert_eq!(
        hook.masked.load(Ordering::SeqCst),
        3,
        "each requested URL is checked before it becomes a citation"
    );
}

#[tokio::test]
async fn web_fetch_fails_on_a_status_outside_2xx_and_on_the_hooks_refusal() {
    let missing = Canned::ok(response(404, "not here"));
    let refused = Canned::answering(Err(Refused::new(
        code::HTTPS_REQUIRED,
        "http://docs.test/ is not https",
    )));
    let missing_tools = web(&missing, &["web_fetch"]);
    let refused_tools = web(&refused, &["web_fetch"]);

    let not_found = dispatch(
        &missing_tools,
        "web_fetch",
        json!({"url": "https://docs.test/"}),
    )
    .await;
    let not_https = dispatch(
        &refused_tools,
        "web_fetch",
        json!({"url": "http://docs.test/"}),
    )
    .await;
    let bad = dispatch(
        &missing_tools,
        "web_fetch",
        json!({"url": "https://docs.test/", "max_chars": -1}),
    )
    .await;

    assert_eq!(
        (not_found.error_code, not_found.text.as_str()),
        (Some(code::UPSTREAM_STATUS), "[upstream_status] not here")
    );
    assert_eq!(
        not_https.text,
        "[https_required] http://docs.test/ is not https"
    );
    assert_eq!(bad.error_code, Some(code::INVALID_ARGUMENTS));
}

/// A hook that keeps the trait's default lookup looks names up as this host
/// does: `localhost` resolves without leaving the machine.
#[tokio::test]
async fn an_outbound_hook_looks_names_up_through_this_hosts_resolver_by_default() {
    let hook = Canned::ok(response(200, ""));
    let name: Name = "localhost".parse().expect("a name");

    let addresses: Vec<SocketAddr> = hook
        .lookup()
        .resolve(name)
        .await
        .expect("localhost resolves")
        .collect();

    assert_ne!(addresses, Vec::<SocketAddr>::new());
    assert!(
        addresses.iter().all(|address| address.ip().is_loopback()),
        "{addresses:?}"
    );
}

/// Unrenderable markup is refused so scripts never surface as fallback text.
#[tokio::test]
async fn web_fetch_refuses_a_page_too_deep_to_render() {
    let html = format!(
        "<html><body>{}quoted</body></html>",
        "<blockquote>".repeat(400)
    );
    let hook = Canned::ok(Response {
        content_type: Some("text/html".to_owned()),
        ..response(200, &html)
    });
    let tools = web(&hook, &["web_fetch"]);

    let output = dispatch(
        &tools,
        "web_fetch",
        json!({"url": "https://docs.test/deep"}),
    )
    .await;

    assert_eq!(output.error_code, Some(code::UNSUPPORTED_MEDIA));
    assert!(!output.text.contains("<blockquote>"));
}

#[tokio::test]
async fn a_fetched_page_comes_back_as_markdown_with_its_json_ld() {
    let raw = r#"<h2>A guide</h2><ul><li>harbour</li></ul><a href="https://docs.test/a">text</a><script>hidden()</script><script type="application/ld+json">{"place":"A & B"}</script>"#;
    let hook = Canned::ok(Response {
        content_type: Some("text/html".into()),
        ..response(200, raw)
    });
    let output = dispatch(
        &web(&hook, &["web_fetch"]),
        "web_fetch",
        json!({"url":"https://docs.test/"}),
    )
    .await;
    assert_eq!(output.error_code, None);
    for fragment in [
        "## A guide",
        "* harbour",
        "[text][1]",
        "## JSON-LD",
        r#"{"place":"A & B"}"#,
    ] {
        assert_eq!(output.text.matches(fragment).count(), 1, "{}", output.text);
    }
    assert!(!output.text.contains("hidden()"));
}

#[tokio::test]
async fn a_fetch_without_max_chars_stops_at_the_output_budget() {
    let hook = Canned::ok(response(200, &"x".repeat(50_000)));
    let output = dispatch(
        &web(&hook, &["web_fetch"]),
        "web_fetch",
        json!({"url":"https://docs.test/"}),
    )
    .await;
    assert_eq!(output.text.matches('x').count(), 40_000);
    assert!(output.text.ends_with("total 50000 chars]"));
}

#[tokio::test]
async fn a_binary_body_is_refused_with_its_type() {
    let hook = Canned::ok(Response {
        content_type: Some("application/pdf".into()),
        ..response(200, "%PDF private bytes")
    });
    let output = dispatch(
        &web(&hook, &["web_fetch"]),
        "web_fetch",
        json!({"url":"https://docs.test/a.pdf"}),
    )
    .await;
    assert_eq!(output.text, "[unsupported_media] application/pdf");
    assert_eq!(output.sources(), &[]);
}

#[tokio::test]
async fn a_fetch_redacts_the_requested_link_before_truncating_the_page() {
    let url = "https://docs.test/page?signature=private-value";
    let hook = Canned::ok(response(200, &format!("Read {url} for details.")));
    let output = dispatch(
        &web(&hook, &["web_fetch"]),
        "web_fetch",
        json!({"url":url,"max_chars":37}),
    )
    .await;
    assert!(!output.text.contains("signature"));
    assert!(output.text.contains("https://docs.test/page"));
    assert_eq!(
        output.sources().first().map(|source| source.url.as_str()),
        Some("https://docs.test/page")
    );
}

#[tokio::test]
async fn a_requested_url_containing_a_minted_token_is_never_a_citation() {
    let url = "https://docs.test/tok-123?signature=private-value";
    let hook = Canned::ok(response(200, &format!("Read {url}")));
    let output = dispatch(&web(&hook, &["web_fetch"]), "web_fetch", json!({"url":url})).await;
    assert_eq!(output.sources(), &[]);
    assert_eq!(output.text, "Read [redacted URL]");
}

#[tokio::test]
async fn a_binary_page_without_a_content_type_uses_the_generic_media_name() {
    let hook = Canned::ok(response(200, "raw\0body"));
    let output = dispatch(
        &web(&hook, &["web_fetch"]),
        "web_fetch",
        json!({"url":"https://docs.test/download"}),
    )
    .await;
    assert_eq!(output.error_code, Some(code::UNSUPPORTED_MEDIA));
    assert_eq!(output.text, "[unsupported_media] application/octet-stream");
    assert_eq!(output.sources(), []);
}

#[tokio::test]
async fn a_page_reader_cannot_publish_credentials_from_the_requested_url() {
    let url = "https://user:private@docs.test/page";
    let hook = Canned::ok(response(200, &format!("Read {url}")));
    let output = dispatch(&web(&hook, &["web_fetch"]), "web_fetch", json!({"url":url})).await;
    assert_eq!(output.error_code, None);
    assert_eq!(output.text, "Read [redacted URL]");
    assert_eq!(output.sources(), []);
}
