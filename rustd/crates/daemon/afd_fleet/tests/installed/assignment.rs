//! The fleet row is the whole of what an assignment installs.

use af_fleet_runtime::config::HttpEgress;
use afd_db::test_util::FIXTURE_MODEL_REF;
use afd_fleet::run::FRESH_CONTEXT;

use crate::{seed::seeded, support::Fixtures};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_fleet_rows_name_model_and_network_are_installed() {
    let fixtures = Fixtures::create().await;
    let held = seeded::<0>(&fixtures).await;
    sqlx::query(
        "UPDATE fleet.fleets SET name = 'Support triage', http_egress = 'public', \
         package_domains = true WHERE id = $1",
    )
    .bind(&held.fleet)
    .execute(&mut *fixtures.database.acquire().await.expect("connection"))
    .await
    .expect("open the fixture fleet's network");

    let installed = fixtures
        .runs()
        .installed(&held.fleet)
        .await
        .expect("assignment read")
        .expect("runnable");

    assert_eq!(installed.name, "Support triage");
    assert_eq!(installed.model_ref, FIXTURE_MODEL_REF);
    assert_eq!(installed.http_egress, HttpEgress::Public);
    assert!(installed.package_domains);
    assert_eq!(installed.context_json, FRESH_CONTEXT);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn archived_and_missing_fleets_do_not_issue_work() {
    let fixtures = Fixtures::create().await;
    let held = seeded::<0>(&fixtures).await;
    sqlx::query("UPDATE fleet.fleets SET status='archived' WHERE id=$1")
        .bind(&held.fleet)
        .execute(&mut *fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("archive owned fleet");
    assert!(
        fixtures
            .runs()
            .installed(&held.fleet)
            .await
            .expect("archived read")
            .is_none()
    );
    let absent = afd_db::test_util::mint_id();
    assert!(
        fixtures
            .runs()
            .installed(&absent)
            .await
            .expect("absent read")
            .is_none()
    );
}
