//! Who decided, by name, and the four ways that answer is legitimately empty.
//!
//! # Why these are proven against a live datastore and not a stub
//!
//! Every assertion here is a property of the correlated subquery inside
//! `RESOLVE_GATE`: which user row it matches at the instant of the decision,
//! and what a non-match writes. A stub would assert that the code runs a
//! statement, which was never in doubt. What is under test is what Postgres
//! does with it, and in particular that the tenant predicate excludes a row the
//! subject predicate alone would have matched.
//!
//! # Why the name is captured and not resolved
//!
//! `resolved_by` is an OIDC subject: right to store, wrong to show. Joining
//! `core.users` on the READ closes that gap and costs 157 shared buffers
//! against 7, because the subject index is searched once per row on every page
//! load. Capturing at the decision pays once, and it is the truer record — a
//! join reports who the account is now, so a rename rewrites history and a
//! deletion erases it.
//!
//! Before either, the dashboard asked the identity provider's admin API from
//! the browser: an instance-wide read outside `requireScope` and
//! `authorizeWorkspace`, for a string this database already held.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_approval::{Decision, Filter, Resolution};

use crate::lane::{Lane, NOW_MS, WINDOW_MS};

/// A subject this deployment has a user row for.
const KNOWN_SUBJECT: &str = "user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu";

/// The name that row carries.
const KNOWN_NAME: &str = "Ada Lovelace";

/// A subject with no user row — a person who never signed up here.
///
/// Distinct from [`FOREIGN_SUBJECT`] on purpose. `seed_person` upserts
/// `ON CONFLICT (oidc_subject) DO UPDATE`, the suite shares one database, and
/// `approval_suite.rs` runs these concurrently — so one subject across both
/// tests would let the no-row assertion pass through the tenant predicate
/// instead of through the absence it names.
const STRANGER_SUBJECT: &str = "user_2StRaNgErNoBoDyKnOwSaBoUtIt";

/// A subject whose only user row belongs to another tenant.
const FOREIGN_SUBJECT: &str = "user_4FoReIgNtEnAnTsUbJeCtHeRe";

/// Someone who signed up without a name: `display_name` is NULL, and the
/// address is the most human thing this deployment holds about them.
const NAMELESS_SUBJECT: &str = "user_5NaMeLeSsBuThAsAnAdDrEsS";

/// What that person's row carries instead of a name.
const NAMELESS_EMAIL: &str = "nameless@fixture.invalid";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// A page big enough that nothing under test is lost to the limit.
const WHOLE_PAGE: i64 = 50;

/// What the join produces when it matches nothing.
const NO_NAME: &str = "";

/// The name reaches the row the operator reads.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decider_with_a_user_row_reads_as_their_name() {
    let lane = Lane::isolated().await;
    seed_person(&lane, lane.tenant.as_str(), KNOWN_SUBJECT, KNOWN_NAME).await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    resolve_as(&lane, &action, KNOWN_SUBJECT).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    let row = page.first().expect("the resolved gate");
    assert_eq!(row.resolved_by, KNOWN_SUBJECT, "the subject is unchanged");
    assert_eq!(
        row.resolved_by_name, KNOWN_NAME,
        "the name joins from this deployment's own user row"
    );
}

/// A subject nothing knows about still shows as a decision, not as a blank.
///
/// This is the fallback the dashboard renders as the shortened subject. Dropping
/// the only record of who decided would be worse than printing it ugly, so the
/// read must return the subject with an empty name rather than skipping the row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decider_with_no_user_row_keeps_the_row_and_answers_no_name() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    resolve_as(&lane, &action, STRANGER_SUBJECT).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    let row = page.first().expect("the resolved gate");
    assert_eq!(
        row.resolved_by, STRANGER_SUBJECT,
        "an unknown decider is still a decider"
    );
    assert_eq!(row.resolved_by_name, NO_NAME, "and has no name to show");
}

/// The tenant predicate bites: another tenant's user never names this gate.
///
/// `uq_users_oidc_subject` is globally unique, so this arrangement cannot arise
/// through the product. It is constructed here precisely because the predicate
/// must hold on its own rather than on that argument — a later schema that
/// scoped the subject per tenant would silently turn the capture cross-tenant,
/// and this test is what fails when it does.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_user_row_in_another_tenant_does_not_name_this_gate() {
    let lane = Lane::isolated().await;
    let other_tenant = seed_other_tenant(&lane).await;
    seed_person(&lane, &other_tenant, FOREIGN_SUBJECT, KNOWN_NAME).await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    resolve_as(&lane, &action, FOREIGN_SUBJECT).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    assert_eq!(
        page.first().expect("the resolved gate").resolved_by_name,
        NO_NAME,
        "a name may only come from the tenant that owns the gate"
    );
}

