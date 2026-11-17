use super::*;

/// work on the platform model never reaches a
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

/// a tenant host is refused platform-posture work
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
