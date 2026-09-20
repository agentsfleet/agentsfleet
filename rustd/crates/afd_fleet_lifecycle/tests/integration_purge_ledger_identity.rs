//! What a hard purge erases, and the one thing it must now leave behind.
//!
//! `billing.usage_ledger` has always outlived the fleet it charges — the wallet
//! was already debited and the reconciliation between the two must still add
//! up. What it did not outlive was any way to say WHICH fleet: `fleet_id` was a
//! foreign key with `ON DELETE SET NULL`, so the purge nulled the only column
//! that answered the question and the billing surface rendered a real charge as
//! a deleted one.
//!
//! Proven against live Postgres rather than by reading the slot, because the
//! claim is about what the SERVER does when a parent row goes: a statement-shape
//! test cannot tell a dropped constraint from one the migration missed. That
//! distinction is the whole slot — `DROP CONSTRAINT IF EXISTS` on a guessed name
//! reports success while the `SET NULL` is still armed.
//!
//! The second test is the other half, and the more important one: an erasure
//! this spec RELAXES must be shown not to have relaxed anything else. Memory,
//! approval gates, integration grants and sessions are counted after the purge
//! and every count must be zero.
//!
//! `#[ignore]`d; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use sqlx::Row as _;

use afd_fleet_lifecycle::{Patch, Requested};

use crate::integration_patch_visibility::installed;
use crate::support::{Lane, mint};

/// The name the fleet carries when the charge is written.
///
/// Distinct from the lane's own `'fixture'` tenant and workspace names so a
/// value read back cannot have come from either by accident.
const CHARGED_AS: &str = "deploy-bot";

/// A charge type and a posture the ledger accepts.
///
/// Spelled here rather than borrowed from `afd_billing`: this crate does not
/// depend on it, and a literal in a fixture that only has to be well formed is
/// cheaper than a crate edge drawn to share two words.
const CHARGE_TYPE: &str = "receive";
/// The posture column's platform spelling.
const POSTURE: &str = "platform";
/// Any model name; nothing here prices it.
const MODEL: &str = "claude-opus-5";

/// Writes one charge against a fleet, capturing the name the way the daemon does.
///
/// `fleet_name` is read from the fleet row by subselect rather than bound, which
/// is how all three production insert sites write it — a fixture that bound a
/// literal would prove the column holds text, not that the capture works.
async fn charge(lane: &Lane, fleet: &Uuid7) -> Uuid7 {
    let row = mint();
    sqlx::query(
        "INSERT INTO billing.usage_ledger
           (id, tenant_id, workspace_id, fleet_id, event_id,
            charge_type, posture, model,
            event_created_at, created_at, last_charged_at, fleet_name)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, $9, $9,
                 (SELECT f.name FROM core.fleets f WHERE f.id = $4::uuid))",
    )
    .bind(row.as_str())
    .bind(lane.tenant.as_str())
    .bind(lane.workspace.as_str())
    .bind(fleet.as_str())
    .bind(row.as_str())
    .bind(CHARGE_TYPE)
    .bind(POSTURE)
    .bind(MODEL)
    .bind(Lane::now().as_millis())
    .execute(&mut *lane.connection().await)
    .await
    .expect("the ledger must accept a charge");
    row
}

/// Gives the fleet the name its charges should record.
async fn rename(lane: &Lane, fleet: &Uuid7, name: &str) {
    sqlx::query("UPDATE core.fleets SET name = $2 WHERE id = $1::uuid")
        .bind(fleet.as_str())
        .bind(name)
        .execute(&mut *lane.connection().await)
        .await
        .expect("the fleet row must accept a rename");
}

/// The identifier and the name a ledger row still carries, if the row is there.
async fn ledger_identity(lane: &Lane, row: &Uuid7) -> Option<(Option<String>, Option<String>)> {
    sqlx::query("SELECT fleet_id::text, fleet_name FROM billing.usage_ledger WHERE id = $1::uuid")
        .bind(row.as_str())
        .fetch_optional(&mut *lane.connection().await)
        .await
        .expect("reading the ledger row")
        .map(|found| {
            (
                found.try_get(0).expect("fleet_id decodes as text"),
                found.try_get(1).expect("fleet_name decodes as text"),
            )
        })
}

/// Rows left in one table for one fleet.
///
/// The table name is a literal from this file, never input.
async fn rows_for(lane: &Lane, table: &str, fleet: &Uuid7) -> i64 {
    let statement = sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table} WHERE fleet_id = $1::uuid"
    ));
    sqlx::query(statement)
        .bind(fleet.as_str())
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("counting a fleet's rows")
        .try_get::<i64, _>(0)
        .expect("a count is a bigint")
}

