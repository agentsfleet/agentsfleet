//! Against a live datastore: a runner's owner is stated at registration and
//! fixed after it, a tenant runner holds no decision model, and a tenant's
//! erasure takes its runners and leaves its receipts.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::rows::Row;
use crate::support::Fixtures;
use af_core::id::Uuid7;

/// the tenant cannot change.
///
/// A tenant runner flipped to NULL would become a platform runner and receive
/// every tenant's work, so the trigger refuses it. A platform runner's other
/// columns still update, so a trigger that refuses every update fails here.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn runner_tenant_cannot_change() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.seed_tenant().await;
    let other = fixtures.seed_tenant().await;
    let tenant_runner = fixtures.insert_runner(Some(&tenant)).await;
    let platform_runner = fixtures.insert_runner(None).await;

    for (runner, to, what) in [
        (&tenant_runner, None, "flipping a tenant runner to NULL"),
        (
            &tenant_runner,
            Some(&other),
            "handing a tenant runner to another tenant",
        ),
        (
            &platform_runner,
            Some(&tenant),
            "claiming a platform runner for a tenant",
        ),
    ] {
        let refused = fixtures
            .set_runner_tenant(runner, to)
            .await
            .expect_err(what);
        assert!(
            refused.to_string().contains("fixed at registration"),
            "{what} must be refused by the trigger, got: {refused}"
        );
    }
    assert_eq!(
        fixtures
            .value::<Uuid7>(Row::Runner(&tenant_runner), "tenant_id")
            .await,
        Some(tenant.clone()),
        "the row still names its tenant"
    );
    assert_eq!(
        fixtures
            .value::<Uuid7>(Row::Runner(&platform_runner), "tenant_id")
            .await,
        None,
        "and a platform runner is still a platform runner"
    );

    fixtures
        .rename_runner_host(&platform_runner, "renamed")
        .await;
    assert_eq!(
        fixtures
            .text(Row::Runner(&platform_runner), "host_id")
            .await
            .as_deref(),
        Some("renamed"),
        "an update that leaves tenant_id alone still lands"
    );

    fixtures.delete_runner(&platform_runner).await;
    fixtures.delete_tenant(&tenant).await;
    fixtures.delete_tenant(&other).await;
    fixtures.cleanup().await;
}

/// Ownership is stated, never inferred from a missing tenant.
///
/// A platform runner takes every tenant's work, so a row that is neither, or
/// both, must not exist, and a stored row's flag must not flip.
// Value: protects=a missing tenant never reads as the platform; fails_when=the
// check is dropped or the trigger misses the flag; why_new=`platform_owned`;
// seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn runner_ownership_is_stated_and_consistent() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.seed_tenant().await;

    for (platform_owned, owner, what) in [
        (false, None, "a tenant runner with no tenant"),
        (true, Some(&tenant), "a platform runner with a tenant"),
    ] {
        let refused = fixtures
            .insert_owned_runner(platform_owned, owner)
            .await
            .expect_err(what);
        assert!(
            refused.to_string().contains("ck_runners_owner"),
            "{what} must be refused by the ownership check, got: {refused}"
        );
    }

    let tenant_runner = fixtures.insert_runner(Some(&tenant)).await;
    let platform_runner = fixtures.insert_runner(None).await;
    for (runner, to, what) in [
        (
            &tenant_runner,
            true,
            "promoting a tenant runner to the platform",
        ),
        (&platform_runner, false, "demoting a platform runner"),
    ] {
        let refused = fixtures
            .set_runner_platform_owned(runner, to)
            .await
            .expect_err(what);
        assert!(
            refused.to_string().contains("fixed at registration"),
            "{what} must be refused by the trigger, got: {refused}"
        );
    }

    fixtures.delete_runner(&platform_runner).await;
    fixtures.delete_tenant(&tenant).await;
    fixtures.cleanup().await;
}

/// a tenant runner holds no decision model.
///
/// A platform decision model hands the platform decision key to a host its
/// tenant holds root on. A platform runner still takes one, so a check that
/// refuses every assignment fails here.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn tenant_runner_refuses_a_decision_model() {
    let fixtures = Fixtures::create().await;
    let tenant = fixtures.seed_tenant().await;
    let tenant_runner = fixtures.insert_runner(Some(&tenant)).await;
    let platform_runner = fixtures.insert_runner(None).await;
    let model = fixtures.seed_platform_model().await;

    let refused = fixtures
        .set_runner_decision_model(&tenant_runner, &model)
        .await
        .expect_err("a tenant runner must not take a decision model");
    assert!(
        refused
            .to_string()
            .contains("ck_runners_tenant_no_decision_model"),
        "the table check must be what refused, got: {refused}"
    );
    fixtures
        .set_runner_decision_model(&platform_runner, &model)
        .await
        .expect("a platform runner still takes one, so the check is not a blanket refusal");

    fixtures.delete_runner(&platform_runner).await;
    fixtures.delete_tenant(&tenant).await;
    fixtures.remove_platform_model(&model).await;
    fixtures.cleanup().await;
}

/// erasure takes only the tenant's runners.
///
/// The receipt stays: it holds opaque identifiers and a digest, and a key
/// retried after its runner was erased must still enrol nothing.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn tenant_erasure_takes_its_runners() {
    let fixtures = Fixtures::create().await;
    let a = fixtures.seed_tenant().await;
    let b = fixtures.seed_tenant().await;
    let a_first = fixtures.insert_runner(Some(&a)).await;
    let a_second = fixtures.insert_runner(Some(&a)).await;
    let b_runner = fixtures.insert_runner(Some(&b)).await;
    let platform = fixtures.insert_runner(None).await;
    fixtures.insert_registration_receipt(&a, &a_first).await;
    let hosts = [a_first, a_second, b_runner.clone(), platform.clone()];

    fixtures.delete_tenant(&a).await;

    let mut survivors = fixtures.surviving_runners(&hosts).await;
    let mut expected = vec![b_runner, platform.clone()];
    survivors.sort();
    expected.sort();
    assert_eq!(survivors, expected, "only A's two runners are gone");
    assert_eq!(
        fixtures.registration_receipts(&a).await,
        1,
        "A's receipt outlives its runner and its tenant"
    );

    fixtures.delete_runner(&platform).await;
    fixtures.delete_tenant(&b).await;
    fixtures.cleanup().await;
}
