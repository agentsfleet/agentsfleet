//! The platform default a test holds is removed by the process that made it —
//! when the holding test fails as well as when it passes — and a default that
//! was already there is never removed by a hold.
//!
//! Each case seeds a provider of its own, so no other suite's default is in
//! reach, and removes its own tenant, workspace and catalogue row afterwards.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and one case panics on purpose"
)]

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    DefaultSeed, PlatformDefault, TenantSeed, TestDatabase, WorkspaceSeed, mint_id,
};

/// The model every case prices, under its own provider.
const MODEL: &str = "guard-model";

/// The context ceiling every case's default carries.
const CAP: i32 = 1_000;

/// A provider, workspace and priced model of one case's own.
struct Scene {
    database: Db,
    provider: String,
    tenant: String,
    workspace: String,
    model_row: String,
}

impl Scene {
    async fn seed() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        // The id is time-ordered, so its random tail, not its head, keeps two
        // cases seeded in the same millisecond on different providers.
        let id = mint_id().replace('-', "");
        let provider = format!("guard{}", &id[id.len() - 12..]);
        let scene = Self {
            database,
            provider,
            tenant: mint_id(),
            workspace: mint_id(),
            model_row: mint_id(),
        };
        let mut connection = scene.database.acquire().await.expect("a connection");
        TenantSeed::new(&scene.tenant)
            .insert(&mut *connection)
            .await
            .expect("the tenant seed");
        WorkspaceSeed::new(&scene.workspace, &scene.tenant)
            .insert(&mut *connection)
            .await
            .expect("the workspace seed");
        sqlx::query(
            "INSERT INTO model.platform_models \
               (id, model_name, provider, display_name, context_cap_tokens, context_source_url, \
                context_verified_at, advanced, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, $2, $4, $5, now(), false, now(), now())",
        )
        .bind(&scene.model_row)
        .bind(MODEL)
        .bind(&scene.provider)
        .bind(CAP)
        .bind("https://example.test/models/guard")
        .execute(&mut *connection)
        .await
        .expect("the catalogue row seeds");
        scene
    }

    fn seed_for_hold(&self) -> DefaultSeed<'_> {
        DefaultSeed {
            provider: &self.provider,
            source_workspace_id: &self.workspace,
            model: MODEL,
            context_cap_tokens: CAP,
            created_at: afd_core::clock::now(),
        }
    }

    /// The workspace this provider's default names, if it has one.
    async fn default_names(&self) -> Option<String> {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_scalar(
            "SELECT source_workspace_id::text FROM model.platform_defaults \
             WHERE provider = $1",
        )
        .bind(&self.provider)
        .fetch_optional(&mut *connection)
        .await
        .expect("the default read runs")
    }

    /// Writes this provider's default directly: a row no hold created.
    async fn preexisting_default(&self) {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query(
            "INSERT INTO model.platform_defaults \
               (provider, source_workspace_id, active, platform_model_id, created_at, updated_at) \
             VALUES ($1, $2::uuid, FALSE, $3::uuid, now(), now())",
        )
        .bind(&self.provider)
        .bind(&self.workspace)
        .bind(&self.model_row)
        .execute(&mut *connection)
        .await
        .expect("the pre-existing default seeds");
    }

    /// Removes everything this case wrote, the default first.
    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("a connection");
        for (statement, id) in [
            (
                "DELETE FROM model.platform_defaults WHERE provider = $1",
                &self.provider,
            ),
            (
                "DELETE FROM model.platform_models WHERE id = $1::uuid",
                &self.model_row,
            ),
            (
                "DELETE FROM identity.workspaces WHERE id = $1::uuid",
                &self.workspace,
            ),
            (
                "DELETE FROM identity.tenants WHERE id = $1::uuid",
                &self.tenant,
            ),
        ] {
            sqlx::query(statement)
                .bind(id)
                .execute(&mut *connection)
                .await
                .expect("the fixture cleanup runs");
        }
    }
}

/// A test that fails while holding the default it created still removes it:
/// the hold drops while the panic unwinds, before the task's failure is seen.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_default_a_failing_holder_created_is_removed_as_it_unwinds() {
    let scene = Scene::seed().await;
    let (database, seed_provider, workspace) = (
        scene.database.clone(),
        scene.provider.clone(),
        scene.workspace.clone(),
    );

    let failed = tokio::spawn(async move {
        let _hold = PlatformDefault::hold(
            &database,
            DefaultSeed {
                provider: &seed_provider,
                source_workspace_id: &workspace,
                model: MODEL,
                context_cap_tokens: CAP,
                created_at: afd_core::clock::now(),
            },
        )
        .await;
        panic!("the holding test fails");
    })
    .await
    .expect_err("the holding task panicked");

    assert!(failed.is_panic(), "the task failed by panicking: {failed}");
    assert_eq!(
        scene.default_names().await,
        None,
        "the row the failing holder created is gone"
    );
    scene.cleanup().await;
}

/// A default that existed before any hold survives a hold that passes and a
/// hold that panics: neither created it, so neither may remove it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_default_that_already_existed_survives_a_passing_and_a_failing_hold() {
    let scene = Scene::seed().await;
    scene.preexisting_default().await;

    drop(PlatformDefault::hold(&scene.database, scene.seed_for_hold()).await);
    assert_eq!(
        scene.default_names().await.as_deref(),
        Some(scene.workspace.as_str()),
        "a passing hold left the row it did not create"
    );

    let (database, provider, workspace) = (
        scene.database.clone(),
        scene.provider.clone(),
        scene.workspace.clone(),
    );
    let failed = tokio::spawn(async move {
        let _hold = PlatformDefault::hold(
            &database,
            DefaultSeed {
                provider: &provider,
                source_workspace_id: &workspace,
                model: MODEL,
                context_cap_tokens: CAP,
                created_at: afd_core::clock::now(),
            },
        )
        .await;
        panic!("the holding test fails");
    })
    .await
    .expect_err("the holding task panicked");

    assert!(failed.is_panic(), "the task failed by panicking: {failed}");
    assert_eq!(
        scene.default_names().await.as_deref(),
        Some(scene.workspace.as_str()),
        "a failing hold left the row it did not create"
    );
    scene.cleanup().await;
}
