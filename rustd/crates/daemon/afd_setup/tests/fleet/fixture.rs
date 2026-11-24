//! One workspace's people, models and fleets over the lane's Postgres and NATS.

use super::{mint, seed};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_api_wire::fleet::{CreateFleetRequest, PatchFleetRequest};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase},
};
use afd_setup::{Access, Creations};
use sqlx::pool::PoolConnection;

/// The message every fixture fleet is created from.
pub(super) const FIRST_MESSAGE: &str = "Watch checkout failures and tell me what broke.";

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) creations: Creations,
    pub(super) owner: Uuid7,
    pub(super) member: Uuid7,
    pub(super) workspace: Uuid7,
    pub(super) tenant: Uuid7,
    pub(super) member_tenant: Uuid7,
    /// The owner's registered model a create names.
    pub(super) model: Uuid7,
    /// A second registered model a patch moves a fleet to.
    pub(super) other_model: Uuid7,
    catalogue: [Uuid7; 2],
    cleanup: OwnedRows,
    lane: TestDatabase,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        Self::in_lane(TestDatabase::shared()).await
    }

    /// A database of the test's own, holding no platform default and
    /// counting its statements.
    pub(super) async fn isolated() -> Self {
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        afd_db::Migrator::new()
            .run(&migrator)
            .await
            .expect("isolated platform schema");
        afd_db::test_util::statements::install(&migrator).await;
        migrator.close().await;
        Self::in_lane(lane).await
    }

    async fn in_lane(lane: TestDatabase) -> Self {
        let db = lane.open(DbRole::Api, &[]).await;
        let bus = afd_bus::test_util::lane()
            .await
            .expect("the lane's NATS: run through make test-integration-rustd");
        let turns = afd_turns::Turns::new(db.clone(), bus, Entropy::new());
        let mut fixture = Self {
            creations: Creations::new(db.clone(), Entropy::new(), turns),
            db,
            cleanup: OwnedRows::new(&lane),
            lane,
            owner: mint(),
            member: mint(),
            workspace: mint(),
            tenant: mint(),
            member_tenant: mint(),
            model: mint(),
            other_model: mint(),
            catalogue: [mint(), mint()],
        };
        fixture.arm_cleanup();
        seed::people(&fixture).await;
        seed::credential(&fixture).await;
        for (catalogue, entry) in fixture
            .catalogue
            .iter()
            .zip([&fixture.model, &fixture.other_model])
        {
            seed::model(&fixture, catalogue, entry).await;
        }
        fixture
    }

    pub(super) fn access(&self) -> Access<'_> {
        Access {
            workspace: &self.workspace,
            subject: self.owner.as_str(),
        }
    }

    /// A create of [`FIRST_MESSAGE`] under `operation`, on the owner's model.
    pub(super) fn request(&self, operation: &Uuid7) -> CreateFleetRequest {
        CreateFleetRequest {
            message: FIRST_MESSAGE.to_owned(),
            operation_id: operation.to_string(),
            model_ref: Some(self.model.to_string()),
            name: None,
        }
    }

    pub(super) async fn connection(&self) -> PoolConnection<sqlx::Postgres> {
        self.db.acquire().await.expect("fixture database answers")
    }

    /// The lane owner's connection, which may read the statement counters.
    pub(super) async fn owner_db(&self) -> Db {
        self.lane.open(DbRole::Migrator, &[]).await
    }

    /// This workspace's fleets, their threads, and their threads' turns.
    pub(super) async fn counts(&self) -> (i64, i64, i64) {
        sqlx::query_as(
            "SELECT (SELECT count(*) FROM fleet.fleets WHERE workspace_id=$1), \
             (SELECT count(*) FROM thread.threads t JOIN fleet.fleets f ON f.id=t.fleet_id \
               WHERE f.workspace_id=$1), \
             (SELECT count(*) FROM thread.turns WHERE workspace_id=$1)",
        )
        .bind(&self.workspace)
        .fetch_one(&mut *self.connection().await)
        .await
        .expect("scoped durable counts")
    }

    /// The `(actor, status)` of every turn `fleet` holds, oldest first.
    pub(super) async fn turns(&self, fleet: &Uuid7) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT actor, status FROM thread.turns WHERE thread_id=$1 ORDER BY sequence",
        )
        .bind(fleet)
        .fetch_all(&mut *self.connection().await)
        .await
        .expect("the fleet's turns")
    }

    fn arm_cleanup(&mut self) {
        self.cleanup.record_text(
            "DELETE FROM thread.replay_receipts WHERE scope LIKE $1",
            &format!("{}:%", self.workspace),
        );
        self.cleanup
            .record("DELETE FROM fleet.fleets WHERE tenant_id=$1", &self.tenant);
        for tenant in [&self.tenant, &self.member_tenant] {
            self.cleanup
                .record("DELETE FROM identity.tenants WHERE id=$1", tenant);
        }
        for id in &self.catalogue {
            self.cleanup
                .record("DELETE FROM model.platform_models WHERE id=$1", id);
        }
    }

    pub(super) async fn cleanup(mut self) {
        self.cleanup
            .finish()
            .await
            .expect("remove only fixture-owned rows");
        assert_eq!(self.counts().await, (0, 0, 0), "fleet rows do not leak");
        self.db.close().await;
        self.lane.cleanup().await;
    }
}

/// `request` as the route's extractor proves it.
pub(super) fn valid<T: garde::Validate<Context = ()>>(request: T) -> garde::Valid<T> {
    garde::Unvalidated::new(request)
        .validate()
        .expect("a request inside its bounds")
}

/// A patch that changes nothing but what the caller sets.
pub(super) const fn patch() -> PatchFleetRequest {
    PatchFleetRequest {
        status: None,
        name: None,
        model_ref: None,
        http_egress: None,
        package_domains: None,
    }
}

pub(super) fn now() -> jiff::Timestamp {
    af_core::clock::now()
}
