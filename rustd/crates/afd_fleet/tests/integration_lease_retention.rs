//! Lease acknowledgement trims completed history without discarding owed work.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "fixture preconditions must fail loudly")]

use afd_core::id::Uuid7;
use afd_dragonfly::streams::{ACKNOWLEDGED_HISTORY, Backlog, EventId, FleetStreams, TRIM_SLACK};

use crate::support::Fixtures;

const APPENDED: usize = ACKNOWLEDGED_HISTORY + TRIM_SLACK + 1;
const DELIVERED: usize = 30;
const ACKNOWLEDGED: usize = 20;

async fn deliver_history(streams: &FleetStreams, fleet: &str) -> Vec<EventId> {
    streams.ensure_group(fleet).await.expect("create group");
    let mut receipts = Vec::with_capacity(APPENDED);
    for sequence in 0..APPENDED {
        receipts.push(
            streams
                .append(fleet, &[("sequence", &sequence.to_string())])
                .await
                .expect("append history"),
        );
    }
    for expected in receipts.iter().take(DELIVERED) {
        let read = streams
            .read_new(fleet, "retention-consumer")
            .await
            .expect("read stream")
            .expect("an entry is waiting");
        assert_eq!(&read.receipt, expected);
    }
    for receipt in receipts.iter().take(ACKNOWLEDGED - 1) {
        assert!(
            streams
                .ack(fleet, receipt)
                .await
                .expect("acknowledge history")
        );
    }
    receipts
}

async fn assert_retained_work(
    fixtures: &Fixtures,
    streams: &FleetStreams,
    fleet: &Uuid7,
    receipts: &[EventId],
) -> Backlog {
    let remaining = afd_dragonfly::test_util::fleet_entries(fixtures.queue(), fleet.as_str())
        .await
        .expect("read retained stream");
    let retained: Vec<_> = remaining.iter().map(|entry| &entry.receipt).collect();
    assert_eq!(
        retained,
        receipts.iter().skip(ACKNOWLEDGED).collect::<Vec<_>>(),
        "only completed history below the oldest pending entry is removed"
    );
    let owed = streams
        .backlog(fleet.as_str())
        .await
        .expect("read group")
        .expect("group still exists");
    assert_eq!(
        owed.pending,
        u64::try_from(DELIVERED - ACKNOWLEDGED).expect("fits")
    );
    assert_eq!(
        owed.undelivered,
        Some(u64::try_from(APPENDED - DELIVERED).expect("fits"))
    );
    owed
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lease_acknowledgement_trims_history_and_preserves_pending_and_unread_entries() {
    let fixtures = Fixtures::create_with_queue().await;
    let fleet = Uuid7::parse(&afd_db::test_util::mint_id()).expect("minted fleet identifier");
    let streams = FleetStreams::new(fixtures.queue().clone());
    let receipts = deliver_history(&streams, fleet.as_str()).await;
    let last_acknowledged = receipts
        .get(ACKNOWLEDGED - 1)
        .expect("last completed entry");
    let leases = fixtures.leases();

    leases
        .acknowledge(&fleet, last_acknowledged)
        .await
        .expect("lease acknowledgement succeeds");
    let owed = assert_retained_work(&fixtures, &streams, &fleet, &receipts).await;

    leases
        .acknowledge(&fleet, last_acknowledged)
        .await
        .expect("a repeated acknowledgement remains successful");
    assert_eq!(
        streams.backlog(fleet.as_str()).await.expect("read group"),
        Some(owed)
    );
    streams
        .forget(fleet.as_str())
        .await
        .expect("remove this test's stream");
    fixtures.cleanup().await;
}
