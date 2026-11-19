use super::*;

/// The live matrix matches the rule.
// Value: protects=a tenant runner takes only its tenant's work and a platform
// runner any; fails_when=the scan or claim drops the owner, or refuses
// everything; why_new=the ownership filter; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn cross_tenant_placement_matrix() {
    let fixtures = Fixtures::create().await;
    let runs = fixtures.runs();
    let t = Owner::new();
    let [own, shared, foreign] = [
        t.fleet(&fixtures, TENANT_MODEL).await,
        t.fleet(&fixtures, TENANT_MODEL).await,
        t.fleet(&fixtures, TENANT_MODEL).await,
    ];
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let p_runner = platform_runner(&fixtures, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;

    let held = select_fleet(&runs, &t_runner, &own)
        .await
        .expect("T's runner takes T's work");
    assert!(issue(&fixtures, &runs, &t_runner, &held, &t.tenant, SELF_MANAGED).await);
    let held = select_fleet(&runs, &p_runner, &shared)
        .await
        .expect("a platform runner takes any tenant's");
    assert!(issue(&fixtures, &runs, &p_runner, &held, &t.tenant, SELF_MANAGED).await);

    assert!(
        select_fleet(&runs, &u_runner, &foreign).await.is_none(),
        "U's runner sees no candidate"
    );
    assert!(!slotted(&fixtures, &foreign).await, "and writes no slot");
    assert_eq!(runs_of(&fixtures, &u_runner).await, 0, "and holds no run");
    fixtures.cleanup().await;
}

/// The claim alone refuses a foreign runner.
// Value: protects=the claim statement is the control, not the scan;
// fails_when=the claim's `fleet` step stops joining the runner; why_new=the claim filter;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn claim_statement_refuses_a_foreign_runner() {
    let fixtures = Fixtures::create().await;
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;

    assert!(
        claim_head(&fixtures, &fleet, &t_runner).await,
        "T's runner claims"
    );
    lapse(&fixtures, &fleet).await;
    let before = slot(&fixtures, &fleet).await;
    assert!(
        !claim_head(&fixtures, &fleet, &u_runner).await,
        "U's runner is refused"
    );
    assert_eq!(
        slot(&fixtures, &fleet).await,
        before,
        "the slot, its fence and deadline are untouched"
    );
    assert!(
        claim_head(&fixtures, &fleet, &t_runner).await,
        "and T's runner still claims"
    );
    assert_eq!(
        slot(&fixtures, &fleet).await.0,
        before.0 + 1,
        "with the next fence"
    );
    fixtures.cleanup().await;
}

/// Issue refuses a foreign runner under the host lock.
// Value: protects=issue holds even when the claim lets a placement through;
// fails_when=the host lock stops binding the work's tenant; why_new=the issue
// re-check; seam=an Acquired handed straight to issue.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn issue_refuses_a_foreign_runner_under_the_host_lock() {
    let fixtures = Fixtures::create().await;
    let runs = fixtures.runs();
    let t = Owner::new();
    let fleet = t.fleet(&fixtures, TENANT_MODEL).await;
    let u = fixtures.seed_tenant().await;
    let t_runner = tenant_runner(&fixtures, &t.tenant, &t.tag).await;
    let u_runner = tenant_runner(&fixtures, &u, &t.tag).await;
    let held = select_fleet(&runs, &t_runner, &fleet)
        .await
        .expect("T's runner claims");

    let captured = af_core::test_util::trace::Capture::install();
    let issued = issue(&fixtures, &runs, &u_runner, &held, &t.tenant, SELF_MANAGED).await;
    let refused = captured.only("runner_placement_refused");
    drop(captured);

    assert!(!issued, "a foreign host gets no run");
    assert_eq!(runs_of(&fixtures, &u_runner).await, 0);
    assert_eq!(refused.field("error_code"), Some("UZ-RUN-027"));
    assert_eq!(refused.field("reason"), Some("foreign_tenant"));
    assert_eq!(refused.field("run_type"), Some("fleet"));
    fixtures.cleanup().await;
}
