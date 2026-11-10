//! Live daemon coverage for the tenant-credit lease gate.

use super::*;

/// Dimension 2.3 — an exhausted tenant is refused at issue, and nothing is
/// written.
///
/// The credits gate is reached when a native session proves
/// its capabilities. The event is ended rather than retried forever, while no
/// lease or charge is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_lease_money_gate_refusal() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    assert!(
        !wait_without_offer(&run, || async { false }).await,
        "a newly enrolled runner receives no work before proving its capabilities"
    );
    assert_eq!(lease_rows(&run).await, 0);

    // Drained after the seed, so the wallet row exists and holds zero.
    run.drain_wallet().await;

    let beat = crate::wire::heartbeat(&run, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );

    // Durable refusal proves the claim ran; silence alone would not.
    let refused = wait_without_offer(&run, || async {
        event_column(&run, &run.event_id, "status").await.as_deref() == Some("gate_blocked")
    })
    .await;
    assert!(
        refused,
        "an exhausted tenant's event is ended at the money gate"
    );
    assert_eq!(
        lease_rows(&run).await,
        0,
        "the gate refuses before writing a lease row"
    );
    assert_eq!(
        balance(&run).await,
        Some(0),
        "the refused claim charges nothing"
    );

    // Stop background tasks before releasing the scenario's placement guard.
    supervisor.shutdown().await;
    run.cleanup().await;
}
