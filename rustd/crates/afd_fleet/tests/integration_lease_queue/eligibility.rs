//! What the claim may take, and how a reclaimed delivery reads.

use afd_fleet::lease::Delivery;

use super::admit_at;
use crate::rows::Row;
use crate::seed::{claim_fleet, seeded_parts};
use crate::support::Fixtures;

/// A fleet status the claim must pass over.
const STOPPED: &str = "stopped";

/// A fleet whose tags a runner lacks, and a fleet that is not
/// active, are claimed by no runner that may not run them.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_respects_required_tags_and_fleet_status() {
    let fixtures = Fixtures::create_with_queue().await;
    let (tagged, tagged_ws, _, [plain]) = seeded_parts::<1>(&fixtures).await;
    let (stopped, stopped_ws, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let gpu = format!("gpu-{tagged}");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query("UPDATE fleet.fleets SET required_tags = required_tags || $2 WHERE id = $1::uuid")
        .bind(&tagged)
        .bind(vec![gpu.clone()])
        .execute(&mut *connection)
        .await
        .expect("the fleet asks for a gpu");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(&stopped)
        .bind(STOPPED)
        .execute(&mut *connection)
        .await
        .expect("the fleet stops");
    drop(connection);
    let _gpu_work = admit_at(&fixtures, &tagged, &tagged_ws, 0).await;
    let _stopped_work = admit_at(&fixtures, &stopped, &stopped_ws, 0).await;
    let leases = fixtures.leases();

    assert!(
        claim_fleet(&leases, &plain, &tagged).await.is_none(),
        "a runner without the gpu tag passes over the gpu fleet"
    );
    assert!(
        claim_fleet(&leases, &runner, &stopped).await.is_none(),
        "a stopped fleet's ready slot is claimed by nobody"
    );
    assert!(
        fixtures.slot_is_free(&tagged).await && fixtures.slot_is_free(&stopped).await,
        "neither slot was touched"
    );
    fixtures.cleanup().await;
}

/// A lapsed claim on an open event reads `Repeat`, redelivered;
/// on an event that already ended it reads `Terminal`, and the pass settles
/// it with no lease. The first delivery's instant stands throughout.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_reclaim_reports_repeat_and_terminal_correctly() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = admit_at(&fixtures, &fleet, &workspace, 0).await;
    let leases = fixtures.leases();
    let first = claim_fleet(&leases, &runner, &fleet).await.expect("first");
    let delivered = fixtures
        .instant(Row::Admission(&event), "delivered_at")
        .await;
    assert_eq!(
        (first.delivery, first.redelivered),
        (Delivery::First, false)
    );

    fixtures
        .age(
            Row::Slot(&fleet),
            "lease_expires_at",
            afd_core::timing::LEASE_TTL,
        )
        .await;
    let repeat = claim_fleet(&leases, &runner, &fleet).await.expect("repeat");
    assert_eq!(
        (repeat.delivery, repeat.redelivered),
        (Delivery::Repeat, true)
    );

    sqlx::query("UPDATE fleet.events SET status = $3 WHERE fleet_id = $1::uuid AND event_id = $2")
        .bind(&fleet)
        .bind(&event)
        .bind(afd_core::event::status::PROCESSED)
        .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the event ended elsewhere");
    fixtures
        .age(
            Row::Slot(&fleet),
            "lease_expires_at",
            afd_core::timing::LEASE_TTL,
        )
        .await;
    let terminal = claim_fleet(&leases, &runner, &fleet)
        .await
        .expect("terminal");
    assert_eq!(terminal.delivery, Delivery::Terminal);
    let answer = fixtures
        .plane()
        .lease_claimed(terminal, &runner, fixtures.enrolled_at())
        .await
        .expect("a finished event is a decision");

    assert!(answer.contains("\"lease\":null"), "{answer}");
    assert_eq!(
        (
            fixtures
                .value::<bool>(Row::Admission(&event), "settled_at IS NOT NULL")
                .await,
            fixtures
                .instant(Row::Admission(&event), "delivered_at")
                .await,
            fixtures.slot_is_free(&fleet).await,
        ),
        (Some(true), delivered, true),
        "settled with no lease, its first delivery instant kept, the slot free"
    );
    fixtures.cleanup().await;
}
