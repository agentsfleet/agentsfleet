//! The public transport recorder preserves the prepared request and scripted reply.
#![expect(clippy::expect_used, reason = "fixture preconditions fail loudly")]

use super::{CountingMint, RecordingTransport};
use crate::{
    admission::{Draft, Placement},
    egress::Egress,
    transport::Transport as _,
};
use afd_core::clock::{FixedClock, UnixMillis};

// Value: protects=HTTP test evidence records the credentialed request actually prepared;
// fails_when=the recording transport mutates the request or its fixed response;
// why_new=consumer tests use the helper but never independently validate the recorder;
// seam=none; no_claim=real socket transport.
#[tokio::test]
async fn fixed_replies_record_the_exact_prepared_request() {
    let policy = crate::fixture::policy(true);
    let mint = CountingMint::never();
    let clock = FixedClock::at(UnixMillis::EPOCH);
    let mut egress = Egress::new("lease-proof", &policy, &mint, &clock);
    let draft = Draft {
        method: "GET".into(),
        url: "https://demo-grafana.internal/api/health".into(),
        headers: vec![
            (
                "Authorization".into(),
                "Bearer ${secrets.grafana.token}".into(),
            ),
            ("x-request-id".into(), "trace-proof".into()),
        ],
        body: Some("request body".into()),
        placement: Placement::Authorization,
    };
    let outbound = egress
        .prepare(draft)
        .await
        .expect("static credential allowed");
    let (transport, sent) = RecordingTransport::replying(202, "queued");
    assert_eq!(format!("{transport:?}"), "RecordingTransport { .. }");
    let response = transport.send(outbound).await.expect("scripted reply");
    assert_eq!(response.status, 202);
    assert_eq!(response.body, "queued");
    let recorded = sent.try_recv().expect("one request recorded");
    assert_eq!(recorded.method, "GET");
    assert_eq!(recorded.url, "https://demo-grafana.internal/api/health");
    assert_eq!(
        recorded.header("authorization"),
        Some("Bearer glsa_live_token")
    );
    assert_eq!(recorded.header("x-request-id"), Some("trace-proof"));
    assert_eq!(recorded.body.as_deref(), Some("request body"));
    sent.try_recv().expect_err("only one request was sent");
    assert_eq!(mint.asked(), 0);
}
