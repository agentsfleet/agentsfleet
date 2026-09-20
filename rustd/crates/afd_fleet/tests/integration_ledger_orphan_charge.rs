//! A charge written against a fleet the database cannot show you.
//!
//! Separate from the capture suite because it is the opposite question. That
//! one asks what the name says when the fleet is there; this asks what happens
//! when it is not — and the answer has to be "a charge, with a null name",
//! because the wallet is debited on this path. A charge refused for want of a
//! name is money taken with nothing in the ledger to show for it.
//!
//! The state is reachable for two reasons that arrive together. Slot 915
//! dropped the foreign key, so the row may now name a fleet that does not
//! exist; and the name is read by subselect, so a miss yields NULL instead of
//! failing. Both of the tempting alternative spellings lose the charge: a join
//! drops the row, a `NOT NULL` column refuses it.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::{Accounts, Charged, Posture};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use sqlx::Row as _;

use crate::queue;
use crate::report_seed;
use crate::seed::MODEL;

use self::report_seed::{Held, held};

/// Dimension 2.2. A missing fleet row costs the charge nothing but its name.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_charge_survives_unreadable_fleet_row() {
    let held = held().await;
    let tenant = Uuid7::parse(&held.tenant).expect("the fixture tenant is a v7 spelling");
    let fleet = Uuid7::parse(&held.fleet).expect("the fixture fleet is a v7 spelling");
    let workspace = workspace_of(&held).await;
    // Distinct from the fixture's own event, so the replay guard on
    // `(event_id, charge_type)` cannot turn this insert into a no-op that would
    // read as a pass.
    let orphan_event = format!("{}-orphan", held.event_id);

    remove_fleet(&held).await;

    let charged = Accounts::new(held.fixtures.database.clone(), Entropy::new())
        .debit_receive(
            Charged {
                tenant_id: &tenant,
                workspace_id: &workspace,
                fleet_id: &fleet,
                event_id: &orphan_event,
                posture: Posture::Platform,
                model: MODEL,
                event_created_at: held.now,
            },
            held.now,
        )
        .await;
    assert!(
        charged.is_ok(),
        "a missing fleet row must never cost a charge: {:?}",
        charged.err()
    );

    let (identifier, name) = orphan_row(&held, &orphan_event).await;
    assert_eq!(
        identifier,
        Some(held.fleet.clone()),
        "the identifier is bound by the caller, so it must be written whether \
         or not the fleet row can be read — it is what the dashboard derives \
         the callsign from"
    );
    assert_eq!(
        name, None,
        "an unreadable fleet must leave the name NULL — anything else is a \
         fabricated name a reader cannot tell from a captured one"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// The workspace the fixture's fleet belongs to, read before the fleet is gone.
///
/// `billing.usage_ledger.workspace_id` is still a foreign key, so the charge
/// below needs a workspace that exists; only `fleet_id` was freed.
async fn workspace_of(held: &Held) -> Uuid7 {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let workspace: String =
        sqlx::query("SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the fixture fleet must still be readable here")
            .try_get(0)
            .expect("workspace_id decodes as text");
    Uuid7::parse(&workspace).expect("the fixture workspace is a v7 spelling")
}

/// Deletes the fleet row, leaving the ledger's reference to it dangling.
///
/// The same state a hard purge leaves, reached without the purge — this suite
/// is about the WRITE meeting a missing fleet, and `afd_fleet_lifecycle` owns
/// what the purge itself destroys.
async fn remove_fleet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM core.fleets WHERE id = $1::uuid")
        .bind(&held.fleet)
        .execute(&mut *connection)
        .await
        .expect("the fleet row is removed");
}

/// The identifier and name the orphaned charge ended up with.
async fn orphan_row(held: &Held, event: &str) -> (Option<String>, Option<String>) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT fleet_id::text, fleet_name FROM billing.usage_ledger WHERE event_id = $1",
    )
    .bind(event)
    .fetch_optional(&mut *connection)
    .await
    .expect("reading the orphaned charge")
    .expect("the charge must have been written");
    (
        row.try_get(0).expect("fleet_id decodes as text"),
        row.try_get(1).expect("fleet_name decodes as text"),
    )
}
