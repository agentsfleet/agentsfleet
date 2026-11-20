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
    assert_eq!(hook.masked.load(Ordering::SeqCst), 1);
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
                redirects: Redirects::Stop,
            }
        )],
        "a fetch carries no header, no body and no placeholder, and follows nothing"
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
        0,
        "plain text is not masked again"
    );
}

#[tokio::test]
async fn web_fetch_reads_at_most_fifty_thousand_chars_when_no_limit_is_named() {
    let long = "a".repeat(50_001);
    let hook = Canned::ok(response(200, &long));
    let tools = web(&hook, &["web_fetch"]);

    let output = dispatch(&tools, "web_fetch", json!({"url": "https://docs.test/"})).await;

    assert!(
        output
            .text
            .ends_with("\n\n[Content truncated at 50000 chars, total 50001 chars]"),
        "{}",
        output
            .text
            .get(output.text.len().saturating_sub(80)..)
            .unwrap_or_default()
    );
    assert_eq!(
        output.text.split("\n\n").next().map(str::len),
        Some(50_000),
        "the kept text is the first fifty thousand characters"
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
