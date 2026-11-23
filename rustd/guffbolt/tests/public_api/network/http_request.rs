//! `http_request` over a canned hook: redirects, bodies and refusals.

use super::*;

/// A bodyless GET follows its redirects; the answer names the status, the
/// media type and the hops, then the body.
#[tokio::test]
async fn http_request_get_follows_redirects_and_reads_status_type_and_body() {
    let hook = Canned::ok(Response {
        content_type: Some("application/json".to_owned()),
        followed_redirects: 2,
        ..response(200, r#"{"ok":true}"#)
    });
    let tools = web(&hook, &["http_request"]);

    let output = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/v1", "headers": {"X-B": "2", "Authorization": "Bearer ${secrets.api.token}"}}),
    )
    .await;

    assert_eq!(output.error_code, None);
    assert_eq!(
        output.text,
        "Status: 200\nContent-Type: application/json\nRedirects followed: 2\n\nResponse Body:\n{\"ok\":true}"
    );
    assert_eq!(
        hook.sent(),
        [(
            "http_request".to_owned(),
            Request {
                method: "GET".to_owned(),
                url: "https://api.test/v1".to_owned(),
                headers: vec![
                    (
                        "Authorization".to_owned(),
                        "Bearer ${secrets.api.token}".to_owned()
                    ),
                    ("X-B".to_owned(), "2".to_owned()),
                ],
                body: None,
                placement: Placement::Authorization,
                redirects: Redirects::Follow,
            }
        )],
        "the placeholder reaches the hook unplaced, in the Authorization header"
    );
}

/// A request with a body, or another method, stops at a redirect and reads
/// where it points; a status outside 2xx fails as `upstream_status`.
#[tokio::test]
async fn http_request_with_a_body_stops_at_a_redirect_and_fails_as_upstream_status() {
    let hook = Canned::ok(Response {
        location: Some("https://api.test/elsewhere".to_owned()),
        ..response(302, "")
    });
    let tools = web(&hook, &["http_request"]);

    let posted = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "method": "POST", "body": "{}"}),
    )
    .await;
    let got_with_body = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "body": "x"}),
    )
    .await;
    let lower_get = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "method": "get"}),
    )
    .await;

    assert_eq!(posted.error_code, Some(code::UPSTREAM_STATUS));
    assert_eq!(
        posted.text,
        "[upstream_status] Status: 302\nLocation: https://api.test/elsewhere\n\nResponse Body:\n"
    );
    assert_eq!(got_with_body.error_code, Some(code::UPSTREAM_STATUS));
    assert_eq!(lower_get.error_code, Some(code::UPSTREAM_STATUS));
    let followed: Vec<(String, Redirects)> = hook
        .sent()
        .into_iter()
        .map(|(_tool, request)| (request.method, request.redirects))
        .collect();
    assert_eq!(
        followed,
        [
            ("POST".to_owned(), Redirects::Stop),
            ("GET".to_owned(), Redirects::Stop),
            ("get".to_owned(), Redirects::Follow),
        ]
    );
}

/// A cut body is never handed to the model as if it were whole: the answer
/// says where it was cut and omits the prefix; replaced bytes are named.
#[tokio::test]
async fn http_request_omits_a_truncated_body_and_names_replaced_bytes() {
    let cut = Canned::ok(Response {
        truncated: true,
        lossy: true,
        ..response(200, "half of it")
    });
    let empty_cut = Canned::ok(Response {
        truncated: true,
        ..response(200, "")
    });
    let cut_tools = web(&cut, &["http_request"]);
    let empty_tools = web(&empty_cut, &["http_request"]);

    let read = dispatch(
        &cut_tools,
        "http_request",
        json!({"url": "https://api.test/"}),
    )
    .await;
    let read_empty = dispatch(
        &empty_tools,
        "http_request",
        json!({"url": "https://api.test/"}),
    )
    .await;

    assert_eq!(
        read.text,
        "Status: 200\n[Response truncated at 1048576 bytes]\n[Response contains replaced non-UTF-8 bytes]\n\n\
         Response Body:\n[Incomplete response body omitted; the source was truncated]"
    );
    assert_eq!(
        read_empty.text,
        "Status: 200\n[Response truncated at 1048576 bytes]\n\nResponse Body:\n"
    );
}

#[tokio::test]
async fn http_request_reads_the_hooks_refusal_as_its_code_and_sentence() {
    let hook = Canned::answering(Err(Refused::new(
        code::HOST_NOT_ALLOWED,
        "api.test is not an allowed host",
    )));
    let tools = web(&hook, &["http_request"]);

    let output = dispatch(&tools, "http_request", json!({"url": "https://api.test/"})).await;

    assert_eq!(output.error_code, Some(code::HOST_NOT_ALLOWED));
    assert_eq!(
        output.text,
        "[host_not_allowed] api.test is not an allowed host"
    );
}

#[tokio::test]
async fn http_request_with_arguments_outside_its_schema_sends_nothing() {
    let hook = Canned::ok(response(200, ""));
    let tools = web(&hook, &["http_request"]);

    let unknown = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "verb": "GET"}),
    )
    .await;
    let missing = dispatch(&tools, "http_request", json!({"method": "GET"})).await;
    let mistyped = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://api.test/", "headers": ["a"]}),
    )
    .await;

    for output in [&unknown, &missing, &mistyped] {
        assert_eq!(
            output.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{}",
            output.text
        );
    }
    assert_eq!(missing.text, "[invalid_arguments] missing field `url`");
    assert!(
        unknown
            .text
            .starts_with("[invalid_arguments] unknown field `verb`"),
        "{}",
        unknown.text
    );
    assert_eq!(hook.sent(), Vec::new(), "no request reached the hook");
}
