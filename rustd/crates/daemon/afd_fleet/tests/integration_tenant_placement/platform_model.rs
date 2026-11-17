use super::*;

/// Work on the platform model never reaches a
/// tenant runner, and a platform runner still takes it.
// Value: protects=the platform key stays on platform hosts; fails_when=the
// head's model test is dropped from the scan or the claim; why_new=the model filter;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn platform_model_work_never_reaches_a_tenant_runner() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t
        .fleet(&fixtures, afd_credential::provider::PLATFORM_MODEL_REF)
        .await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;

    for poll in 1..=2 {
        assert!(
            select_fleet(&leases, &t_runner, &fleet).await.is_none(),
            "poll {poll}: no candidate"
        );
    }
    assert!(!slotted(&fixtures, &fleet).await, "no slot write");
    assert!(
        !claim_head(&fixtures, &fleet, &t_runner).await,
        "the claim alone refuses too"
    );
    assert_eq!(leases_of(&fixtures, &t_runner).await, 0, "no lease");

    let p_runner = platform_runner(&fixtures, &t.tag).await;
    let held = select_fleet(&leases, &p_runner, &fleet)
        .await
        .expect("a platform runner takes it");
    assert!(issue(&fixtures, &leases, &p_runner, &held, &t.tenant, PLATFORM).await);
    fixtures.cleanup().await;
}

/// A tenant host is refused platform-posture work
/// under the host lock, even when handed it directly.
// Value: protects=issue's posture re-check; fails_when=`tenant_owned` stops
// reaching issue; why_new=the posture re-check; seam=an Acquired handed straight to issue.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn issue_refuses_platform_posture_on_a_tenant_runner() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let held = select_fleet(&leases, &t_runner, &fleet)
        .await
        .expect("T's runner claims");

    let captured = af_core::test_util::trace::Capture::install();
    let issued = issue(&fixtures, &leases, &t_runner, &held, &t.tenant, PLATFORM).await;
    let refused = captured.only("runner_placement_refused");
    drop(captured);

    assert!(!issued, "platform posture never issues to a tenant host");
    assert_eq!(leases_of(&fixtures, &t_runner).await, 0);
    assert_eq!(refused.field("reason"), Some("platform_model"));
    fixtures.cleanup().await;
}

/// A native setup a fleet can be moved onto; `native_config` saves it on the
/// platform model.
const PLATFORM_CONFIG: &str = r#"{"name":"moved","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// Runs one statement about `fleet` or an admission, on a connection borrowed
/// for that statement.
async fn execute(fixtures: &Fixtures, statement: &'static str, id: &Uuid7) {
    sqlx::query(statement)
        .bind(id)
        .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the fixture statement runs");
}

// Value: protects=a hold keeps a fleet for its tenant holder even after its
// head moves to the platform model, and only its expiry frees the fleet for a
// platform runner; fails_when=the model filter or the hold rule drops the
// other; why_new=the model filter; seam=the slot's hold, set as a report leaves it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn held_fleet_on_the_platform_model_waits_for_its_hold() {
    let fixtures = Fixtures::create().await;
    let leases = fixtures.leases();
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let first = select_fleet(&leases, &t_runner, &fleet)
        .await
        .expect("T's runner takes its fleet");
    assert!(
        issue(
            &fixtures,
            &leases,
            &t_runner,
            &first,
            &t.tenant,
            SELF_MANAGED
        )
        .await
    );
    execute(
        &fixtures,
        "UPDATE fleet.admissions SET settled_at = clock_timestamp() WHERE id = $1",
        &first.event_id,
    )
    .await;
    execute(
        &fixtures,
        "UPDATE runner.fleet_slots SET lease_expires_at = clock_timestamp() - interval '1 second', \
         hold_expires_at = clock_timestamp() + interval '10 minutes' WHERE fleet_id = $1",
        &fleet,
    )
    .await;

    fixtures.native_config(&fleet, PLATFORM_CONFIG).await;
    crate::queue::enqueue(
        &fixtures.database,
        &fleet,
        &t.workspace,
        crate::seed::ACTOR,
        "chat",
        crate::seed::REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
    let p_runner = platform_runner(&fixtures, &t.tag).await;
    assert!(
        select_fleet(&leases, &p_runner, &fleet).await.is_none(),
        "the tenant holder's live hold keeps the fleet"
    );
    assert!(
        select_fleet(&leases, &t_runner, &fleet).await.is_none(),
        "and the holder cannot take platform-model work"
    );

    execute(
        &fixtures,
        "UPDATE runner.fleet_slots SET hold_expires_at = clock_timestamp() - interval '1 second' \
         WHERE fleet_id = $1",
        &fleet,
    )
    .await;
    let taken = select_fleet(&leases, &p_runner, &fleet)
        .await
        .expect("once the hold expires, a platform runner takes the fleet");
    assert!(issue(&fixtures, &leases, &p_runner, &taken, &t.tenant, PLATFORM).await);
    fixtures.cleanup().await;
}
