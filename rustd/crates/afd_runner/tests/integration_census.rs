//! Census publishes exact durable queue counts and withdraws them after query failure.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_db::{Db, Migrator};
use afd_observability::metrics::label::fleet::FleetStatusLabel;
use afd_observability::producers::fleet::census::fleet_census_readings;
use afd_observability::test_util::Capture;
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
    let capture = Capture::install();
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let census = Census::new(fixture.database.clone());
    let swept = census.sweep().await.expect("the census counts real rows");
    assert_eq!(swept.changed, 0, "a census changes nothing");

    assert_statuses(swept.scanned);
    fixture.seed_backlog().await;
    census.sweep().await.expect("the backlog counts");
    assert_eq!(capture.gauge("agentsfleet_admission_backlog", &[]), Some(3));
    assert!(
        capture
            .gauge("agentsfleet_admission_backlog_oldest_age_seconds", &[])
            .is_some_and(|age| age >= 299)
    );

    fixture.database.close().await;
    census
        .sweep()
        .await
        .expect_err("a closed database withdraws its readings");
    assert_eq!(capture.gauge("agentsfleet_admission_backlog", &[]), None);
    assert_eq!(
        capture.gauge("agentsfleet_admission_backlog_oldest_age_seconds", &[]),
        None
    );
    assert!(fleet_census_readings().is_empty());

    fixture.cleanup().await;
}

fn assert_statuses(scanned: u64) {
    let readings = fleet_census_readings();
    assert_eq!(readings.len(), FleetStatusLabel::ALL.len());
    assert_eq!(
        readings.iter().map(|reading| reading.value).sum::<u64>(),
        scanned
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
            .expect("every status publishes");
        assert_eq!(counted, seeded);
    }
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
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("private schema migrates");
        migrator.close().await;
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
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Census test")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("census")
            .insert(&mut *connection)
            .await
            .expect("the workspace seeds");
        for (index, (fleet, status)) in self.fleets.iter().enumerate() {
            let name = format!("census-{index}");
            FleetSeed::new(fleet, &self.workspace, &self.tenant, status.as_str())
                .name(&name)
                .insert(&mut *connection)
                .await
                .expect("the fleet row seeds");
        }
    }

    async fn seed_backlog(&self) {
        let mut connection = self.database.acquire().await.expect("database");
        for ordinal in 0..4 {
            let id = mint_id();
            sqlx::query(
                "INSERT INTO fleet.admissions \
                (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type, \
                 request_json,event_created_at,created_at,updated_at,delivered_at,settled_at) \
                VALUES ($1::uuid,$2::uuid,$3::uuid,$4,$1,'fixture','fixture',$5,'{}', \
                  now()-interval '5 minutes',now()-interval '5 minutes',now(), \
                  CASE WHEN $6 THEN now() END, CASE WHEN $7 THEN now() END)",
            )
            .bind(&id)
            .bind(&self.fleets.first().expect("seeded fleet").0)
            .bind(&self.workspace)
            .bind(afd_admission::Producer::Steer.as_str())
            .bind(afd_wire::event::EventType::Chat.as_str())
            .bind(ordinal == 1)
            .bind(ordinal == 3)
            .execute(&mut *connection)
            .await
            .expect("backlog fixture");
        }
    }

    async fn cleanup(self) {
        self.lane.cleanup().await;
    }
}
