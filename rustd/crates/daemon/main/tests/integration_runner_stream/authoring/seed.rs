//! Real creator authority, priced model and sealed credential, scoped to this run.

use af_core::{clock, id::Uuid7, money::NANOS_PER_USD};
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_db::{
    Db,
    test_util::{ModelSeed, OwnedRows, TenantSeed, TestDatabase, UserSeed, WorkspaceSeed, mint_id},
};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::json;
use std::sync::Arc;

pub(super) struct Rows {
    pub(super) tenant: Uuid7,
    pub(super) user: Uuid7,
    pub(super) workspace: Uuid7,
    pub(super) model: Uuid7,
    pub(super) catalogue: Uuid7,
}

impl Rows {
    pub(super) async fn seed(db: &Db) -> Self {
        let rows = Self {
            tenant: mint_id(),
            user: mint_id(),
            workspace: mint_id(),
            model: mint_id(),
            catalogue: mint_id(),
        };
        let mut connection = db.acquire().await.expect("seed connection");
        TenantSeed::new(&rows.tenant)
            .name("Setup review")
            .insert(&mut *connection)
            .await
            .expect("tenant");
        UserSeed::new(
            &rows.user,
            &rows.tenant,
            rows.user.as_str(),
            rows.user.as_str(),
        )
        .insert(&mut *connection)
        .await
        .expect("creator");
        WorkspaceSeed::new(&rows.workspace, &rows.tenant)
            .insert(&mut *connection)
            .await
            .expect("workspace");
        let name = format!("native-authoring-{}", rows.catalogue);
        ModelSeed::new(&rows.catalogue, "openai", &name, 128_000)
            .rates(NANOS_PER_USD, NANOS_PER_USD / 10, NANOS_PER_USD * 2)
            .verified_at(clock::now())
            .inference(r#"{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":8192}"#)
            .insert(&mut *connection).await.expect("priced model");
        sqlx::query("INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,created_at,updated_at) VALUES($1,$2,$3,'authoring-model',now(),now())")
            .bind(&rows.model).bind(&rows.tenant).bind(name).execute(&mut *connection).await.expect("tenant model");
        sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1,$2,now(),now())")
            .bind(&rows.tenant).bind(100 * NANOS_PER_USD).execute(&mut *connection).await.expect("funded wallet");
        drop(connection);
        rows.credential(db).await;
        rows
    }

    pub(super) fn owned_rows(&self, runner: &Uuid7) -> OwnedRows {
        let mut cleanup = OwnedRows::new(&TestDatabase::shared());
        for (statement, id) in [
            (
                "DELETE FROM authoring.drafts WHERE workspace_id=$1",
                &self.workspace,
            ),
            (
                "DELETE FROM authoring.replay_receipts WHERE workspace_id=$1",
                &self.workspace,
            ),
            (
                "DELETE FROM authoring.attempts WHERE workspace_id=$1",
                &self.workspace,
            ),
            ("DELETE FROM identity.tenants WHERE id=$1", &self.tenant),
            ("DELETE FROM runner.runners WHERE id=$1", runner),
            (
                "DELETE FROM model.platform_models WHERE id=$1",
                &self.catalogue,
            ),
        ] {
            cleanup.record(statement, id);
        }
        cleanup
    }

    async fn credential(&self, db: &Db) {
        let kek = Kek::from_hex(crate::e2e::GOOD_KEK).expect("daemon key");
        let raw = serde_json::value::to_raw_value(
            &json!({"provider":"openai","api_key":"native-authoring-fixture"}),
        )
        .expect("provider JSON");
        let body = SecretBody::parse(&raw).expect("provider credential");
        Vault::new(db.clone(), Arc::new(kek), Entropy::new())
            .create(
                &self.workspace,
                &SecretName::parse("authoring-model").expect("secret name"),
                &body,
                clock::now(),
            )
            .await
            .expect("sealed credential");
    }
}
