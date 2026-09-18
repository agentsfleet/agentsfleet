//! The census pass against the lane's migrated Postgres schema.
//!
//! What a table proves that the unit rows cannot: the statement is
//! schema-qualified and grouped the way the pass expects, a real `COUNT(*)`
//! lands in the cells, and every status publishes after one pass.
//!
//! # Why the counts are lower bounds
//!
//! The lane database is shared with every other integration test, several of
//! which seed fleets of their own while this runs. The census counts the
//! table, not this fixture, so what is provable is that the seeded rows are
//! IN the count and that the pass scanned exactly what it published. Exact
//! zeros for absent statuses are the unit proof on the cells themselves.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_observability::metrics::label::fleet::FleetStatusLabel;
use afd_observability::producers::fleet::census::fleet_census_readings;
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::census::Census;

/// The statuses this fixture seeds, and how many of each.
const SEEDED: [(FleetStatusLabel, u64); 3] = [
    (FleetStatusLabel::Active, 2),
    (FleetStatusLabel::Paused, 1),
    (FleetStatusLabel::Stopped, 1),
];

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_census_publishes_every_status() {
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let census = Census::new(fixture.database.clone());
    let swept = census.sweep().await.expect("the census counts real rows");
    assert_eq!(swept.changed, 0, "a census changes nothing");

    let readings = fleet_census_readings();
    assert_eq!(
        readings.len(),
        FleetStatusLabel::ALL.len(),
        "one reading per status after a successful pass, zero or not"
    );
    let published: u64 = readings.iter().map(|reading| reading.value).sum();
    assert_eq!(
        published, swept.scanned,
        "the pass scanned exactly what it published"
    );
    for (status, seeded) in SEEDED {
        let counted = readings
            .iter()
            .find(|reading| {
                reading
                    .attributes
                    .iter()
                    .any(|attribute| attribute.value.as_str() == status.as_str())
            })
            .map(|reading| reading.value)
            .expect("every seeded status has a reading");
        assert!(
            counted >= seeded,
            "{} fleets seeded as {}, census read {counted}",
            seeded,
            status.as_str()
        );
    }

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: String,
    fleets: Vec<(String, FleetStatusLabel)>,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let fleets = SEEDED
            .iter()
            .flat_map(|(status, count)| (0..*count).map(move |_| (mint_id(), *status)))
            .collect();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: mint_id(),
            fleets,
            lane,
        }
    }

    async fn seed(&self) {
        let now = clock::now().as_millis();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Census test', $2, $2) \
               RETURNING id \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $3::uuid, id, 'census', 'test', $2 FROM tenant",
        )
        .bind(&self.tenant)
        .bind(now)
        .bind(&self.workspace)
        .execute(&mut *connection)
        .await
        .expect("the tenant and workspace seed");
        for (index, (fleet, status)) in self.fleets.iter().enumerate() {
            sqlx::query(
                "INSERT INTO core.fleets \
                   (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                    status, created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, $3::uuid, $4, '# test', '{}', $5, $6, $6)",
            )
            .bind(fleet)
            .bind(&self.workspace)
            .bind(&self.tenant)
            .bind(format!("census-{index}"))
            .bind(status.as_str())
            .bind(now)
            .execute(&mut *connection)
            .await
            .expect("the fleet row seeds");
        }
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.fleets WHERE tenant_id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the fleet rows clean up");
        sqlx::query("DELETE FROM core.workspaces WHERE id = $1::uuid")
            .bind(&self.workspace)
            .execute(&mut *connection)
            .await
            .expect("the workspace cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the tenant cleans up");
        drop(connection);
        self.lane.cleanup().await;
    }
}
