//! What a request that leaves reads back: a public redirect followed, a body
//! cut at its limit, and a dropped connection.

use super::*;

#[tokio::test]
async fn local_outbound_follows_a_redirect_to_a_public_host() {
    let arrived = "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\narrived";
    let onward = redirect("https://public.test/next");
    let (address, root, seen) = fake(&[("/start", onward.as_str()), ("/next", arrived)]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let answered = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        answered,
        Ok(Response {
            status: 200,
            body: "arrived".to_owned(),
            followed_redirects: 1,
            ..Response::default()
        })
    );
    assert_eq!(requests(&seen), ["GET /start", "GET /next"]);
}

/// A body past the hook's limit is cut there, and the answer's media type
/// comes back with it.
#[tokio::test]
async fn local_outbound_cuts_a_body_at_its_limit_and_reads_the_media_type() {
    let body = "a".repeat(RESPONSE_MAX_BYTES + 1);
    let large = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (address, root, _seen) = fake(&[("/large", large.as_str())]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let answered = sent(&outbound, "https://public.test/large")
        .await
        .expect("the server answers");

    assert_eq!(answered.content_type.as_deref(), Some("text/plain"));
    assert!(answered.truncated, "the body ran past the limit");
    assert_eq!(answered.body.len(), RESPONSE_MAX_BYTES);
}

/// A host that takes the connection and drops it unanswered reads as
/// unreachable.
#[tokio::test]
async fn local_outbound_reads_a_dropped_connection_as_unreachable() {
    let dropping = Counted::listen().await;
    let (root, _acceptor) = authority();
    let outbound = LocalOutbound::routed(
        Arc::new(FixedLookup::default()),
        PUBLIC,
        dropping.address,
        root,
    );

    let refused = sent(&outbound, "https://public.test/")
        .await
        .expect_err("nothing answered");

    assert_eq!(
        refused.code,
        code::UPSTREAM_UNREACHABLE,
        "{}",
        refused.detail
    );
    assert!(dropping.connections() >= 1, "the host was reached");
}
