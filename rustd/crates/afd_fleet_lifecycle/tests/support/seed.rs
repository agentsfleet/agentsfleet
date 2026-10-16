//! The fleet rows a lane writes before a test runs.
//!
//! Native-shaped rows written in SQL: setup is `afd_authoring`'s to perform,
//! and this crate only ever reads, counts and purges what setup wrote. Every
//! row carries the resource version the conditional purge compares against.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::test_util::FleetSeed;

use super::{Lane, mint};

/// The stored configuration every fixture fleet carries: one API trigger.
const FIXTURE_CONFIG: &str = r#"{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// The status a fixture fleet is born in: leasable, as activation leaves it.
const ACTIVE: &str = "active";

/// The terminal status a purge requires.
const KILLED: &str = "killed";

/// The resource version a native Save writes on its first row.
const INITIAL_VERSION: i64 = 1;

impl Lane {
    /// One active fleet in `workspace`, created at `at`, named by its identifier.
    pub(crate) async fn fleet_at(&self, workspace: &Uuid7, at: UnixMillis) -> Uuid7 {
        let id = mint();
        let name = format!("fleet-{id}");
        FleetSeed::new(
            id.as_str(),
            workspace.as_str(),
            self.tenant.as_str(),
            ACTIVE,
        )
        .name(&name)
        .config(FIXTURE_CONFIG)
        .versions(INITIAL_VERSION, INITIAL_VERSION)
        .at(at.as_millis())
        .insert(&mut *self.connection().await)
        .await
        .expect("seeding a fleet");
        id
    }

    /// One active fleet in the lane's own workspace.
    pub(crate) async fn fleet(&self) -> Uuid7 {
        self.fleet_at(&self.workspace, Self::now()).await
    }

    /// Takes the fleet to `killed` and answers the tag a purge must present,
    /// as the native status PATCH advances the resource version.
    pub(crate) async fn kill(&self, fleet: &Uuid7) -> String {
        let version: i64 = sqlx::query_scalar(
            "UPDATE core.fleets SET status = $2, resource_version = resource_version + 1 \
             WHERE id = $1::uuid RETURNING resource_version",
        )
        .bind(fleet.as_str())
        .bind(KILLED)
        .fetch_one(&mut *self.connection().await)
        .await
        .expect("an active fleet can be killed");
        afd_core::etag::versioned(fleet, version)
    }

    /// The tag a purge must present for a fleet left where it is.
    pub(crate) async fn tag(&self, fleet: &Uuid7) -> String {
        let version: i64 =
            sqlx::query_scalar("SELECT resource_version FROM core.fleets WHERE id = $1::uuid")
                .bind(fleet.as_str())
                .fetch_one(&mut *self.connection().await)
                .await
                .expect("reading a fleet's version");
        afd_core::etag::versioned(fleet, version)
    }
}
