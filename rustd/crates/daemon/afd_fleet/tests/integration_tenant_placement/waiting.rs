//! The offer probe: one statement per wake that names the connected tenant
//! runners whose tenant has nothing waiting, so the pass skips them.

use super::*;
use afd_fleet::lease::waiting::waiting;

/// The probe names exactly the tenant runners with nothing to take.
// Value: protects=a wake skips only a tenant runner whose tenant has no work
// waiting; fails_when=the probe names a runner whose tenant has an admission
// waiting, names a platform runner, or misses an idle tenant runner;
// why_new=the idle skip; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_probe_names_only_the_tenant_runners_with_nothing_waiting() {
    let fixtures = Fixtures::create().await;
    let busy = Owner::new();
    busy.fleet(&fixtures, TENANT_MODEL).await;
    let quiet = fixtures.seed_tenant().await;
    let busy_runner = tenant_runner(&fixtures, &busy.tenant, &busy.tag).await;
    let quiet_runner = tenant_runner(&fixtures, &quiet, &busy.tag).await;
    let platform = platform_runner(&fixtures, &busy.tag).await;

    let mut connection = fixtures.database.acquire().await.expect("a connection");
    let found = waiting(
        &mut connection,
        &[busy_runner.clone(), quiet_runner.clone(), platform],
    )
    .await
    .expect("the probe reads");

    assert!(found.pending(), "the busy tenant's admission waits");
    assert_eq!(
        found.idle(),
        [quiet_runner],
        "only the quiet tenant's runner is idle; the busy tenant's and the platform's are offered"
    );
}

/// An empty runner list asks only whether anything waits: the recovery tick.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_probe_without_runners_names_none_idle() {
    let fixtures = Fixtures::create().await;
    Owner::new().fleet(&fixtures, TENANT_MODEL).await;
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    let found = waiting(&mut connection, &[])
        .await
        .expect("the probe reads");
    assert!(found.pending());
    assert_eq!(found.idle(), &[] as &[Uuid7], "no runner was asked about");
}
