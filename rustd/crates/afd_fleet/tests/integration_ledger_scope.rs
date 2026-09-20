//! One event id, two fleets, two bills.
//!
//! # The invariant, stated as money
//!
//! `event_id` is a LOGICAL id — the `<millis>-<seq>` string an admission mints
//! — and until slot 916 the ledger arbitrated a charge by `(event_id,
//! charge_type)` alone. Two fleets holding one such string therefore shared a
//! row, and the accumulate arms in `renew.rs` and `report.rs` added one
//! fleet's spend to the other's. Nothing read would have noticed: the row is
//! well-formed, the totals are plausible, and the only evidence is a bill that
//! does not match the work.
//!
//! That the ids do not collide today rests on the admission sequence being
//! global. That is true and it is not a billing guarantee anybody declared —
//! it lives in a sequence's implementation. Slot 916 moves the guarantee into
//! the key, and these are the proofs that it did.
//!
//! # Why the charges are driven, not inserted
//!
//! `Accounts::debit_receive` is the production write. A fixture issuing its
//! own `INSERT` would prove that the fixture's SQL respects the constraint,
//! which is not the claim. The one test here that does insert directly is the
//! refusal, because no typed caller can express a null fleet — which is the
//! whole point of the column being `NOT NULL`, and why the constraint rather
//! than a caller is what has to be asked.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
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

/// PostgreSQL's `not_null_violation`.
const NOT_NULL_VIOLATION: &str = "23502";

/// Charges `event` against `fleet`, the way the pull path does.
async fn charge(held: &Held, fleet: &Uuid7, workspace: &Uuid7, event: &str) -> bool {
    let tenant = Uuid7::parse(&held.tenant).expect("the fixture tenant is a v7 spelling");
    Accounts::new(held.fixtures.database.clone(), Entropy::new())
        .debit_receive(
            Charged {
                tenant_id: &tenant,
                workspace_id: workspace,
                fleet_id: fleet,
                event_id: event,
                posture: Posture::Platform,
                model: MODEL,
                event_created_at: held.now,
            },
            held.now,
        )
        .await
        .is_ok()
}

/// The workspace the fixture's fleet belongs to.
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
            .expect("the fixture fleet must be readable")
            .try_get(0)
            .expect("workspace_id decodes as text");
    Uuid7::parse(&workspace).expect("the fixture workspace is a v7 spelling")
}

/// A second fleet in the same workspace, so one tenant holds both.
///
/// Same workspace deliberately: a collision across two tenants would also be
/// caught by the tenant column, and a reader could conclude the fleet was not
/// carrying its weight in the key. Inside one workspace the fleet is the only
/// thing telling the two rows apart.
async fn second_fleet(held: &Held) -> Uuid7 {
    let sibling = Uuid7::parse(&afd_db::test_util::mint_id())
        .expect("the lane's minted identifier is a v7 spelling");
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         SELECT $1::uuid, f.workspace_id, f.tenant_id, 'ledger-scope-sibling',
                '# fixture', '{}'::jsonb, f.status, f.created_at, f.updated_at
         FROM core.fleets f WHERE f.id = $2::uuid",
    )
    .bind(sibling.as_str())
    .bind(&held.fleet)
    .execute(&mut *connection)
    .await
    .expect("seeding a sibling fleet");
    sibling
}

/// What each fleet's row under `event` was charged.
async fn charges_for(held: &Held, event: &str) -> Vec<(String, i64)> {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "SELECT fleet_id::text, credit_deducted_nanos
         FROM billing.usage_ledger
         WHERE tenant_id = $1::uuid AND event_id = $2 AND charge_type = 'receive'
         ORDER BY fleet_id",
    )
    .bind(&held.tenant)
    .bind(event)
    .fetch_all(&mut *connection)
    .await
    .expect("reading the ledger")
    .iter()
    .map(|row| {
        (
            row.try_get::<String, _>(0).expect("fleet_id decodes"),
            row.try_get::<i64, _>(1).expect("credit decodes"),
        )
    })
    .collect()
}

