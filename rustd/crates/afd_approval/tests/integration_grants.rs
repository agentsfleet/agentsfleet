//! The operator's grant verbs against a live table.
//!
//! # Why these need a real datastore
//!
//! Every claim here is a claim about a STATEMENT. The revoke's idempotence is
//! `status != 'revoked'` inside one UPDATE, the list's order is `ORDER BY
//! created_at DESC`, and the scope refusal is a predicate over `core.fleets`.
//! A stub would be asserting that the code calls a statement, which was never
//! in doubt; what is under test is that the statement decides.
//!
//! # The cross-workspace claim is asserted as a caller sees it
//!
//! `sql::REVOKE_GRANT` carries a join to `core.fleets` that the scope read
//! before it has already made redundant, and that redundancy is deliberate. Its
//! own half is proven where the statement text is reachable —
//! `integration_grants/workspace.zig` runs exactly this text with a foreign
//! workspace. What is proven HERE is the guarantee the two halves exist for: a
//! revoke naming somebody else's workspace changes no row.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_lane.rs"]
mod lane;

use afd_approval::{IntegrationGrants, Revocation};
use afd_core::id::Uuid7;
use afd_wire::grant::status;
use sqlx::Row as _;

use self::lane::{Lane, NOW_MS};

/// The instant a revoke in this suite stamps.
const REVOKED_AT_MS: i64 = NOW_MS + 5_000;

/// The services the fixture rows are raised for.
const SERVICE_SLACK: &str = "slack";
const SERVICE_GITHUB: &str = "github";
const SERVICE_ZOHO: &str = "zoho";

/// The reason the install records on every seeded row.
const REASON: &str = "Declared by the fleet bundle at install";

/// A workspace that exists in no fixture, for the scope refusals.
const FOREIGN_WORKSPACE: &str = "01900000-0000-7000-8000-0000000000ff";

/// Seeds one grant on `lane`'s fleet, returning its id.
///
/// `created_at` is a parameter because the list's whole claim is an ORDER, and
/// a fixture that fixed the instant could not express one.
async fn seed_grant(lane: &Lane, service: &str, state: &str, created_at: i64) -> Uuid7 {
    let id = lane::mint();
    let approved_at = (state == status::APPROVED).then_some(created_at + 1);
    let revoked_at = (state == status::REVOKED).then_some(created_at + 2);
    sqlx::query(
        "INSERT INTO core.integration_grants
           (id, fleet_id, service, status, requested_reason,
            approved_at, revoked_at, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id.as_str())
    .bind(lane.fleet.as_str())
    .bind(service)
    .bind(state)
    .bind(REASON)
    .bind(approved_at)
    .bind(revoked_at)
    .bind(created_at)
    .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("the grant row must insert");
    id
}

/// One column of one grant, as text, straight from the table.
async fn column_of(lane: &Lane, grant: &Uuid7, column: &str) -> Option<String> {
    // The column name is a literal from this suite, never input.
    let statement = sqlx::AssertSqlSafe(format!(
        "SELECT {column}::text FROM core.integration_grants WHERE id = $1::uuid"
    ));
    sqlx::query(statement)
        .bind(grant.as_str())
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the grant row is readable")
        .try_get::<Option<String>, _>(0)
        .expect("the column must be readable as text")
}

/// The store under test, over the lane's pool.
fn grants(lane: &Lane) -> IntegrationGrants {
    IntegrationGrants::new(lane.pool.clone())
}

/// A fleet with no grants and a fleet that is not there are different answers.
///
/// The distinction the whole return type exists for: collapsing them would make
/// a workspace's own un-granted fleet indistinguishable from somebody else's,
/// and the edge answers a different 404 for each.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_with_no_grants_is_not_a_fleet_that_is_not_there() {
    let lane = Lane::isolated().await;
    let store = grants(&lane);

    let held = store
        .page(&lane.workspace, &lane.fleet)
        .await
        .expect("the read must not fault");
    assert_eq!(
        held,
        Some(Vec::new()),
        "a fleet declaring no mintable credential holds no grants and still exists"
    );

    let absent = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a minted id is well formed");
    let missing = store
        .page(&lane.workspace, &absent)
        .await
        .expect("the read must not fault");
    assert_eq!(
        missing, None,
        "a fleet nobody installed is absent, not empty"
    );
}

