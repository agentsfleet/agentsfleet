//! Live credential-mint behavior and ordering cases.

use super::*;
use crate::requests;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_mint_scope_is_the_presenting_runners_lease() {
    // Invariant 2, and the whole reason the wire carries no workspace: a
    // prompt-injected child has nothing to forge, because a lease that is not
    // this runner's resolves to NO ROW rather than to another tenant's
    // workspace.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    let stranger = bound(&fixtures, "{}").await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(NOW_MS);

    let scope = leases
        .mint_scope(&owner.runner, &owner.lease, now)
        .await
        .expect("the read must succeed")
        .expect("the owner's own lease resolves");
    assert_eq!(scope.workspace_id.as_str(), owner.workspace);
    assert_eq!(scope.fleet_id.as_str(), owner.fleet);
    assert_eq!(&*scope.event_id, EVENT_ID);

    // The IDOR negative: a real, live lease belonging to somebody else.
    assert!(
        leases
            .mint_scope(&stranger.runner, &owner.lease, now)
            .await
            .expect("the read must succeed")
            .is_none(),
        "a foreign lease resolved to a scope"
    );

    // And the lease's own lifetime bounds the authority: past its expiry the
    // same runner presenting the same id resolves to nothing.
    let expired = UnixMillis::from_millis(NOW_MS + LEASE_WINDOW_MS + 1);
    assert!(
        leases
            .mint_scope(&owner.runner, &owner.lease, expired)
            .await
            .expect("the read must succeed")
            .is_none(),
        "an expired lease still authorised a mint"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_static_handle_mints_from_the_leases_own_workspace() {
    // The positive, and the scope proof: the token's VALUE is what
    // distinguishes the owner's workspace from any other, so a mint that
    // resolved the wrong workspace fails here rather than passing on presence.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;

    let minted = fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_STATIC),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("a connected static handle mints");
    assert_eq!(minted.token.as_str(), OWNER_TOKEN);
    assert!(
        minted.rotated_refresh_token.is_none(),
        "a static handle rotates nothing"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_the_grant_gate_refuses_before_the_vault_is_opened() {
    // The ordering property no unit test can reach: an ungranted request must
    // not touch credential bytes. Proven by connecting the integration and
    // withholding only the grant — if the gate ran after the vault read, this
    // would surface as a successful mint.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_GITHUB,
        r#"{"integration":"github","installation_id":"42"}"#,
    )
    .await;

    for withheld in ["pending", "revoked"] {
        seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, withheld).await;
        let refusal = fixtures
            .plane()
            .mint(
                &owner.runner,
                &requests::mint(&owner.lease, CONNECTOR_GITHUB),
                UnixMillis::from_millis(NOW_MS),
            )
            .await
            .expect_err("an ungranted integration must not mint");
        assert_eq!(
            refusal.code(),
            error_code::GRANT_NOT_FOUND,
            "a {withheld} grant was treated as an approval"
        );
    }

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_an_unconnected_integration_is_not_a_grant_failure() {
    // The two refusals a runner must be able to tell apart: nobody approved it,
    // versus nobody connected it. Approving the grant and storing no handle
    // isolates the second.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, "approved").await;

    let refusal = fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_GITHUB),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect_err("an unconnected integration must not mint");
    assert_eq!(refusal.code(), error_code::CRED_INTEGRATION_NOT_CONNECTED);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_two_concurrent_mints_cannot_spend_one_approval_twice() {
    // The race the whole `FOR UPDATE` + guarded-update pair exists for, and the
    // one thing no unit test can observe: with a single request left on an
    // approval, two simultaneous reservations must produce exactly one approval
    // and one exhaustion. A check-then-write without the lock passes this test
    // by luck most of the time, which is why the assertion is on the PAIR
    // rather than on either outcome alone.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    let _gate = seed_write_gate(
        &fixtures,
        &owner.fleet,
        STATED_BINDING,
        WRITE_SPEND_CEILING - 1,
    )
    .await;

    let gates = std::sync::Arc::new(fixtures.gates());
    let fleet = Uuid7::parse(&owner.fleet).expect("a seeded fleet id parses");
    let contenders: Vec<_> = (0..2)
        .map(|_| {
            let gates = std::sync::Arc::clone(&gates);
            let fleet = fleet.clone();
            tokio::spawn(async move {
                gates
                    .reserve_write_approval(&fleet, EVENT_ID, &declared())
                    .await
                    .expect("the reservation read must succeed")
            })
        })
        .collect();

    let mut verdicts = Vec::with_capacity(2);
    for contender in contenders {
        verdicts.push(contender.await.expect("a contender ran to completion"));
    }
    verdicts.sort_by_key(|verdict| format!("{verdict:?}"));
    assert_eq!(
        verdicts,
        vec![WriteApproval::Approved, WriteApproval::Exhausted],
        "the last request on an approval was spent twice"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_reach_the_fleet_no_longer_declares_is_refused_as_drift() {
    // The approval-to-mint drift, against real rows: the card recorded one
    // repository and the fleet now declares another. What makes this worth a
    // datastore is that BOTH sides are stored — the gate's `stated_binding` and
    // the fleet's `config_json` — and the refusal is the comparison between
    // them.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    let _gate = seed_write_gate(
        &fixtures,
        &owner.fleet,
        r#"{"repositories":["acme/other"],"access":"write","base":"main"}"#,
        0,
    )
    .await;

    let fleet = Uuid7::parse(&owner.fleet).expect("a seeded fleet id parses");
    let verdict = fixtures
        .gates()
        .reserve_write_approval(&fleet, EVENT_ID, &declared())
        .await
        .expect("the reservation read must succeed");
    assert_eq!(verdict, WriteApproval::BindingDrift);

    assert_eq!(spent_count(&fixtures, &owner.fleet).await, 0);
    fixtures.cleanup().await;
}

async fn spent_count(fixtures: &Fixtures, fleet: &str) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    // Keyed on the FLEET as well as the event: `EVENT_ID` is one constant every
    // test here seeds under, so on one shared database a read on it alone hands
    // `fetch_one` somebody else's gate — the concurrent-spend test's, sitting at
    // the ceiling. The fleet is minted per test, so the pair names our own row.
    sqlx::query_scalar::<_, i64>(
        "SELECT spend_count FROM core.fleet_approval_gates \
         WHERE event_id = $1 AND fleet_id = $2::uuid",
    )
    .bind(EVENT_ID)
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("the fleet's gate row is readable")
}

#[tokio::test]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_delivery_uses_only_an_approved_matching_write_gate() {
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    let fleet = Uuid7::parse(&owner.fleet).expect("a seeded fleet id parses");
    let gates = fixtures.gates();

    assert_eq!(
        gates
            .approved_write_gate(&fleet, EVENT_ID, &declared())
            .await
            .expect("an absent gate is not a datastore failure"),
        None
    );

    let gate_id = seed_write_gate(&fixtures, &owner.fleet, STATED_BINDING, 0).await;
    assert_eq!(
        gates
            .approved_write_gate(&fleet, EVENT_ID, &declared())
            .await
            .expect("the approved gate lookup succeeds")
            .as_ref()
            .map(Uuid7::as_str),
        Some(gate_id.as_str()),
        "the exact reach a human approved supplies the repair branch identity"
    );

    let drifted = RepositoryBinding::from_parts(
        vec!["acme/another".into()],
        Access::Write,
        Some("main".into()),
    );
    assert_eq!(
        gates
            .approved_write_gate(&fleet, EVENT_ID, &drifted)
            .await
            .expect("binding drift is a decision, not a datastore failure"),
        None,
        "a changed repository set cannot reuse an older approval"
    );

    fixtures.cleanup().await;
}