/// A gate nobody has answered has no decider and therefore no name.
///
/// Nothing has run `RESOLVE_GATE` over this row, so the column holds slot 838's
/// DEFAULT. The waiting gate is still on the page: an absent name was never a
/// reason to drop a row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pending_gate_carries_neither_a_decider_nor_a_name() {
    let lane = Lane::isolated().await;
    lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    let row = page.first().expect("the pending gate stays on the page");
    assert_eq!(row.resolved_by, NO_NAME, "nobody has decided");
    assert_eq!(row.resolved_by_name, NO_NAME, "so nobody is named");
}

/// The single-gate read returns the same captured name the page does.
///
/// Two statements select this column and a fix applied to one of them is the
/// failure mode worth pinning: the detail page is where an operator looks when
/// the table's answer surprised them.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_single_gate_read_names_the_decider_too() {
    let lane = Lane::isolated().await;
    seed_person(&lane, lane.tenant.as_str(), KNOWN_SUBJECT, KNOWN_NAME).await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    resolve_as(&lane, &action, KNOWN_SUBJECT).await;

    let listed = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    let gate_id = afd_core::id::Uuid7::parse(&listed.first().expect("the resolved gate").gate_id)
        .expect("the read returns a well-formed gate id");

    let one = lane
        .inbox
        .one(&lane.workspace, &gate_id)
        .await
        .expect("the single-gate read must not fault")
        .expect("the gate this test just resolved");
    assert_eq!(one.resolved_by_name, KNOWN_NAME);
}

/// Someone with no name still reads as a person, not as an identifier.
///
/// `core.users.display_name` is written once at signup from the provider's
/// first and last name alone, so an account created without one has NULL there
/// permanently. The browser lookup this replaced fell back to the address, and
/// dropping that would show a shortened subject where a human string used to be.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decider_with_no_display_name_reads_as_their_address() {
    let lane = Lane::isolated().await;
    seed_nameless_person(&lane, NAMELESS_SUBJECT, NAMELESS_EMAIL).await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    resolve_as(&lane, &action, NAMELESS_SUBJECT).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    assert_eq!(
        page.first().expect("the resolved gate").resolved_by_name,
        NAMELESS_EMAIL,
        "a nameless account falls back to its address, as the deleted lookup did"
    );
}

/// One user row carrying an address and no name at all.
async fn seed_nameless_person(lane: &Lane, subject: &str, email: &str) {
    sqlx::query(
        "INSERT INTO core.users
           (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, NULL, $5, $5)
         ON CONFLICT (oidc_subject) DO UPDATE
           SET tenant_id = EXCLUDED.tenant_id, display_name = NULL",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(lane.tenant.as_str())
    .bind(subject)
    .bind(email)
    .bind(NOW_MS)
    .execute(&mut *connection(lane).await)
    .await
    .expect("seeding a nameless person");
}

/// Answers `action` as `subject`, asserting the decision landed.
async fn resolve_as(lane: &Lane, action: &str, subject: &str) {
    let resolved = lane
        .inbox
        .resolve(
            action,
            Decision::Approved,
            subject,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");
    assert!(
        matches!(resolved, Resolution::Resolved(_)),
        "the fixture gate must be the one this resolve decided"
    );
}

/// One user row under `tenant`, idempotent across a shared database.
///
/// `ON CONFLICT (oidc_subject)` rather than on the primary key: the subject is
/// what the join reads and what a re-run collides on.
async fn seed_person(lane: &Lane, tenant: &str, subject: &str, name: &str) {
    sqlx::query(
        "INSERT INTO core.users
           (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)
         ON CONFLICT (oidc_subject) DO UPDATE
           SET tenant_id = EXCLUDED.tenant_id, display_name = EXCLUDED.display_name",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(tenant)
    .bind(subject)
    .bind(format!("{subject}@fixture.invalid"))
    .bind(name)
    .bind(NOW_MS)
    .execute(&mut *connection(lane).await)
    .await
    .expect("seeding a person");
}

/// A second tenant, so a cross-tenant user row can exist to be excluded.
async fn seed_other_tenant(lane: &Lane) -> String {
    let tenant = afd_db::test_util::mint_id();
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, $1::text, $2, $2)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&tenant)
    .bind(NOW_MS)
    .execute(&mut *connection(lane).await)
    .await
    .expect("seeding the other tenant");
    tenant
}

/// One pooled connection. The lane's own is private to its module.
async fn connection(lane: &Lane) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    lane.pool
        .acquire()
        .await
        .expect("the fixture database must answer")
}