/// The list carries every status, newest first.
///
/// Including the `pending` and `revoked` rows the runner plane is blind to.
/// That breadth is the point of the operator's surface: it shows what a person
/// has and has not answered, which is exactly the distinction a mint must not
/// be able to make.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_list_shows_every_status_newest_first() {
    let lane = Lane::isolated().await;
    seed_grant(&lane, SERVICE_SLACK, status::PENDING, NOW_MS).await;
    seed_grant(&lane, SERVICE_GITHUB, status::APPROVED, NOW_MS + 10).await;
    seed_grant(&lane, SERVICE_ZOHO, status::REVOKED, NOW_MS + 20).await;

    let held = grants(&lane)
        .page(&lane.workspace, &lane.fleet)
        .await
        .expect("the read must not fault")
        .expect("the lane's own fleet is present");

    let order: Vec<&str> = held.iter().map(|row| row.service.as_str()).collect();
    assert_eq!(
        order,
        vec![SERVICE_ZOHO, SERVICE_GITHUB, SERVICE_SLACK],
        "newest first, which is what a dashboard renders top-down"
    );
    let states: Vec<&str> = held.iter().map(|row| row.status.as_str()).collect();
    assert_eq!(
        states,
        vec![status::REVOKED, status::APPROVED, status::PENDING],
        "no status is filtered out of the operator's view"
    );

    // The two instants are the row's own transitions, and a pending row has
    // neither. Mapping them positionally is the one thing a `SELECT` reorder
    // would break silently, so it is asserted rather than assumed.
    let pending = held.last().expect("three rows were seeded");
    assert_eq!(pending.approved_at, None);
    assert_eq!(pending.revoked_at, None);
    assert_eq!(pending.created_at, NOW_MS);
    assert_eq!(pending.reason, REASON);
}

/// A revoke moves the row once, and says so once.
///
/// The second call changes nothing and reports [`Revocation::GrantAbsent`] —
/// not a second success. `status != 'revoked'` in the statement's predicate is
/// what decides that, without a read-then-write in front of it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_revoke_moves_the_row_once_and_reports_the_second_as_absent() {
    let lane = Lane::isolated().await;
    let grant = seed_grant(&lane, SERVICE_SLACK, status::APPROVED, NOW_MS).await;
    let store = grants(&lane);
    let now = afd_core::clock::UnixMillis::from_millis(REVOKED_AT_MS);

    let first = store
        .revoke(&lane.workspace, &lane.fleet, &grant, now)
        .await
        .expect("the revoke must not fault");
    assert_eq!(first, Revocation::Revoked);
    assert_eq!(
        column_of(&lane, &grant, "status").await.as_deref(),
        Some(status::REVOKED)
    );
    assert_eq!(
        column_of(&lane, &grant, "revoked_at").await,
        Some(REVOKED_AT_MS.to_string())
    );

    // The approval it once had survives, so the row still records that somebody
    // said yes before somebody took it back.
    assert_eq!(
        column_of(&lane, &grant, "approved_at").await,
        Some((NOW_MS + 1).to_string()),
        "revoking withdraws the permission, not the history of granting it"
    );

    let again = store
        .revoke(&lane.workspace, &lane.fleet, &grant, now)
        .await
        .expect("the second revoke must not fault");
    assert_eq!(
        again,
        Revocation::GrantAbsent,
        "the grant is already unusable; the caller is told so rather than told it landed twice"
    );
    assert_eq!(
        column_of(&lane, &grant, "revoked_at").await,
        Some(REVOKED_AT_MS.to_string()),
        "and the instant is not re-stamped by the loser"
    );
}

/// A grant id this fleet does not hold is absent, not an error.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_grant_this_fleet_does_not_hold_is_absent() {
    let lane = Lane::isolated().await;
    let stranger = lane::mint();

    let outcome = grants(&lane)
        .revoke(&lane.workspace, &lane.fleet, &stranger, Lane::now())
        .await
        .expect("the revoke must not fault");

    assert_eq!(outcome, Revocation::GrantAbsent);
}

/// A revoke naming somebody else's workspace changes no row.
///
/// The guarantee both scope checks exist for, asserted as a caller sees it: the
/// grant is real, the grant id is right, and the answer is still that this
/// workspace holds no such fleet — with the row exactly as it was found.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cross_workspace_revoke_changes_nothing() {
    let lane = Lane::isolated().await;
    let grant = seed_grant(&lane, SERVICE_GITHUB, status::APPROVED, NOW_MS).await;
    let foreign = Uuid7::parse(FOREIGN_WORKSPACE).expect("the fixture workspace id is well formed");

    let outcome = grants(&lane)
        .revoke(&foreign, &lane.fleet, &grant, Lane::now())
        .await
        .expect("the revoke must not fault");

    assert_eq!(
        outcome,
        Revocation::FleetAbsent,
        "a fleet outside the caller's workspace is absent, never forbidden — the \
         endpoint must not be an oracle for which fleets are real elsewhere"
    );
    assert_eq!(
        column_of(&lane, &grant, "status").await.as_deref(),
        Some(status::APPROVED),
        "and the row is untouched"
    );
    assert_eq!(column_of(&lane, &grant, "revoked_at").await, None);
}

/// The list refuses a foreign workspace the same way, and reads nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cross_workspace_list_discloses_nothing() {
    let lane = Lane::isolated().await;
    seed_grant(&lane, SERVICE_SLACK, status::APPROVED, NOW_MS).await;
    let foreign = Uuid7::parse(FOREIGN_WORKSPACE).expect("the fixture workspace id is well formed");

    let held = grants(&lane)
        .page(&foreign, &lane.fleet)
        .await
        .expect("the read must not fault");

    assert_eq!(
        held, None,
        "the fleet's grants are never disclosed to a workspace that does not hold it"
    );
}
