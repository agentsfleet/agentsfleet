//! §3 Dimension 3.1 — the install guarantee, and what a failed install leaves.
//!
//! `#[ignore]`d so `make test-unit-all` compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them against compose Postgres
//! and Dragonfly.
//!
//! # What only this lane can prove
//!
//! The unit suite proves the retry SCHEDULE — four attempts, three sleeps,
//! jittered, inside the wall budget. It cannot prove the claim the dimension
//! actually makes, which is about a ROW: that the stream and its consumer group
//! exist before the install answers, and that an install which could not create
//! them leaves `core.fleets` exactly as it found it. Both are statements about
//! state, and a response assertion cannot make either.
//!
//! # The partial-completion window
//!
//! One install touches the pool three times around a Dragonfly call:
//!
//! ```text
//!   acquire PG ①  library read → INSERT core.fleets (installing)
//!   release PG    ← released BEFORE Dragonfly, so a slow queue is not a PG outage
//!                 XGROUP CREATE          ← 4 attempts, ~1.75s, jittered
//!   acquire PG ②  flip installing → active
//!     on failure  acquire PG ③ (fresh)   DELETE the row
//! ```
//!
//! The release-then-reacquire gap is the interesting one, and the two failure
//! classes on either side of it answer differently ON PURPOSE: a transport
//! failure is retried, a refused command is not.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Instant;

use afd_core::error_code;
use afd_fleet_lifecycle::{Install, LibrarySource};

use crate::support::{LIBRARY_ID, Lane};

/// The install request every test here makes.
fn request() -> Install<'static> {
    Install {
        source: LibrarySource::Platform(LIBRARY_ID),
        name: None,
        mention: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn an_install_leaves_the_stream_and_its_group_before_it_answers() {
    // The guarantee itself. An event published a millisecond after the 201 has
    // to find the group the lease XREADGROUP reads through, so the group
    // existing is not a later consequence of the install — it is part of it.
    let lane = Lane::create().await;

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("a seeded library entry installs");

    assert!(
        lane.has_consumer_group(&installed.id).await,
        "the consumer group must exist by the time install returns"
    );
    assert_eq!(
        lane.fleet_column(&installed.id, "status").await.as_deref(),
        Some("active"),
        "the flip is part of the pipeline, not a later worker's job"
    );
    assert_eq!(lane.fleet_count(&lane.workspace).await, 1);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn a_queue_that_never_answers_rolls_the_row_back_and_leaves_no_orphan() {
    // Workflow ordinal 2 (the stream setup), transport class, retries exhausted.
    //
    // Residual state is the whole assertion: the response says the install
    // failed, and only the row count says whether that failure was clean. An
    // orphan here is a fleet nobody can see, lease or delete.
    let lane = Lane::create().await;
    let before = lane.fleet_count(&lane.workspace).await;

    let failure = lane
        .with_dead_queue()
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect_err("a queue that never answers cannot finish an install");

    assert_eq!(
        failure.code(),
        error_code::AGENTSFLEET_INSTALL_ROLLED_BACK,
        "the caller is told nothing was kept, which is what makes a retry safe"
    );
    assert_eq!(
        lane.fleet_count(&lane.workspace).await,
        before,
        "the INSERT is rolled back on a fresh connection — no orphan row"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn a_refused_command_fails_fast_rather_than_spending_the_retry_budget() {
    // Workflow ordinal 2 again, COMMAND class this time, and the point is the
    // classification: Dragonfly is up and answering. `XGROUP CREATE … MKSTREAM`
    // against a key holding a string is `WRONGTYPE`, and asking three more
    // times answers the same. Spending 1.75 seconds on that makes a person wait
    // out a foregone conclusion.
    //
    // Injection: the fleet id is minted INSIDE the install, so no test can
    // occupy the key in advance for a fleet that does not exist yet. So the
    // proof is made one level down, against the same `ensure_group` the install
    // calls — install once to get a real fleet id, put a string on its stream
    // key, and ask again. Same command, same client, same classification.
    let lane = Lane::create().await;
    let first = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("the first install succeeds");
    lane.occupy_stream_key(&first.id).await;

    let started = Instant::now();
    let refused = afd_dragonfly::FleetStreams::new(lane.queue.clone())
        .ensure_group(first.id.as_str())
        .await
        .expect_err("a key holding a string is not a stream");
    let elapsed = started.elapsed();

    assert!(
        !refused.is_unavailable(),
        "WRONGTYPE is a command error, and the install must not retry one"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "a refused command answered in {elapsed:?}, which is the retry budget being spent"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn a_rolled_back_install_can_be_retried_into_a_working_fleet() {
    // The retry-heals column of the matrix, and the reason the rollback is worth
    // its complexity: "nothing was created" is only useful if acting on it
    // works. Same workspace, same library entry, same name — the second attempt
    // must not collide with a row the first one left behind.
    let lane = Lane::create().await;

    let failed = lane
        .with_dead_queue()
        .install(&lane.workspace, &request(), Lane::now())
        .await;
    assert!(failed.is_err(), "the first attempt cannot finish");

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("the retry heals, because the rollback took the name with it");

    assert_eq!(
        lane.fleet_column(&installed.id, "name").await.as_deref(),
        Some("daily-digest"),
        "the retry takes the bundle's own name, not a suffixed re-draw"
    );
    assert_eq!(lane.fleet_count(&lane.workspace).await, 1);

    lane.cleanup().await;
}

/// An install writes no bundle pointer, and every later column still lands.
///
/// `INSERT_FLEET` used to write `bundle_snapshot_key`, deriving
/// `fleet-bundles/{hash}.tar.zst` — a layout the store stopped using when
/// preparation began deriving keys from the content hash, and a column nothing
/// has ever read. Removing the bind renumbered every parameter after it, which
/// is the risk this test exists for: a transposition there writes a wrong value
/// into a right column and compiles clean.
///
/// So the assertions are the pointer is NULL and the columns that moved are
/// still themselves. The content hash is the one that matters — it is what
/// retrieval derives the real object key from, and `afd_fleet::bundle` grades
/// that derivation against the layout the store actually uses.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn an_install_writes_no_bundle_pointer_and_renumbers_nothing_else() {
    let lane = Lane::create().await;

    let installed = lane
        .fleets
        .install(&lane.workspace, &request(), Lane::now())
        .await
        .expect("a seeded library entry installs");

    assert_eq!(
        lane.fleet_column(&installed.id, "bundle_snapshot_key")
            .await,
        None,
        "the unread pointer is not written; the column stays declared and NULL"
    );
    assert_eq!(
        lane.fleet_column(&installed.id, "status").await.as_deref(),
        Some("active"),
        "the status after the removed bind is still the status"
    );
    assert!(
        lane.fleet_column(&installed.id, "created_at")
            .await
            .is_some_and(|stamped| stamped == Lane::now().as_millis().to_string()),
        "both timestamps take the instant, which is the parameter the removal moved"
    );
    assert_eq!(
        lane.fleet_column(&installed.id, "updated_at").await,
        lane.fleet_column(&installed.id, "created_at").await,
        "one instant, stamped on both"
    );

    lane.cleanup().await;
}
