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
use afd_db::test_util::{FleetSeed, TestDatabase, mint_id};

use crate::integration_workspace_approvals::SUBJECT;

/// The seeded gates' expiry: 2100-01-01, far past any run, so no test seeding
/// through it races the expiry sweeper or a locking validity check; no assertion
/// reads it back.
const GATE_EXPIRES_AT: &str = "2100-01-01T00:00:00Z";

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
               INSERT INTO identity.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Approval live', now(), now()) \
             ), workspace AS ( \
               INSERT INTO identity.workspaces (id, tenant_id, name, created_at) \
               VALUES ($2::uuid, $1::uuid, 'approval', now()) \
             ), person AS ( \
               INSERT INTO identity.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'approval-live@example.test', now(), now()) \
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
        .expect("the authenticated approval tenant seeds");
        FleetSeed::new(
            &self.fleet,
            self.workspace.as_str(),
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("approval")
        .at(afd_db::test_util::SEEDED_AT)
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("the approval fleet seeds");
        sqlx::query(
            "INSERT INTO fleet.approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type, \
                proposed_action, evidence, blast_radius, expires_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push', 'tool', \
                     'open a pull request', '{}', 'one repository', $6::timestamptz, '', \
                     'pending', '', '1970-01-01T00:00:00.001Z', \
                     '1970-01-01T00:00:00.001Z', $5, 0, 32)",
        )
        .bind(&self.gate)
        .bind(&self.fleet)
        .bind(self.workspace.as_str())
        .bind(&self.action)
        .bind(&self.event)
        .bind(GATE_EXPIRES_AT)
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

    /// One more pending gate, of `kind`, raised `created_at` milliseconds past the
    /// epoch.
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
            "INSERT INTO fleet.approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type, \
                proposed_action, evidence, blast_radius, expires_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'stripe', 'charge', $7, \
                     'refund a customer', '{}', 'one account', $6::timestamptz, '', \
                     'pending', '', to_timestamp($8::bigint / 1000.0), \
                     to_timestamp($8::bigint / 1000.0), $5, 0, 32)",
        )
        .bind(&second)
        .bind(fleet)
        .bind(self.workspace.as_str())
        .bind(mint_id())
        .bind(mint_id())
        .bind(GATE_EXPIRES_AT)
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
        FleetSeed::new(
            &other,
            self.workspace.as_str(),
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("approval-other")
        .at(afd_db::test_util::SEEDED_AT)
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("the second fleet seeds");
        other
    }

    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
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
