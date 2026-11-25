//! A run's policy reads exactly the grants a person approved for its fleet.
//!
//! Pending and revoked grants, and another fleet's approved one, never read as
//! access: the policy build's one source of names and scope is this read, so a
//! row it let through would be a credential nobody approved.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_api_wire::grant::{grant_type, status};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, GrantSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_fleet_lifecycle::FleetStatus;
use afd_gate::policy::approved::{Approved, ApprovedGrant, hold_approved_on};

/// The scope an approved GitHub grant stores.
const SCOPE: &str = r#"{"repositories":["acme/widgets"],"access":"read"}"#;

/// A tenant, a workspace and two fleets of this test's own.
struct Lane {
    database: Db,
    fleet: Uuid7,
    other: Uuid7,
}

impl Lane {
    async fn seed() -> Self {
        let database = TestDatabase::shared().open(DbRole::Api, &[]).await;
        let (tenant, workspace) = (mint_id(), mint_id());
        let lane = Self {
            database,
            fleet: mint_id(),
            other: mint_id(),
        };
        let mut connection = lane.database.acquire().await.expect("a connection");
        TenantSeed::new(&tenant)
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&workspace, &tenant)
            .insert(&mut *connection)
            .await
            .expect("the workspace seeds");
        for fleet in [&lane.fleet, &lane.other] {
            FleetSeed::new(fleet, &workspace, &tenant, FleetStatus::Active.as_str())
                .insert(&mut *connection)
                .await
                .expect("the fleet seeds");
        }
        lane
    }

    async fn grant(&self, fleet: &Uuid7, name: &str, kind: &str, status: &str) {
        let mut seed = GrantSeed::new(fleet, name, kind, status);
        if name == "github" {
            seed = seed.scope(SCOPE);
        }
        seed.insert(&mut *self.database.acquire().await.expect("a connection"))
            .await
            .expect("the grant seeds");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn only_the_fleets_approved_grants_are_read() {
    let lane = Lane::seed().await;
    lane.grant(&lane.fleet, "sentry", grant_type::SECRET, status::APPROVED)
        .await;
    lane.grant(
        &lane.fleet,
        "github",
        grant_type::INTEGRATION,
        status::APPROVED,
    )
    .await;
    lane.grant(&lane.fleet, "grafana", grant_type::SECRET, status::PENDING)
        .await;
    lane.grant(
        &lane.fleet,
        "linear",
        grant_type::INTEGRATION,
        status::REVOKED,
    )
    .await;
    lane.grant(&lane.other, "stripe", grant_type::SECRET, status::APPROVED)
        .await;

    let mut connection = lane.database.acquire().await.expect("a connection");
    let approved = Approved::read_on(&mut connection, &lane.fleet)
        .await
        .expect("the approved grants read");

    let expected: Approved = [
        ApprovedGrant {
            name: "github".to_owned(),
            grant_type: grant_type::INTEGRATION.to_owned(),
            scope: Some(r#"{"access": "read", "repositories": ["acme/widgets"]}"#.to_owned()),
        },
        ApprovedGrant {
            name: "sentry".to_owned(),
            grant_type: grant_type::SECRET.to_owned(),
            scope: None,
        },
    ]
    .into_iter()
    .collect();
    assert_eq!(approved, expected);
    assert!(
        approved
            .repository_binding()
            .expect("the stored scope reads back")
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn only_an_approved_grant_is_held() {
    let lane = Lane::seed().await;
    lane.grant(
        &lane.fleet,
        "github",
        grant_type::INTEGRATION,
        status::APPROVED,
    )
    .await;
    lane.grant(
        &lane.fleet,
        "zoho",
        grant_type::INTEGRATION,
        status::PENDING,
    )
    .await;
    lane.grant(
        &lane.other,
        "linear",
        grant_type::INTEGRATION,
        status::APPROVED,
    )
    .await;

    let mut connection = lane.database.acquire().await.expect("a connection");
    for (name, held) in [
        ("github", true),
        ("zoho", false),
        ("linear", false),
        ("jira", false),
    ] {
        assert_eq!(
            hold_approved_on(&mut connection, &lane.fleet, name)
                .await
                .expect("the hold reads"),
            held,
            "{name}"
        );
    }
}
