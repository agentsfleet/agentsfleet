//! A tenant with keys of its own, over the lane's migrated database.
//!
//! Sibling of `preference_lane.rs` and deliberately not an extension of it: a
//! preference hangs from a `(user, workspace)` pair and a key hangs from a
//! tenant, so the two fixtures seed different rows and sharing one would make
//! each suite carry the other's scaffolding.
//!
//! # Every identifier is minted
//!
//! The lane's database is SHARED — `TestDatabase::shared`, never one per test —
//! so two suites running in parallel address the same tables. What keeps them
//! apart is that each mints its own tenant, and every read here is scoped by
//! it. Nothing carries an `ON CONFLICT` arm: a collision would mean the minting
//! is broken, and absorbing it would hide that.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use afd_core::id::Uuid7;
use afd_core::paging::{Cursor, Page};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TenantSeed, TestDatabase};
use afd_tenant::apikey::{ApiKeySort, ApiKeys, Description, KeyName, Listing, MintRequest};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

/// The instant this fixture's first key is stamped with.
pub(crate) const NOW: Timestamp = Timestamp::constant(1_770_000_000, 0);

/// How many keys a page answers.
///
/// Three against a corpus of five, so the walk takes exactly two pages and the
/// boundary falls in the middle of the corpus rather than at either end — the
/// only placement where a skip and a repeat are both observable.
pub(crate) const PAGE_SIZE: u32 = 3;

/// The subject the fixture keys record as their minter.
const MINTED_BY: &str = "fixture|apikey-paging";

/// A migrated database, the key store over it, and a tenant to hang keys from.
pub(crate) struct ApiKeyLane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) keys: ApiKeys,
    pub(crate) tenant: Uuid7,
}

impl ApiKeyLane {
    /// Seeds one tenant inside the shared database and opens the store over it.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let keys = ApiKeys::new(pool.clone(), Entropy::new());
        let tenant = mint();

        let lane = Self {
            database,
            pool,
            keys,
            tenant,
        };
        lane.seed_tenant().await;
        lane
    }

    /// Mints one key called `name`, then backdates it to `at`.
    ///
    /// The instant is a PARAMETER rather than the database clock's stamp,
    /// which is the whole reason this helper exists: every claim the paging
    /// suite makes is a claim about the ordering of `created_at`, and the
    /// insert's `now()` writes whatever instants the inserts happened to take.
    /// So the store mints the key and the fixture moves its stamp.
    pub(crate) async fn mint_key(&self, name: &str, at: Timestamp) {
        let request = MintRequest {
            tenant: &self.tenant,
            name: KeyName::parse(name).expect("the fixture names are well formed"),
            description: Description::parse(None).expect("an absent description is legal"),
            created_by: MINTED_BY,
        };
        let minted = self
            .keys
            .mint(&request, at)
            .await
            .expect("minting a fixture key");
        sqlx::query("UPDATE identity.api_keys SET created_at = $2 WHERE id = $1::uuid")
            .bind(minted.id.as_str())
            .bind(at.to_sqlx())
            .execute(&mut *self.connection().await)
            .await
            .expect("backdating a fixture key");
    }

    /// One page of this tenant's keys.
    pub(crate) async fn page(&self, sort: ApiKeySort, cursor: Option<Cursor>) -> Listing {
        let page = Page {
            cursor,
            limit: PAGE_SIZE,
            sort,
        };
        self.keys
            .list(&self.tenant, &page)
            .await
            .expect("the listing must run")
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

    /// Seeds the tenant every key hangs from.
    async fn seed_tenant(&self) {
        TenantSeed::new(self.tenant.as_str())
            .insert(&mut *self.connection().await)
            .await
            .expect("seeding a tenant");
    }
}

/// A fresh identifier, so no two fixtures name each other's rows.
fn mint() -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(NOW, bytes).expect("a well-formed identifier")
}
