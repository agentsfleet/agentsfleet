//! What a hard purge erases, and the one thing it must now leave behind.
//!
//! `billing.usage_lines` outlives the fleet it charges — the wallet was
//! already debited and the reconciliation between the two must still add up.
//! It must also still say WHICH fleet: `fleet_id` carries no foreign key, so a
//! purge cannot null it, and `fleet_name` is captured at charge time, so the
//! billing surface never renders a real charge as a deleted one.
//!
//! Proven against live Postgres rather than by reading the slot, because the
//! claim is about what the SERVER does when a parent row goes: a statement-shape
//! test cannot tell a column with no referential action from one that grew a
//! `SET NULL` or a `CASCADE` in a later slot.
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

use crate::support::{Lane, mint};

/// The name the fleet carries when the charge is written.
///
/// Distinct from the lane's own `'fixture'` tenant and workspace names so a
/// value read back cannot have come from either by accident.
const CHARGED_AS: &str = "deploy-bot";

/// A work type and a posture the ledger accepts.
///
/// Spelled here rather than borrowed from `afd_billing`: this crate does not
/// depend on it, and a literal in a fixture that only has to be well formed is
/// cheaper than a crate edge drawn to share two words.
const WORK_TYPE: &str = "fleet_lease";
/// The posture column's platform spelling.
const POSTURE: &str = "platform";
/// Any model name; nothing here prices it.
const MODEL: &str = "claude-opus-5";

/// Every table a purge empties, with the rows
/// [`seed_everything_the_purge_destroys`] gives it.
pub(crate) const SEEDED: [(&str, i64); 4] = [
    ("memory.entries", 1),
    ("fleet.approval_gates", 2),
    ("connector.integration_grants", 1),
    ("fleet.sessions", 1),
];

/// Writes one usage line against a fleet, capturing the name the way the
/// daemon does.
///
/// `fleet_name` is read from the fleet row by subselect rather than bound, as
/// the lease path's metering statement reads it — a fixture that bound a
/// literal would prove the column holds text, not that the capture works.
async fn charge(lane: &Lane, fleet: &Uuid7) -> Uuid7 {
    let row = mint();
    sqlx::query(
        "INSERT INTO billing.usage_lines
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name,
            event_id, authoring_attempt_id, provider, model, posture, requested_at,
            input_token_count, cached_input_token_count, cache_write_token_count,
            cache_write_5m_token_count, cache_write_1h_token_count, output_token_count,
            runtime_ms, platform_model_price_id, unpriced_token_count, charged_nanos,
            created_at, updated_at)
         VALUES ($1::uuid, $5, $1::uuid, 0, $2::uuid, $3::uuid, $4::uuid,
                 (SELECT f.name FROM fleet.fleets f WHERE f.id = $4::uuid),
                 $1, NULL, 'anthropic', $7, $6, now(), 0, 0, 0, 0, 0, 0, 0, NULL, 0, 0,
                 now(), now())",
    )
    .bind(row.as_str())
    .bind(lane.tenant.as_str())
    .bind(lane.workspace.as_str())
    .bind(fleet.as_str())
    .bind(WORK_TYPE)
    .bind(POSTURE)
    .bind(MODEL)
    .execute(&mut *lane.connection().await)
    .await
    .expect("the ledger must accept a charge");
    row
}

/// Gives the fleet the name its charges should record.
async fn rename(lane: &Lane, fleet: &Uuid7, name: &str) {
    sqlx::query("UPDATE fleet.fleets SET name = $2 WHERE id = $1::uuid")
        .bind(fleet.as_str())
        .bind(name)
        .execute(&mut *lane.connection().await)
        .await
        .expect("the fleet row must accept a rename");
}

/// The identifier and the name a ledger row still carries.
#[derive(Debug, sqlx::FromRow)]
struct ChargedAs {
    fleet_id: String,
    fleet_name: String,
}

/// The identity a ledger row still carries, if the row is there.
async fn ledger_identity(lane: &Lane, row: &Uuid7) -> Option<ChargedAs> {
    sqlx::query_as("SELECT fleet_id::text, fleet_name FROM billing.usage_lines WHERE id = $1::uuid")
        .bind(row.as_str())
        .fetch_optional(&mut *lane.connection().await)
        .await
        .expect("reading the ledger row")
}

/// Rows left in one table for one fleet.
///
/// The table name is a literal from this file, never input. Counted as the
/// lane's owner, because `memory.entries` is out of `api_runtime`'s reach and
/// the count is the test's observation, not the code under test.
pub(crate) async fn rows_for(lane: &Lane, table: &str, fleet: &Uuid7) -> i64 {
    let statement = sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table} WHERE fleet_id = $1::uuid"
    ));
    let owner = lane.owner().await;
    let rows = sqlx::query_scalar(statement)
        .bind(fleet.as_str())
        .fetch_one(&mut *owner.acquire().await.expect("an owner connection"))
        .await
        .expect("counting a fleet's rows");
    owner.close().await;
    rows
}

