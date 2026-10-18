//! One tenant, one owned workspace, one fleet and one API key per test, seeded
//! straight into the migrated schema and torn down by deleting the tenant.

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_core::id::Uuid7;
use afd_cron::MAX_SCHEDULES_PER_FLEET;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TestDatabase, mint_id};

use super::NIGHTLY;

pub(super) struct Fixture {
    lane: TestDatabase,
    pub(super) database: Db,
    subject: &'static str,
    tenant: String,
    pub(super) workspace: Uuid7,
    pub(super) fleet: Uuid7,
    user: String,
    key: String,
    pub(super) token: String,
}

impl Fixture {
    pub(super) async fn create(subject: &'static str) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            subject,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    pub(super) fn collection(&self) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/schedules",
            self.workspace.as_str(),
            self.fleet.as_str()
        )
    }

    pub(super) fn one(&self, schedule: &str) -> String {
        format!("{}/{schedule}", self.collection())
    }

    pub(super) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO identity.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Fleet schedules', now(), now()) \
             ), workspace AS ( \
               INSERT INTO identity.workspaces (id, tenant_id, name, created_at) \
               VALUES ($2::uuid, $1::uuid, 'schedules', now()) \
             ), person AS ( \
               INSERT INTO identity.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'schedules-live@example.test', now(), now()) \
             ) \
             INSERT INTO identity.api_keys \
               (id, tenant_id, name, description, key_hash, created_by_subject, \
                created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, now(), now())",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace seeds");

        FleetSeed::new(
            self.fleet.as_str(),
            self.workspace.as_str(),
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("schedules-fixture-fleet")
        .at(afd_db::test_util::SEEDED_AT)
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("the fleet seeds");
    }

    /// Fills this fleet to [`MAX_SCHEDULES_PER_FLEET`], bypassing the API.
    ///
    /// Written straight to the table for the reason the calling test states:
    /// the frozen clock makes a second create over HTTP a key collision, so the
    /// ceiling is not reachable through the surface being graded. Each row
    /// carries its own `provider_schedule_id`, which is the uniqueness the table
    /// actually enforces, and `resource_version` starts at one because zero is banned by a
    /// check constraint that exists to keep "never synced" distinguishable.
    ///
    /// `sync_status` is `failed` rather than any word meaning "not yet tried":
    /// the column holds one of three states this build parses, and a row
    /// carrying anything else is unreadable — the list answers an error rather
    /// than a page, which is how the first draft of this helper failed. Failed
    /// is also the honest state here, since nothing resolves the scheduler.
    pub(super) async fn fill_to_the_ceiling(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        for slot in 0..MAX_SCHEDULES_PER_FLEET {
            sqlx::query(
                "INSERT INTO fleet.schedules \
                   (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, \
                    message, once, desired_status, sync_status, resource_version, \
                    created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, 'api', $3, $4, 'UTC', 'seeded', false, \
                         'active', 'failed', 1, now(), now())",
            )
            .bind(mint_id())
            .bind(self.fleet.as_str())
            .bind(format!("seeded-{slot}"))
            .bind(NIGHTLY)
            .execute(&mut *connection)
            .await
            .expect("a seeded schedule row");
        }
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        let _ = self.lane;
    }
}
