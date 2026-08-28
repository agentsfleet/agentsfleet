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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::paging::{Cursor, Page};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_tenant::apikey::{ApiKeySort, ApiKeys, Description, KeyName, Listing, MintRequest};

/// The instant this fixture's first key is stamped with.
pub(crate) const NOW_MS: i64 = 1_770_000_000_000;

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

    /// The instant this fixture's corpus starts at.
    pub(crate) const fn instant(&self) -> i64 {
        NOW_MS
    }

    /// Mints one key called `name`, stamped at `at_ms`.
    ///
    /// The instant is a PARAMETER rather than a clock read, which is the whole
    /// reason this helper exists: every claim the paging suite makes is a claim
    /// about the ordering of `created_at`, and a fixture that stamped "now"
    /// could only ever write one instant — or worse, a handful separated by
    /// however long the inserts happened to take.
    pub(crate) async fn mint_key(&self, name: &str, at_ms: i64) {
        let request = MintRequest {
            tenant: &self.tenant,
            name: KeyName::parse(name).expect("the fixture names are well formed"),
            description: Description::parse(None).expect("an absent description is legal"),
            created_by: MINTED_BY,
        };
        self.keys
            .mint(&request, UnixMillis::from_millis(at_ms))
            .await
            .expect("minting a fixture key");
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
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, $1::text, $2, $2)",
        )
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
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
    Uuid7::encode(UnixMillis::from_millis(NOW_MS), bytes).expect("a well-formed identifier")
}
