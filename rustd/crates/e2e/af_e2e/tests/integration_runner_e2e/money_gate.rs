//! Live daemon coverage for the tenant-credit run gate.

use super::*;

/// An exhausted tenant is refused at issue, and nothing is
/// written.
///
/// The credits gate is reached when a native session proves
/// its capabilities. The event is ended rather than retried forever, while no
/// run or charge is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_run_money_gate_refusal() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    assert!(
        !wait_without_assignment(&scene, async || { false }).await,
        "a newly enrolled runner receives no work before proving its capabilities"
    );
    assert_eq!(run_rows(&scene).await, 0);

    // Drained after the seed, so the wallet row exists and holds zero.
    scene.drain_wallet().await;

    let beat = crate::wire::heartbeat(&scene, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );

    // Durable refusal proves the assignment ran; silence alone would not.
    let refused = wait_without_assignment(&scene, async || {
        event_column(&scene, &scene.turn_id, "status")
            .await
            .as_deref()
            == Some("gate_blocked")
    })
    .await;
    assert!(
        refused,
        "an exhausted tenant's event is ended at the money gate"
    );
    assert_eq!(
        run_rows(&scene).await,
        0,
        "the gate refuses before writing a run row"
    );
    assert_eq!(
        balance(&scene).await,
        Some(0),
        "the refused assignment charges nothing"
    );

    // Stop background tasks before releasing the scenario's placement guard.
    supervisor.shutdown().await;
    scene.cleanup().await;
}
