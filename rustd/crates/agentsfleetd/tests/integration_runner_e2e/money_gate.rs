//! Live daemon coverage for the tenant-credit lease gate.

use super::*;

/// Dimension 2.3 — an exhausted tenant is refused at issue, and nothing is
/// written.
///
/// The credits gate is only reached on the pull path, after the runner proves
/// its capabilities. The event is ended rather than retried forever, while no
/// lease or charge is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_lease_money_gate_refusal() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let degraded = post(&http, &run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(degraded.status().as_u16(), 200);
    assert_eq!(
        field(&json(degraded).await, "lease"),
        &Value::Null,
        "a newly enrolled runner receives no work before proving its capabilities"
    );

    // Drained after the seed, so the wallet row exists and holds zero.
    run.drain_wallet().await;

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities before the money refusal"
    );

    let polled = post(&http, &run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(polled.status().as_u16(), 200);
    assert_eq!(
        field(&json(polled).await, "lease"),
        &json!(null),
        "an exhausted tenant receives no lease"
    );
    assert_eq!(
        lease_rows(&run).await,
        0,
        "the gate refuses before writing a lease row"
    );
    assert_eq!(
        balance(&run).await,
        Some(0),
        "the refused poll charges nothing"
    );
}
