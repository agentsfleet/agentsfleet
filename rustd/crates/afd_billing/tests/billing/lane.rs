//! The lane's database, one seeded tenant, workspace and fleet, and the rows a
//! money read needs to find.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated. Isolation is the minted identifiers: every
//! statement this suite exercises carries the tenant, the workspace or the
//! fleet in its predicate, and no other test can name them.
//!
//! The wallet and the usage lines are written here directly, because their
//! production writers live in `afd_tenant` (signup) and the lease and setup
//! chat paths, which depend on this crate rather than the other way round.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_billing::tenant::Billing;
use afd_billing::{Accounts, Posture};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    FleetSeed, ModelSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id, mint_uuid7,
};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

/// The status a seeded fleet holds; billing reads only its name.
const FLEET_ACTIVE: &str = "active";

/// The name the seeded fleet carries, which a charge snapshots.
pub(crate) const FLEET_NAME: &str = "billing-suite-fleet";

/// The provider every catalogue row this suite writes is filed under.
pub(crate) const PROVIDER: &str = "anthropic";

/// Writes one wallet row, as signup does.
const INSERT_WALLET: &str = "\
INSERT INTO billing.wallets (tenant_id, balance_nanos, exhausted_at, created_at, updated_at)
VALUES ($1::uuid, $2, $3, now(), $4)";

/// Writes one charge row with chosen stamps, as the renewal statement would
/// have accumulated it.
const INSERT_LINE: &str = "\
INSERT INTO billing.usage_lines
  (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
   provider, model, posture, requested_at, input_token_count, cached_input_token_count,
   cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
   output_token_count, runtime_ms, unpriced_token_count, charged_nanos, created_at, updated_at)
VALUES ($1::uuid, $2, $3::uuid, 0, $4::uuid, $5::uuid, $6::uuid, $7, $8, $9, $9, $10, $11,
        0, 0, 0, 0, 0, 0, 0, 0, $12, $11, $11)";

/// The context window a seeded catalogue row declares; nothing here reads it.
const CONTEXT_CAP_TOKENS: i32 = 200_000;

/// A charge row's shape, for [`Lane::charge`].
/// One fleet usage line, by what it cost and when it was written.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line {
    pub(crate) nanos: i64,
    pub(crate) created_at: Timestamp,
}

pub(crate) struct Lane {
    pub(crate) pool: Db,
    pub(crate) accounts: Accounts,
    pub(crate) billing: Billing,
    pub(crate) tenant: Uuid7,
    pub(crate) workspace: Uuid7,
    pub(crate) fleet: Uuid7,
}

impl Lane {
    /// Seeds a tenant, one workspace and one fleet inside the lane's shared
    /// database. No wallet: a suite that wants one calls [`Self::wallet`].
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let lane = Self {
            accounts: Accounts::new(pool.clone()),
            billing: Billing::new(pool.clone()),
            tenant: mint_uuid7(),
            workspace: mint_uuid7(),
            fleet: mint_uuid7(),
            pool,
        };
        let mut connection = lane.pool.acquire().await.expect("the lane answers");
        TenantSeed::new(lane.tenant.as_str())
            .insert(&mut *connection)
            .await
            .expect("seeding the tenant");
        WorkspaceSeed::new(lane.workspace.as_str(), lane.tenant.as_str())
            .insert(&mut *connection)
            .await
            .expect("seeding the workspace");
        FleetSeed::new(
            lane.fleet.as_str(),
            lane.workspace.as_str(),
            lane.tenant.as_str(),
            FLEET_ACTIVE,
        )
        .name(FLEET_NAME)
        .insert(&mut *connection)
        .await
        .expect("seeding the fleet");
        lane
    }

    /// Opens the tenant's wallet at `balance`, last moved at `updated_at`.
    pub(crate) async fn wallet(
        &self,
        balance: i64,
        exhausted_at: Option<Timestamp>,
        updated_at: Timestamp,
    ) {
        sqlx::query(INSERT_WALLET)
            .bind(self.tenant.as_str())
            .bind(balance)
            .bind(exhausted_at.map(jiff_sqlx::ToSqlx::to_sqlx))
            .bind(updated_at.to_sqlx())
            .execute(&mut *self.pool.acquire().await.expect("the lane answers"))
            .await
            .expect("opening the wallet");
    }

    /// Writes one usage line against the seeded fleet, answering its id.
    pub(crate) async fn line(&self, line: Line) -> Uuid7 {
        let mut connection = self.pool.acquire().await.expect("the lane answers");
        self.line_on(&mut *connection, line).await
    }

    /// [`Self::line`], written on `executor` — inside a caller's transaction,
    /// so a later read in it sees the row.
    pub(crate) async fn line_on<'e>(
        &self,
        executor: impl sqlx::PgExecutor<'e>,
        line: Line,
    ) -> Uuid7 {
        let id = mint_uuid7();
        sqlx::query(INSERT_LINE)
            .bind(id.as_str())
            .bind(afd_billing::sql::work::FLEET_LEASE)
            .bind(mint_uuid7().as_str())
            .bind(self.tenant.as_str())
            .bind(self.workspace.as_str())
            .bind(self.fleet.as_str())
            .bind(FLEET_NAME)
            .bind(mint_id())
            .bind(PROVIDER)
            .bind(Posture::Platform.as_str())
            .bind(line.created_at.to_sqlx())
            .bind(line.nanos)
            .execute(executor)
            .await
            .expect("writing a usage line");
        id
    }

    /// Files a model in the catalogue at three prices, answering its name.
    pub(crate) async fn priced_model(&self, input: i64, cached: i64, output: i64) -> String {
        let model = mint_id();
        ModelSeed::new(&mint_id(), PROVIDER, &model, CONTEXT_CAP_TOKENS)
            .rates(input, cached, output)
            .insert(&mut *self.pool.acquire().await.expect("the lane answers"))
            .await
            .expect("filing a catalogue row");
        model
    }
}
