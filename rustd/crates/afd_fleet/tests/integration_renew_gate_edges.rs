//! The renewal's money gates at their edges: a hold already released, a fleet
//! with no installed revision, and a ceiling or a spend that cannot be read.
//!
//! `integration_renew_coverage.rs` proves the refusals. These prove the two
//! answers either side of them: a released hold means the run is over, so the
//! renewal is lost; a gate that cannot be read admits the run for one more
//! slice and says which gate it was, because failing closed on an outage would
//! stop every run in the deployment at once.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::biller::{Release, WorkKey};
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_db::config::DbRole;
use afd_wire::report::RenewRequest;

use crate::queue;
use crate::report_seed::{Held, held, held_in};
use crate::support::Fixtures;

/// A ceiling the fixture spend stays far inside.
const DEEP_DAILY_BUDGET: &str = r#"{"name":"renew-edges","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#;

/// The line an unreadable gate is logged under.
const GATE_UNREADABLE: &str = "renew_gate_unreadable_admitted";

/// Takes the charge column away, so every read that sums charges fails as a
/// database fault: the lease's own (the hold gate) and the fleet's (the spend
/// gate). Only in a database of the case's own.
const HIDE_CHARGES: &str =
    "ALTER TABLE billing.usage_lines RENAME COLUMN charged_nanos TO charged_nanos_hidden";

/// Moves the lease's runtime cursor past the database clock, so the window
/// measures nothing and the renewal logs no line into the hidden column.
const CURSOR_AHEAD: &str = "UPDATE runner.leases \
     SET last_metered_at = now() + interval '1 hour' WHERE id = $1::uuid";

/// Takes the revision table away, so the fleet's ceiling read fails as a
/// database fault. Only in a database of the case's own.
const HIDE_REVISIONS: &str = "ALTER TABLE fleet.revisions RENAME TO revisions_hidden";

/// Uninstalls the fleet's head revision, as a purge racing the run leaves it.
const UNINSTALL: &str = "UPDATE fleet.fleets SET head_revision_id = NULL WHERE id = $1::uuid";

/// Runs one scene-setting statement as the lane's owner: renaming a table or
/// column is the owner's right, and the scene is not the code under test.
async fn run(held: &Held, statement: &'static str, binds: &[&str]) {
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    let owner = held.fixtures.open(DbRole::Migrator).await;
    query
        .execute(&mut *owner.acquire().await.expect("an owner connection"))
        .await
        .expect("the fixture statement runs");
    owner.close().await;
}

/// Renews `held`'s lease with nothing to report.
async fn renew(held: &Held) -> afd_fleet::Result<(jiff::Timestamp, afd_billing::Nanos)> {
    held.fixtures
        .plane()
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            RenewRequest::default(),
            held.now,
        )
        .await
}

async fn close(held: Held) {
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A fleet with no installed revision has no ceiling left to read, so it
/// renews; once its hold is released the run is over and the renewal is lost.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_uninstalled_fleet_renews_and_a_released_hold_is_lost() {
    let held = held().await;
    run(&held, UNINSTALL, &[&held.fleet]).await;

    let uninstalled = renew(&held).await;
    held.fixtures
        .plane()
        .biller
        .release(
            &WorkKey::FleetLease(held.lease_key.clone()),
            Release::Completed,
        )
        .await
        .expect("the hold the renewal placed releases");
    let released = renew(&held).await;

    assert!(uninstalled.is_ok(), "{:?}", uninstalled.err());
    assert_eq!(
        released.err().map(|error| error.code()),
        Some(error_code::RUN_LEASE_LOST)
    );
    close(held).await;
}

/// Charges that cannot be summed, then a ceiling that cannot be read, each
/// admit the renewal and name every gate that could not be read: the hold and
/// the spend first, then the hold and the ceiling, which stops before the
/// spend is asked.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_gate_that_cannot_be_read_admits_the_run_and_names_itself() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    held.fixtures
        .native_config(&held.fleet, DEEP_DAILY_BUDGET)
        .await;
    run(&held, CURSOR_AHEAD, &[held.issued.lease_id.as_str()]).await;

    let mut gates = Vec::new();
    for hide in [HIDE_CHARGES, HIDE_REVISIONS] {
        run(&held, hide, &[]).await;
        let capture = Capture::install();
        let renewed = renew(&held).await;
        let lines: Vec<_> = capture
            .events()
            .into_iter()
            .filter(|line| line.field("event") == Some(GATE_UNREADABLE))
            .collect();
        drop(capture);
        assert!(renewed.is_ok(), "{:?}", renewed.err());
        for line in &lines {
            assert_eq!(line.field("lease_id"), Some(held.issued.lease_id.as_str()));
            // Each gate's own crate decides its code; what holds for all of
            // them is that the line carries one beside an uncoded sentence.
            assert!(
                line.field("error_code")
                    .is_some_and(|code| code.starts_with("UZ-")),
                "{:?}",
                line.field("error_code")
            );
            line.assert_uncoded_reason();
        }
        gates.push(
            lines
                .iter()
                .filter_map(|line| line.field("gate").map(str::to_owned))
                .collect::<Vec<_>>(),
        );
    }

    assert_eq!(gates, [["balance", "spend"], ["balance", "budget"]]);
    close(held).await;
}
