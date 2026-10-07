//! The live approval fixture: one tenant, workspace, fleet, person and gate.
//!
//! Split from `integration_workspace_approvals.rs` at the seam the file cap
//! draws: that file and `integration_workspace_approvals_listing.rs` both seed
//! through it, so it sits beside them rather than inside either.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};

use crate::integration_workspace_approvals::SUBJECT;

/// The seeded gate's sweeper deadline, in the millisecond epoch the column
/// stores. Far enough past this fixture's `created_at` of 1 that no test seeding
/// through it races the timeout sweeper; no assertion reads it back.
const GATE_TIMEOUT_AT: i64 = 10_000;

pub(crate) struct Fixture {
    lane: TestDatabase,
    pub(crate) database: Db,
    tenant: String,
    pub(crate) workspace: Uuid7,
    pub(crate) fleet: String,
    user: String,
    key: String,
    pub(crate) gate: String,
    pub(crate) action: String,
    event: String,
    pub(crate) token: String,
    /// The signed-in person this fixture seeds and authenticates as.
    pub(crate) subject: &'static str,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        Self::create_as(SUBJECT).await
    }

    /// A fixture whose person is `subject`.
    pub(crate) async fn create_as(subject: &'static str) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: mint_id(),
            user: mint_id(),
            key: mint_id(),
            gate: mint_id(),
            action: mint_id(),
            event: mint_id(),
            token: format!("agt_t{token_bits}"),
            subject,
            lane,
        }
    }

    pub(crate) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Approval live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'approval', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'approval-live@example.test', 1, 1) \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1) \
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($7::uuid, $2::uuid, $1::uuid, 'approval', '# fixture', '{}', \
                       'active', 1, 1) \
             ) \
             INSERT INTO core.fleet_approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind, \
                proposed_action, evidence, blast_radius, timeout_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($8::uuid, $7::uuid, $2::uuid, $9, 'git', 'push', 'tool', \
                     'open a pull request', '{}', 'one repository', $11, '', 'pending', \
                     '', 1, NULL, $10, 0, 32)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(&self.fleet)
        .bind(&self.gate)
        .bind(&self.action)
        .bind(&self.event)
        .bind(GATE_TIMEOUT_AT)
        .execute(&mut *connection)
        .await
        .expect("the authenticated approval gate seeds");
    }

    /// A second pending gate, later and of another kind.
    ///
    /// One row cannot show a filter narrowing or a page resuming: both look
    /// identical to an unfiltered single-row read.
    pub(crate) async fn seed_second_gate(&self) -> String {
        self.seed_gate("spend", 2).await
    }

    /// One more pending gate, of `kind`, raised at `created_at`.
    ///
    /// The instant is a parameter because the tie-break needs two rows sharing
    /// one: a cursor that carried only the instant would skip the second, and a
    /// fixture whose rows all differ cannot tell that apart from a correct one.
    pub(crate) async fn seed_gate(&self, kind: &str, created_at: i64) -> String {
        self.seed_gate_for(&self.fleet, kind, created_at).await
    }

    /// The same, under a fleet the caller names.
    pub(crate) async fn seed_gate_for(&self, fleet: &str, kind: &str, created_at: i64) -> String {
        let second = mint_id();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleet_approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind, \
                proposed_action, evidence, blast_radius, timeout_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'stripe', 'charge', $7, \
                     'refund a customer', '{}', 'one account', $6, '', 'pending', \
                     '', $8, NULL, $5, 0, 32)",
        )
        .bind(&second)
        .bind(fleet)
        .bind(self.workspace.as_str())
        .bind(mint_id())
        .bind(mint_id())
        .bind(GATE_TIMEOUT_AT)
        .bind(kind)
        .bind(created_at)
        .execute(&mut *connection)
        .await
        .expect("the second approval gate seeds");
        second
    }

    /// A second fleet in this workspace, so a fleet filter has something to
    /// exclude rather than merely something to match.
    pub(crate) async fn seed_second_fleet(&self) -> String {
        let other = mint_id();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'approval-other', '# fixture', '{}', \
                     'active', 1, 1)",
        )
        .bind(&other)
        .bind(self.workspace.as_str())
        .bind(&self.tenant)
        .execute(&mut *connection)
        .await
        .expect("the second fleet seeds");
        other
    }

    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("SET LOCAL fleet.allow_gate_purge = 'on'")
            .execute(&mut *transaction)
            .await
            .expect("the fixture opts into the guarded approval purge");
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