/// Dimension 2.3. Two fleets charged under one event id hold two rows.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn same_event_id_two_fleets_two_rows() {
    let held = held().await;
    let workspace = workspace_of(&held).await;
    let first = Uuid7::parse(&held.fleet).expect("the fixture fleet is a v7 spelling");
    let second = second_fleet(&held).await;
    let shared = format!("{}-shared", held.event_id);

    assert!(charge(&held, &first, &workspace, &shared).await);
    assert!(charge(&held, &second, &workspace, &shared).await);

    let rows = charges_for(&held, &shared).await;
    assert_eq!(
        rows.len(),
        2,
        "one event id held by two fleets must bill each of them: {rows:#?}"
    );
    let fleets: Vec<&str> = rows.iter().map(|(fleet, _)| fleet.as_str()).collect();
    assert!(
        fleets.contains(&held.fleet.as_str()),
        "the first fleet is billed"
    );
    assert!(
        fleets.contains(&second.as_str()),
        "the second fleet is billed"
    );
    let (_, first_amount) = rows.first().expect("two rows were just asserted");
    let (_, second_amount) = rows.get(1).expect("two rows were just asserted");
    assert_eq!(
        first_amount, second_amount,
        "each fleet pays its own receive charge, neither the sum of both"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 2.5. A redelivered receive still writes nothing.
///
/// The narrower key must not have widened what counts as a replay: the same
/// fleet charging the same event twice is still one row, which is the whole
/// reason the receive insert carries `DO NOTHING`.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn receive_insert_dedups_per_fleet_event() {
    let held = held().await;
    let workspace = workspace_of(&held).await;
    let fleet = Uuid7::parse(&held.fleet).expect("the fixture fleet is a v7 spelling");
    let replayed = format!("{}-replayed", held.event_id);

    assert!(charge(&held, &fleet, &workspace, &replayed).await);
    assert!(charge(&held, &fleet, &workspace, &replayed).await);

    assert_eq!(
        charges_for(&held, &replayed).await.len(),
        1,
        "a redelivery of one fleet's event is still one charge"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 2.6. A charge with no fleet is refused by the column.
///
/// Inserted directly because `Charged::fleet_id` is a `&Uuid7` and no caller
/// can pass nothing. That is the reason the guarantee has to live in the
/// schema: a future writer reaching this table outside the typed path — a
/// backfill, a repair script — is exactly what `NOT NULL` is holding the line
/// against, and it would otherwise write a row that escapes the arbiter
/// entirely, because Postgres treats NULLs as distinct in a unique index.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn ledger_refuses_null_fleet() {
    let held = held().await;
    let workspace = workspace_of(&held).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");

    let refused = sqlx::query(
        "INSERT INTO billing.usage_ledger
           (id, tenant_id, workspace_id, fleet_id, event_id, charge_type,
            posture, model, credit_deducted_nanos,
            event_created_at, created_at, last_charged_at)
         VALUES (gen_random_uuid(), $1::uuid, $2::uuid, NULL, $3, 'receive',
                 'platform', $4, 1, $5, $5, $5)",
    )
    .bind(&held.tenant)
    .bind(workspace.as_str())
    .bind(format!("{}-nullfleet", held.event_id))
    .bind(MODEL)
    .bind(held.now.as_millis())
    .execute(&mut *connection)
    .await;

    let error = refused.expect_err("a charge naming no fleet must be refused");
    let code = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .map(std::borrow::Cow::into_owned)
        .unwrap_or_default();
    assert_eq!(
        code, NOT_NULL_VIOLATION,
        "the refusal must be the column's, not an incidental fault: {error}"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A budget deep enough that forty renewals cannot exhaust it.
///
/// The ceiling is not what this test is about, and a run that halted
/// mid-way would assert a smaller sum and still pass.
const DEEP_DAILY_BUDGET: &str = r#"{"name":"ledger-scope","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1000}}}"#;

/// How many renewals one run makes here.
///
/// A live run renews every ~25 seconds, so forty is an unremarkable hour. The
/// number matters because the invariant is about REPETITION: one renewal
/// proves the insert, and only many prove the conflict arm never multiplies.
const RENEWALS: usize = 40;

/// Raises the fleet's ceiling so the renewals below are not budget-gated.
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
        .expect("the fixture fleet is funded");
}

/// Meters one renewal through the production plane.
async fn renew(held: &Held) {
    let plane = held.fixtures.plane();
    plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            afd_wire::report::RenewRequest::default(),
            held.now,
        )
        .await
        .expect("a funded lease renews");
    drop(plane);
}

/// The stage rows this fleet's event holds, and what they total.
async fn stage_rows(held: &Held) -> (i64, i64) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT count(*), COALESCE(SUM(credit_deducted_nanos), 0)::bigint
         FROM billing.usage_ledger
         WHERE fleet_id = $1::uuid AND event_id = $2 AND charge_type = 'stage'",
    )
    .bind(&held.fleet)
    .bind(&held.event_id)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the stage rows");
    (
        row.try_get::<i64, _>(0).expect("count decodes"),
        row.try_get::<i64, _>(1).expect("sum decodes"),
    )
}

/// Dimension 2.4. Repeated renewals accumulate into one stage row per fleet.
///
/// The regression the narrowed arbiter could most easily have caused. The
/// accumulate arm's whole job is that a renewal finds the row the last one
/// left and adds to it; an arbiter that stopped matching would insert a fresh
/// row per tick instead, and the table would grow without bound on every live
/// run while the totals still looked plausible row by row.
///
/// Forty rather than two, because two proves the conflict arm fires once and
/// says nothing about it continuing to fire.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn renewal_accumulates_per_fleet_event() {
    let held = held().await;
    fund(&held).await;

    for _ in 0..RENEWALS {
        renew(&held).await;
    }

    let (rows, total) = stage_rows(&held).await;
    assert_eq!(
        rows, 1,
        "forty renewals must accumulate into ONE stage row — a row per tick is \
         a table that grows with a run's length"
    );
    // Not asserted to have MOVED: the metered amount is the renewal request's
    // own token deltas, and `RenewRequest::default()` carries none. A test
    // that demanded a non-zero total would be asserting the fixture's payload
    // rather than the conflict arm this file is about.
    assert!(
        total >= 0,
        "the accumulated row must decode as a charge, whatever it totals"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
