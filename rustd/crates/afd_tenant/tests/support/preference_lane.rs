//! The lane's database and the rows a preference needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated. Nothing here creates or migrates one.
//!
//! # Every identifier is minted, and none of these rows is shared
//!
//! A preference keys on `(user, workspace)` and the onboarding signals read
//! five tables scoped by workspace and tenant. Both are per-test DATA rather
//! than scaffolding — a second test seeding the same user would be asserting
//! against the first one's bag — so every identifier here is minted and nothing
//! carries an `ON CONFLICT` arm to absorb a collision that should never happen.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_tenant::preference::Preferences;

/// The instant every fixture row is stamped with.
pub(crate) const NOW_MS: i64 = 1_760_000_000_000;

/// A migrated database, the store over it, and the rows one person needs.
pub(crate) struct Lane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) preferences: Preferences,
    /// The workspace this test's preferences hang from.
    pub(crate) workspace: Uuid7,
    /// Its tenant, which the model signal is scoped by.
    pub(crate) tenant: Uuid7,
    /// The identity-provider subject the fixture person presents.
    pub(crate) subject: String,
    /// The internal user id that subject resolves to.
    pub(crate) user: String,
}

impl Lane {
    /// Seeds a tenant, a workspace and one person inside the shared database.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let preferences = Preferences::new(pool.clone(), Entropy::new());

        let tenant = mint();
        let workspace = mint();
        let user = mint();
        // Minted rather than fixed: `core.users.oidc_subject` is UNIQUE across
        // the whole table, so a shared spelling is a collision the moment a
        // second test in this lane seeds its own person.
        let subject = format!("fixture|{}", user.as_str());

        let lane = Self {
            database,
            pool,
            preferences,
            workspace,
            tenant,
            subject,
            user: user.as_str().to_owned(),
        };
        lane.seed_person().await;
        lane
    }

    /// The instant this suite stamps writes with.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// Puts one fleet in the workspace, so `has_fleet` turns true.
    pub(crate) async fn seed_fleet(&self) {
        sqlx::query(
            "INSERT INTO core.fleets
               (id, workspace_id, tenant_id, name, source_markdown, config_json,
                status, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'fixture', '# fixture',
                     '{}'::jsonb, 'active', $4, $4)",
        )
        .bind(mint().as_str())
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a fleet");
    }

    /// Gives the tenant its own model selection, so `model_configured` is true
    /// without depending on whatever platform default the lane happens to hold.
    /// `mode` must be a spelling `afd_billing::sql::posture` declares —
    /// `self_managed` here. This crate never reads the column, so a bad value
    /// sits inert in its own tests and detonates in whichever suite resolves a
    /// posture from the row; `'byok'` did exactly that. A dev-dependency on
    /// `afd_billing` to name the constant would add a build edge for a value
    /// this crate does not read, so the spelling is pinned by this comment.
    pub(crate) async fn seed_tenant_model(&self, model: &str) {
        sqlx::query(
            "INSERT INTO core.tenant_model_selection
               (tenant_id, mode, provider, model, context_cap_tokens, created_at, updated_at)
             VALUES ($1::uuid, 'self_managed', 'anthropic', $2, 200000, $3, $3)
             ON CONFLICT (tenant_id) DO UPDATE
               SET model = EXCLUDED.model, updated_at = EXCLUDED.updated_at",
        )
        .bind(self.tenant.as_str())
        .bind(model)
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a tenant model selection");
    }

    /// Drops nothing — the lane's database is shared. Kept so every suite here
    /// ends the same way its siblings do.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// One pooled connection, for the fixture's own writes.
    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant, the workspace and the person every read keys on.
    async fn seed_person(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'fixture', $2, $2)",
        )
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
             VALUES ($1::uuid, $2::uuid, 'fixture', 'fixture', $3)",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");

        sqlx::query(
            "INSERT INTO core.users
               (id, tenant_id, oidc_subject, email, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3, 'fixture@example.test', $4, $4)",
        )
        .bind(&self.user)
        .bind(self.tenant.as_str())
        .bind(&self.subject)
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a user");
    }
}

/// A fresh identifier, so no two fixtures can name each other's rows.
pub(crate) fn mint() -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(Lane::now(), bytes).expect("a well-formed identifier")
}
