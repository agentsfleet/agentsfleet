//! What each charging path records about the fleet it charged, and when.
//!
//! Three statements write `billing.usage_ledger` — the receive debit in
//! `afd_billing`, the report's claim-and-settle, and the renewal's meter — and
//! all three now capture `core.fleets.name` into the row. The property is not
//! that the column holds text. It is that the name is a SNAPSHOT: read once, at
//! the instant the money moved, and never revisited. A rename afterwards must
//! leave the charge alone, because a ledger records what was true, not what is.
//!
//! Every test here drives the real path — `Plane::renew`, `settle_alone`,
//! `Accounts::debit_receive` — rather than the statement text. `afd_fleet`'s
//! `lease::sql` already asserts the shape of both its statements, and shape is
//! what a careless `DO UPDATE SET` passes: the accumulate clause can list every
//! column a reader would expect and still be wrong about this one.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::Settled;
use afd_wire::report::RenewRequest;
use sqlx::Row as _;

use crate::queue;
use crate::report_seed;

use self::report_seed::{Held, held, run_fee_meter};

/// The name an operator gives the fleet after its first charge is on the books.
///
/// The fixture names a fleet after its own identifier, so any value that is not
/// a UUID makes a before/after read unambiguous at a glance.
const RENAMED_TO: &str = "deploy-bot";

/// A second rename, for the accumulate path.
///
/// Distinct from [`RENAMED_TO`] so a test that renews twice can tell "the first
/// capture stood" from "the last write happened to win".
const RENAMED_AGAIN: &str = "deploy-bot-mk2";

/// The ledger's two charge types, as the rows are addressed here.
const RECEIVE: &str = "receive";
/// The run's own charge, written by report and accumulated by renewal.
const STAGE: &str = "stage";

/// The captured name on one event's charge of one type.
///
/// `Option<Option<String>>`: the outer says whether the row is there at all,
/// the inner whether a name was captured. Collapsing them would let a missing
/// row read as an uncaptured name, which is the difference between "the charge
/// was dropped" and "the fleet could not be read" — opposite defects.
///
/// Keyed on the FLEET as well as the event, and that is not belt-and-braces.
/// The fixture mints its logical event id from a process-local counter and a
/// fixed instant, so two runs against a lane database that was not reset
/// produce the same id — and `(event_id, charge_type)` is unique across the
/// whole table, not per tenant. An event-only lookup answered with a previous
/// run's row, belonging to a different fleet, and read as a capture defect.
/// The fleet id is minted per fixture and cannot collide that way.
async fn captured_name(held: &Held, charge_type: &str) -> Option<Option<String>> {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "SELECT fleet_name FROM billing.usage_ledger
         WHERE event_id = $1 AND charge_type = $2 AND fleet_id = $3::uuid",
    )
    .bind(&held.event_id)
    .bind(charge_type)
    .bind(&held.fleet)
    .fetch_optional(&mut *connection)
    .await
    .expect("reading the captured name")
    .map(|row| row.try_get(0).expect("fleet_name decodes as text"))
}

/// Gives the live fleet a new name, the way an operator editing it would.
async fn rename(held: &Held, name: &str) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET name = $2 WHERE id = $1::uuid")
        .bind(&held.fleet)
        .bind(name)
        .execute(&mut *connection)
        .await
        .expect("the live fleet is renamed");
}

/// A daily ceiling nothing in this file can spend through.
///
/// The generic fixture seeds `config_json` as `{}`, and an unreadable ceiling
/// makes the renewal fail CLOSED with `BudgetExhausted` — deliberately, per the
/// coverage suite next door. A test whose subject is the captured name has to
/// clear that gate first or it never reaches the statement it is about.
const DEEP_DAILY_BUDGET: &str = r#"{"name":"ledger-name","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1000}}}"#;

/// Gives the fleet a ceiling its renewals can run under.
async fn fund(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid")
        .bind(&held.fleet)
        .bind(DEEP_DAILY_BUDGET)
        .execute(&mut *connection)
        .await
        .expect("the fleet takes a readable ceiling");
}

/// Renews the held lease once, which meters a slice into the stage row.
async fn renew(held: &Held) {
    let plane = held.fixtures.plane();
    plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            RenewRequest::default(),
            held.now,
        )
        .await
        .expect("a funded lease renews");
    drop(plane);
}

