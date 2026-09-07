//! Seeding the rows a live callback walk needs before it can run.
//!
//! Split from the fixture's own construction and teardown beside it: that half
//! is about a lane and a router, and this half is a long SQL arrangement of
//! tenant, workspaces, owner and key. They changed for different reasons and
//! together they put the file over its length cap.

use super::*;

impl Fixture {
    pub(crate) async fn seed(&self) {
        self.seed_rows().await;
        self.seal(
            STATE_KEY,
            &format!(r#"{{"{SECRET_FIELD}":"{STATE_SECRET}"}}"#),
        )
        .await;
        self.seal(
            &Provider::Slack.app_key(),
            &format!(r#"{{"client_id":"{CLIENT_ID}","client_secret":"{CLIENT_SECRET}"}}"#),
        )
        .await;
    }

    /// The tenant, the connected workspace, the admin workspace, and the owner.
    async fn seed_rows(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Connector callback live', 1, 1) \
             ), workspaces AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'connected', $3, 1), \
                      ($4::uuid, $1::uuid, 'platform-admin', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, $3, 'connector-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($6::uuid, $1::uuid, 'fixture', '', $7, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(self.admin.as_str())
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, its workspaces and its owner seed");

        // The bystander: a second person of the same tenant, with a credential
        // of their own. Without one, presenting the starter's token would
        // present the STARTER, and the callback would refuse on scope rather
        // than on the state binding this fixture exists to exercise.
        let bystander_digest =
            Digest::of(&Presented::new(&self.bystander_token).expect("the token is valid"));
        sqlx::query(
            "WITH person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($1::uuid, $2::uuid, $3, 'connector-bystander@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($4::uuid, $2::uuid, 'bystander', '', $5, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.bystander_user)
        .bind(&self.tenant)
        .bind(&self.bystander)
        .bind(&self.bystander_key)
        .bind(bystander_digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the bystander person and credential seed");
    }

    /// Seals `document` into the admin workspace under `key`.
    ///
    /// Through the real vault under the harness's own key rather than an INSERT
    /// of ciphertext: a row this fixture hand-wrote would be one the route could
    /// not open, and every reader here answers "not configured" for that — a
    /// refusal indistinguishable from having stored nothing at all.
    pub(crate) async fn seal(&self, key: &str, document: &str) {
        let raw = serde_json::value::RawValue::from_string(document.to_owned())
            .expect("the fixture credential is an object");
        let sealed = harness::vault(self.database.clone())
            .create(
                &self.admin,
                &SecretName::parse(key).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await;
        // Named in the message rather than left to `expect`: this seals two
        // secrets under different keys, and a failure that did not say which
        // reads as the route being unconfigured for both.
        assert!(sealed.is_ok(), "the fixture secret {key} seals: {sealed:?}");
    }

    /// The grant this workspace holds for `provider`, opened.
    ///
    /// Read through the vault rather than off the row, because the row holds
    /// ciphertext: what the assertion is about is the HANDLE a runner will open
    /// when a fleet declares this integration.
    pub(crate) async fn grant(&self, provider: Provider) -> Option<serde_json::Value> {
        let name = SecretName::parse(provider.grant_key()).expect("a provider key is storable");
        let opened = harness::vault(self.database.clone())
            .load(&self.workspace, &name)
            .await
            .expect("the vault answers");
        opened.map(|bytes| {
            serde_json::from_slice(bytes.expose()).expect("a sealed grant is a JSON object")
        })
    }

    /// How many secrets this workspace holds, by name.
    ///
    /// A count as well as a read: a second connect that sealed under a second
    /// name would leave the first grant intact and pass a read-only assertion
    /// while a runner opened the wrong one.
    pub(crate) async fn secret_names(&self) -> Vec<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "SELECT key_name FROM vault.secrets WHERE workspace_id = $1::uuid ORDER BY key_name",
        )
        .bind(self.workspace.as_str())
        .fetch_all(&mut *connection)
        .await
        .expect("the workspace's secret names read")
        .iter()
        .map(|row| row.get("key_name"))
        .collect()
    }

    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        for workspace in [&self.workspace, &self.admin] {
            sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid")
                .bind(workspace.as_str())
                .execute(&mut *transaction)
                .await
                .expect("the fixture's sealed secrets clean up");
            // The routing rows too. They are keyed on the provider account
            // rather than on this fixture, so a row left behind is one the
            // next test on this shared lane meets as "held elsewhere" — which
            // is the product working and the fixture leaking.
            sqlx::query("DELETE FROM core.connector_installs WHERE workspace_id = $1::uuid")
                .bind(workspace.as_str())
                .execute(&mut *transaction)
                .await
                .expect("the fixture's routing rows clean up");
        }
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the scoped fixture cleans up");
        transaction
            .commit()
            .await
            .expect("the scoped cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
