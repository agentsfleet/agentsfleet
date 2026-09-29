//! The sweep's one failure arm: a row that does not decode.
//!
//! `EXPIRE_GATES` has already expired every row by the time the sweep decodes
//! them, so a bad row must not strand the good ones. Each good row's fleet
//! parked a delivery that only the sweep's wake ends; skip it and that
//! delivery waits out the idle autoclaim instead. The statement cannot return
//! an undecodable row by construction, so the batch is handed in through the
//! crate's `test-util` seam, with a real error standing in for the bad row.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::error::one_of_each_kind;
use afd_dragonfly::ReadyIndex;

use crate::lane::Lane;

/// The gate identifier the good row names; the wake and the frame only carry it.
const GATE: &str = "gate-fixture";

/// A bad row ahead of a good one: the good row's fleet is still woken, and the
/// sweep still answers the bad row's error.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_undecodable_row_does_not_strand_the_rows_that_decoded() {
    let lane = Lane::isolated().await;
    let fleet = lane.fleet.as_str().to_owned();
    let index = ReadyIndex::new(lane.queue.clone());
    // The state a park leaves behind: the mark gone, waiting on the answer.
    index
        .force_clear(&fleet)
        .await
        .expect("the test can clear the ready mark");
    let (label, undecodable) = one_of_each_kind()
        .into_iter()
        .next()
        .expect("the crate names at least one error kind");
    let code = undecodable.code();

    let raised = lane
        .inbox
        .serve_swept(vec![
            Err(undecodable),
            Ok((GATE.to_owned(), fleet.clone(), None, 0)),
        ])
        .await
        .expect_err("a row that does not decode still fails the sweep");

    assert_eq!(
        raised.code(),
        code,
        "the bad row's own error is raised ({label})"
    );
    assert!(
        index
            .token_for(&fleet)
            .await
            .expect("the ready index is readable")
            .is_some(),
        "the row that decoded is woken even though an earlier row did not decode"
    );
}