/// Reports the held lease, which settles the run and writes the stage row.
async fn report(held: &Held) {
    let outcome = held
        .fixtures
        .settle_alone(
            &held.leases,
            held.issued.lease_id.as_str(),
            &held.runner,
            run_fee_meter(),
            true,
            held.now,
        )
        .await;
    assert!(
        matches!(outcome, Settled::Claimed(_)),
        "the only holder of this fleet cannot be fenced out of its own report"
    );
}

/// Releases the lane's queue entry and drops the fixture's database.
async fn finish(held: Held) {
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 2.2. Every path that writes a charge captures the fleet's name.
///
/// Two lanes, because the stage row has two authors and only one of them writes
/// it in any single run: a reported lease is settled by `CLAIM_AND_SETTLE`, a
/// renewed one is metered by `RENEW_AND_METER`, and the second is an `ON
/// CONFLICT` accumulate onto whatever the first left. Driving only one would
/// leave the other free to omit the column — which is exactly the shape of the
/// defect slot 915 exists to end, since a charge with no captured name is
/// unattributable the moment its fleet is purged.
///
/// The receive row is asserted in both lanes rather than once. It is written by
/// a third crate on a third path, and a fixture change that stopped writing it
/// would otherwise show up as a puzzling absence somewhere else.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_all_insert_sites_capture_fleet_name() {
    let reported = held().await;
    rename(&reported, RENAMED_TO).await;
    report(&reported).await;
    assert_eq!(
        captured_name(&reported, STAGE).await,
        Some(Some(RENAMED_TO.to_owned())),
        "the report path wrote a stage charge without capturing the fleet's name"
    );
    let receive_name = captured_name(&reported, RECEIVE).await;
    assert_eq!(
        receive_name,
        Some(Some(reported.fleet.clone())),
        "the receive debit must have captured the name the fleet carried when \
         the event was admitted — the fixture names a fleet after its own id"
    );
    finish(reported).await;

    let renewed = held().await;
    fund(&renewed).await;
    rename(&renewed, RENAMED_TO).await;
    renew(&renewed).await;
    assert_eq!(
        captured_name(&renewed, STAGE).await,
        Some(Some(RENAMED_TO.to_owned())),
        "the renewal path wrote a stage charge without capturing the fleet's name"
    );
    finish(renewed).await;
}

/// Dimension 2.3. Accumulating onto a charge does not re-stamp its name.
///
/// The silent one, and the reason the `DO UPDATE SET` clause omits the column
/// deliberately rather than by oversight. Renewals run every few seconds of a
/// live run, so a listed `fleet_name` would be re-read on every tick and a
/// mid-run rename would rewrite the charge's whole history — not a stale value,
/// an actively falsified one.
///
/// Asserted through two real renewals with a rename between them, because the
/// statement-shape test next door (`afd_fleet::lease::sql`) proves the clause
/// does not MENTION the column and cannot prove the server agrees.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_accumulate_preserves_captured_name() {
    let held = held().await;
    fund(&held).await;
    rename(&held, RENAMED_TO).await;
    renew(&held).await;
    assert_eq!(
        captured_name(&held, STAGE).await,
        Some(Some(RENAMED_TO.to_owned())),
        "the first renewal must capture a name for the second to preserve"
    );

    rename(&held, RENAMED_AGAIN).await;
    renew(&held).await;

    assert_eq!(
        captured_name(&held, STAGE).await,
        Some(Some(RENAMED_TO.to_owned())),
        "the accumulate path re-stamped the name — a rename now rewrites what \
         the charge said when the money moved"
    );
    finish(held).await;
}

/// Dimension 2.4. A rename reaches new charges and no old ones.
///
/// The user-visible half of the snapshot rule, read the way the billing surface
/// reads it: the receive charge was written before the rename and the stage
/// charge after, so one page of charges must show both names at once. A
/// read-time join — the obvious alternative implementation — would show the new
/// name twice and be indistinguishable here from a correct row until the fleet
/// was purged.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_rename_does_not_rewrite_history() {
    let held = held().await;
    let before = held.fleet.clone();

    rename(&held, RENAMED_TO).await;
    report(&held).await;

    assert_eq!(
        captured_name(&held, RECEIVE).await,
        Some(Some(before)),
        "the rename rewrote a charge that was already on the books"
    );
    assert_eq!(
        captured_name(&held, STAGE).await,
        Some(Some(RENAMED_TO.to_owned())),
        "a charge written after the rename must carry the new name, or the \
         capture is not happening at charge time at all"
    );
    finish(held).await;
}
