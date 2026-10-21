//! A lapsed slot requalifies the same row and carries its original billing.

use afd_core::id::Uuid7;
use afd_fleet::lease::{Acquired, Billed, Delivery, Leases};

use super::{enqueue, next};
use crate::rows::Row;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_reclaimed_row_is_leased_as_redelivered() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let leases = fixtures.leases_with_dead_queue();
    let first = next(&leases, &holder).await.expect("first claim");
    assert!(!first.redelivered);
    assert!(first.reused.is_none());
    let received = leases.record_received(&first).await.expect("open event");
    assert_eq!(received.delivery, Delivery::First);
    let issued = issue(&fixtures, &leases, &first, &holder, &tenant).await;
    fixtures.lapse_holder(&issued, &fleet).await;

    let reclaimed = next(&leases, &poller)
        .await
        .expect("expired slot requalifies");
    assert_eq!(reclaimed.event_id, event, "no sweeper or stream repair ran");
    assert!(reclaimed.redelivered);
    assert!(reclaimed.fence > first.fence);
    let reused = reclaimed.reused.expect("original billing context");
    assert_eq!(
        (
            reused.tenant_id.as_str(),
            reused.posture.as_str(),
            reused.model.as_str()
        ),
        (tenant.as_str(), POSTURE, MODEL)
    );
    assert_eq!(
        fixtures
            .text(Row::Lease(&issued), "status")
            .await
            .as_deref(),
        Some(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
    );
    assert!(
        next(&leases, &holder).await.is_none(),
        "new slot remains exclusive"
    );
    drop(leases);
    fixtures.cleanup().await;
}

async fn issue(
    fixtures: &Fixtures,
    leases: &Leases,
    acquired: &Acquired,
    holder: &Uuid7,
    tenant: &str,
) -> String {
    let tenant = Uuid7::parse(tenant).expect("fixture tenant");
    leases
        .issue(
            holder,
            acquired,
            Billed {
                tenant_id: &tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            fixtures.enrolled_at(),
        )
        .await
        .expect("issue query")
        .expect("live fence")
        .lease_id
        .as_str()
        .to_owned()
}
