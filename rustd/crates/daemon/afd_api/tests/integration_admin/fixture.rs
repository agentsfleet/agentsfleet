//! The existing platform fixture owns its catalogue row and tenant cleanup.

use super::SUBJECT;
use af_core::id::Uuid7;
use afd_auth::{credential::Presented, directory::Digest};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{ApiKeySeed, ModelSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id},
};

pub(super) struct Fixture {
    pub(super) lane: TestDatabase,
    pub(super) database: Db,
    pub(super) tenant: Uuid7,
    pub(super) key: Uuid7,
    pub(super) token: String,
    pub(super) workspace: Uuid7,
    pub(super) platform_model_row: Uuid7,
    pub(super) platform_model: String,
    pub(super) provider: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        afd_db::Migrator::new()
            .run(&migrator)
            .await
            .expect("isolated admin schema");
        migrator.close().await;
        let first = mint_id().as_str().replace('-', "");
        let second = mint_id().as_str().replace('-', "");
        let provider = mint_id().as_str().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            key: mint_id(),
            token: format!("agt_t{first}{second}"),
            workspace: mint_id(),
            platform_model_row: mint_id(),
            platform_model: "platform-key-model".to_owned(),
            provider,
            lane,
        }
    }

    pub(super) fn platform_key_body(&self) -> String {
        serde_json::json!({
            "provider": self.provider,
            "source_workspace_id": self.workspace,
            "model": self.platform_model,
        })
        .to_string()
    }

    pub(super) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Platform catalogue")
            .insert(&mut *connection)
            .await
            .expect("the platform tenant seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("platform-default")
            .insert(&mut *connection)
            .await
            .expect("its workspace seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), SUBJECT)
            .name("platform")
            .insert(&mut *connection)
            .await
            .expect("the platform credential seeds");
        ModelSeed::new(
            &self.platform_model_row,
            &self.provider,
            &self.platform_model,
            200_000,
        )
        .rates(5, 1, 25)
        .insert(&mut *connection)
        .await
        .expect("the platform-default model seeds");
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM model.platform_defaults WHERE provider = $1")
            .bind(&self.provider)
            .execute(&mut *connection)
            .await
            .expect("the platform default cleanup runs");
        sqlx::query("DELETE FROM model.platform_models WHERE id = $1")
            .bind(&self.platform_model_row)
            .execute(&mut *connection)
            .await
            .expect("the platform model cleanup runs");

        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped credential cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
