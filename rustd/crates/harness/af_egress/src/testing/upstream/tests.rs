//! The fake upstream outlives a client that never finishes TLS, and once
//! stopped it answers nothing.

use hyper::Method;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::{Reply, Route, Upstream};
use crate::egress::Egress;
use crate::fixture::{GRAFANA, policy};
use crate::policy::{Draft, Placement};
use crate::refusal::Refusal;
use crate::testing::CountingMint;
use crate::transport::Transport as _;
use af_core::clock::FixedClock;

/// A Grafana health read, as the guard prepares it.
async fn health(egress: &mut Egress<'_>) -> crate::Outbound {
    egress
        .prepare(Draft {
            method: "GET".into(),
            url: format!("https://{GRAFANA}/api/health"),
            headers: Vec::new(),
            body: None,
            placement: Placement::Authorization,
        })
        .await
        .expect("the fixture policy allows Grafana")
}

#[tokio::test]
async fn a_failed_handshake_leaves_the_fake_serving_and_stop_closes_it() {
    let mut upstream = Upstream::serve(vec![Route::new(
        GRAFANA,
        Method::GET,
        "/api/health",
        vec![Reply::text(200, "healthy")],
    )])
    .await;
    let network = upstream.network();
    let policy = policy(true);
    let mint = CountingMint::never();
    let clock = FixedClock::at(jiff::Timestamp::UNIX_EPOCH);
    let mut egress = Egress::new("run-upstream", &policy, &mint, &clock);

    // Plain text where a TLS client hello belongs: the fake answers one
    // fatal TLS alert and closes this connection, and only this one.
    let mut plain = tokio::net::TcpStream::connect(upstream.address)
        .await
        .expect("the fake listens");
    plain
        .write_all(b"GET /api/health HTTP/1.1\r\n\r\n")
        .await
        .expect("the bytes are written");
    let mut answered = Vec::new();
    plain
        .read_to_end(&mut answered)
        .await
        .expect("the fake closes the connection");
    // A TLS record of content type 21 (alert) whose level byte is 2 (fatal).
    assert_eq!(
        (answered.first(), answered.get(5)),
        (Some(&21), Some(&2)),
        "a connection that never shook hands is refused: {answered:?}"
    );

    let answered = network
        .send(health(&mut egress).await)
        .await
        .expect("a well-formed request is still answered");
    assert_eq!((answered.status, answered.body.as_str()), (200, "healthy"));
    let seen = upstream.seen();
    assert_eq!(
        seen.iter()
            .map(|request| (request.host.as_str(), request.path.as_str()))
            .collect::<Vec<_>>(),
        vec![(GRAFANA, "/api/health")]
    );

    upstream.stop().await;
    let refused = network
        .send(health(&mut egress).await)
        .await
        .expect_err("a stopped fake answers nothing");
    assert_eq!(refused.refusal(), Some(Refusal::UpstreamUnreachable));
    assert!(upstream.seen().is_empty(), "nothing reached a stopped fake");
}