/// Gives the fleet one row in every table the purge is responsible for.
///
/// Two gates rather than one: the append-only trigger fires per row, so a purge
/// that opened its entitlement for the first and lost it for the second would
/// leave exactly one behind — and a single-row fixture cannot see that.
///
/// Seeded as the lane's owner: `api_runtime` never writes `memory.entries`
/// itself, and the seed is the scene, not the purge under test.
pub(crate) async fn seed_everything_the_purge_destroys(lane: &Lane, fleet: &Uuid7) {
    let owner = lane.owner().await;
    let mut connection = owner.acquire().await.expect("an owner connection");

    sqlx::query(
        "INSERT INTO memory.entries
           (id, key, content, category, workspace_visible, fleet_id, workspace_id,
            created_at, updated_at)
         SELECT $1::uuid, 'note', 'remembered', 'fact', false, id, workspace_id, now(), now()
         FROM fleet.fleets WHERE id = $2::uuid",
    )
    .bind(mint().as_str())
    .bind(fleet.as_str())
    .execute(&mut *connection)
    .await
    .expect("seeding a memory entry");

    seed_approval_gates(&mut connection, lane, fleet).await;

    sqlx::query(
        "INSERT INTO connector.integration_grants
           (id, fleet_id, service, status, requested_reason, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, 'github', 'requested', 'fixture', now(), now())",
    )
    .bind(mint().as_str())
    .bind(fleet.as_str())
    .execute(&mut *connection)
    .await
    .expect("seeding an integration grant");

    sqlx::query(
        "INSERT INTO fleet.sessions (fleet_id, context_json, created_at, updated_at)
         VALUES ($1::uuid, '{}'::jsonb, now(), now())",
    )
    .bind(fleet.as_str())
    .execute(&mut *connection)
    .await
    .expect("seeding a session");
    drop(connection);
    owner.close().await;
}

/// Two pending approval gates on the fleet — see
/// [`seed_everything_the_purge_destroys`] for why two.
async fn seed_approval_gates(connection: &mut sqlx::PgConnection, lane: &Lane, fleet: &Uuid7) {
    for action in ["deploy", "rollback"] {
        sqlx::query(
            "INSERT INTO fleet.approval_gates
               (id, fleet_id, workspace_id, action_id, tool_name, action_name,
                gate_type, proposed_action, evidence, blast_radius, expires_at,
                status, detail, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'shell', $4,
                     'confirm', $4, '{}'::jsonb, 'workspace', now() + interval '1 hour',
                     'pending', '', now(), now())",
        )
        .bind(mint().as_str())
        .bind(fleet.as_str())
        .bind(lane.workspace.as_str())
        .bind(action)
        .execute(&mut *connection)
        .await
        .expect("seeding an approval gate");
    }
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
    let fleet = lane.fleet().await;
    rename(&lane, &fleet, CHARGED_AS).await;
    let row = charge(&lane, &fleet).await;

    let tag = lane.kill(&fleet).await;
    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect("a killed fleet purges");

    assert!(
        lane.fleet_column(&fleet, "name").await.is_none(),
        "the fleet row must be gone — otherwise this proves nothing about what \
         survives a purge"
    );

    let charged = ledger_identity(&lane, &row)
        .await
        .expect("the charge must outlive the fleet it charged");
    assert_eq!(
        charged.fleet_id,
        fleet.as_str(),
        "the purge nulled fleet_id — a referential action on \
         billing.usage_lines.fleet_id is armed, which schema/2040 must not grow"
    );
    assert_eq!(
        charged.fleet_name, CHARGED_AS,
        "the captured name did not survive the fleet row it was copied from"
    );

    lane.cleanup().await;
}

/// Dimension 1.4. Relaxing one erasure relaxed no other.
///
/// The risk this carries: `fleet_id` is retained by declaring NO referential
/// action, and the tempting way to write that slot drops a cascade beside it. Counting the four children after a
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
    let fleet = lane.fleet().await;
    seed_everything_the_purge_destroys(&lane, &fleet).await;
    let row = charge(&lane, &fleet).await;

    for (table, expected) in SEEDED {
        assert_eq!(
            rows_for(&lane, table, &fleet).await,
            expected,
            "{table} must hold the seeded rows before the purge, or the count \
             after it proves nothing"
        );
    }

    let tag = lane.kill(&fleet).await;
    lane.fleets
        .purge_tagged(&lane.workspace, &fleet, &tag)
        .await
        .expect("a killed fleet holding every child purges");

    for (table, _seeded) in SEEDED {
        assert_eq!(
            rows_for(&lane, table, &fleet).await,
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