/// Gives the fleet one row in every table the purge is responsible for.
///
/// Two gates rather than one: the append-only trigger fires per row, so a purge
/// that opened its entitlement for the first and lost it for the second would
/// leave exactly one behind — and a single-row fixture cannot see that.
async fn seed_everything_the_purge_destroys(lane: &Lane, fleet: &Uuid7) {
    let at = Lane::now().as_millis();
    let mut connection = lane.connection().await;

    sqlx::query(
        "INSERT INTO memory.memory_entries
           (id, key, content, category, fleet_id, created_at, updated_at)
         VALUES ($1::uuid, 'note', 'remembered', 'fact', $2::uuid, $3, $3)",
    )
    .bind(mint().as_str())
    .bind(fleet.as_str())
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("seeding a memory entry");

    for action in ["deploy", "rollback"] {
        sqlx::query(
            "INSERT INTO core.fleet_approval_gates
               (id, fleet_id, workspace_id, action_id, tool_name, action_name,
                gate_kind, proposed_action, evidence, blast_radius, timeout_at,
                resolved_by, status, detail, created_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'shell', $4,
                     'confirm', $4, '{}'::jsonb, 'workspace', $5,
                     '', 'pending', '', $5)",
        )
        .bind(mint().as_str())
        .bind(fleet.as_str())
        .bind(lane.workspace.as_str())
        .bind(action)
        .bind(at)
        .execute(&mut *connection)
        .await
        .expect("seeding an approval gate");
    }

    sqlx::query(
        "INSERT INTO core.integration_grants
           (id, fleet_id, service, status, requested_reason, created_at)
         VALUES ($1::uuid, $2::uuid, 'github', 'requested', 'fixture', $3)",
    )
    .bind(mint().as_str())
    .bind(fleet.as_str())
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("seeding an integration grant");

    sqlx::query(
        "INSERT INTO core.fleet_sessions
           (fleet_id, checkpoint_at, created_at, updated_at)
         VALUES ($1::uuid, $2, $2, $2)",
    )
    .bind(fleet.as_str())
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("seeding a session");
}

/// Takes the fleet to `killed`, which is the only status a purge accepts.
///
/// `Fleets::purge` probes the status first and answers `MustKillFirst` for
/// anything else — deleting a fleet is two deliberate steps, not one, so an
/// operator cannot erase a running fleet with a single call.
async fn kill(lane: &Lane, fleet: &Uuid7) {
    lane.fleets
        .patch(
            &lane.workspace,
            fleet,
            &Patch {
                status: Some(Requested::Killed),
                ..Patch::default()
            },
            Lane::now(),
        )
        .await
        .expect("active to killed is legal");
}

/// Dimension 1.1. The charge still names its fleet after the fleet is gone.
///
/// Both halves in one purge, because they fail for different reasons and a
/// reader needs to see which: the identifier survives only if the foreign key
/// was actually dropped, and the name survives only if the capture happened at
/// charge time rather than being joined at read time.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn test_m201_ledger_retains_fleet_id_across_purge() {
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    rename(&lane, &fleet.id, CHARGED_AS).await;
    let row = charge(&lane, &fleet.id).await;

    kill(&lane, &fleet.id).await;
    lane.fleets
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect("a killed fleet purges");

    assert!(
        lane.fleet_column(&fleet.id, "name").await.is_none(),
        "the fleet row must be gone — otherwise this proves nothing about what \
         survives a purge"
    );

    let (identifier, name) = ledger_identity(&lane, &row)
        .await
        .expect("the charge must outlive the fleet it charged");
    assert_eq!(
        identifier.as_deref(),
        Some(fleet.id.as_str()),
        "the purge nulled fleet_id — the foreign key's ON DELETE SET NULL is \
         still armed, so slot 915 did not find the constraint it drops"
    );
    assert_eq!(
        name.as_deref(),
        Some(CHARGED_AS),
        "the captured name did not survive the fleet row it was copied from"
    );

    lane.cleanup().await;
}

/// Dimension 1.4. Relaxing one erasure relaxed no other.
///
/// The risk this spec carries: `fleet_id` is retained by REMOVING a referential
/// action, and the tempting way to write that migration reaches for the wrong
/// constraint or drops a cascade beside it. Counting the four children after a
/// purge is the only assertion that cannot be satisfied by a diff that looks
/// right.
///
/// The ledger is counted in the same test rather than trusted to the one above:
/// "destroys no less" and "destroys no more" are one property read from two
/// sides, and separating them lets a fixture drift so each passes against a
/// different fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn test_m201_purge_destroys_no_less_than_before() {
    let lane = Lane::create().await;
    let fleet = installed(&lane).await;
    seed_everything_the_purge_destroys(&lane, &fleet.id).await;
    let row = charge(&lane, &fleet.id).await;

    for (table, expected) in [
        ("memory.memory_entries", 1),
        ("core.fleet_approval_gates", 2),
        ("core.integration_grants", 1),
        ("core.fleet_sessions", 1),
    ] {
        assert_eq!(
            rows_for(&lane, table, &fleet.id).await,
            expected,
            "{table} must hold the seeded rows before the purge, or the count \
             after it proves nothing"
        );
    }

    kill(&lane, &fleet.id).await;
    lane.fleets
        .purge(&lane.workspace, &fleet.id)
        .await
        .expect("a killed fleet holding every child purges");

    for table in [
        "memory.memory_entries",
        "core.fleet_approval_gates",
        "core.integration_grants",
        "core.fleet_sessions",
    ] {
        assert_eq!(
            rows_for(&lane, table, &fleet.id).await,
            0,
            "the purge left rows in {table} — retaining the ledger's identifier \
             must not have cost any other erasure"
        );
    }

    assert!(
        ledger_identity(&lane, &row).await.is_some(),
        "the purge destroyed a charge — the wallet was already debited for it \
         and the reconciliation now cannot be answered"
    );

    lane.cleanup().await;
}
