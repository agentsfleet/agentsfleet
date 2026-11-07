//! The lane's database, its Dragonfly, and the rows a gate needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated.
//!
//! # Its own fixture, and not `afd_fleet`'s
//!
//! The crate under test does not depend on the runner plane, so neither does
//! its suite. Seeding a tenant, a workspace and a fleet is three statements;
//! borrowing them from `afd_fleet::tests` would put leases, money and policy
//! behind a test about a person answering a question.
//!
//! # One tenant, and a workspace and fleet per test
//!
//! The tenant is shared scaffolding: every test writes the same row under
//! `ON CONFLICT (id) DO NOTHING`, whichever runs first wins, and nothing
//! asserts over it. The workspace and fleet are minted per test, because tests
//! DO assert over those — a count of a fleet's events, a listing of a
//! workspace's queue — and on one shared database a fixed pair puts every
//! sibling's rows inside this test's answer. That was a real failure, three
//! runs in a row on two different tests, and it is the line: shared when every
//! writer writes the same row, minted the moment a test reads a set.
//!
//! There is no teardown. `TestDatabase::shared` owns no database to drop, so a
//! `cleanup` here would be a call every test makes that does nothing, implying
//! an isolation boundary that is not there.
//!
//! # The sweeper is global, so its tests take a lock
//!
//! `Inbox::expire` is a system-wide statement by design — `WHERE resolved_at
//! IS NULL AND expires_at <= now()`, no workspace or fleet in it — which is what
//! a sweeper must be in production and what no amount of identifier minting can
//! isolate. A test that sweeps therefore expires every OTHER test's lapsed
//! gate, and a test whose premise is a gate past its window finds it already
//! resolved. Those two groups take [`sweeper_exclusive`] and run one at a time;
//! everything else still runs concurrently.
//!
//! Dragonfly carries activity frames and wake notifications. Continuations
//! commit to the admissions ledger; notification failure cannot undo them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::time::Duration;

use afd_approval::Inbox;
use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TestDatabase, WorkspaceSeed};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_fleet_lifecycle::FleetStatus;
use jiff::Timestamp;

#[path = "gate_lane_read.rs"]
mod read;

#[path = "gate_lane_seed.rs"]
mod seed;

pub(crate) use self::seed::Window;

/// The environment knob naming the lane's Dragonfly.
const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";

/// The environment knob naming its certificate authority.
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// Serialises the tests the global sweeper cannot be isolated from.
///
/// Held for the whole test body by anything that calls `Inbox::expire`, and by
/// anything that seeds a gate already past its window and expects to find it
/// PENDING. Both sides are needed: the lock exists to keep those two groups
/// apart, and a lock only one side takes keeps nothing apart at all.
///
/// A `tokio` mutex rather than the standard library's, because the guard is
/// held across `await` points, and it does not poison — one failing test leaves
/// the rest runnable rather than turning a single red into a suite of them.
pub(crate) async fn sweeper_exclusive() -> tokio::sync::MutexGuard<'static, ()> {
    static SWEEPER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    SWEEPER.lock().await
}

/// The instant a fixture row is raised at, and identifiers are minted at.
///
/// Raised-at only: whether a gate has LAPSED is the database's clock against
/// the row's `expires_at`, which [`Window`] places relative to that clock.
pub(crate) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The tenant every gate in this suite hangs from.
///
/// Fixed, readable and SHARED — see the module note. Version-7 shaped so the
/// schema's `ck_*_id_uuidv7` admits it.
const TENANT: &str = "0195b4ba-8d3a-7a11-8abc-000000000001";

/// A migrated database, a queue, and one fleet to hang gates from.
pub(crate) struct Lane {
    /// Held, not used: `TestDatabase::shared` is what resolves the lane's URL,
    /// and dropping it early would close nothing but would make the pool's
    /// provenance unreadable.
    _database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) queue: Dragonfly,
    pub(crate) inbox: Inbox,
    /// The fleet every seeded gate belongs to.
    pub(crate) fleet: Uuid7,
    /// Its workspace.
    pub(crate) workspace: Uuid7,
    /// Its tenant, shared by every lane in the suite.
    pub(crate) tenant: Uuid7,
}

impl Lane {
    /// A workspace and fleet of this test's OWN, under the shared tenant.
    ///
    /// The only constructor, because on one shared database every alternative
    /// is a race. A lane on fixed identifiers puts every test's gates in every
    /// other test's fleet: a count of that fleet's events reads a sibling's
    /// continuation, and a sweep over that workspace expires the gate a
    /// sibling is about to answer. Both are real failures this suite has had.
    /// Minting the pair costs three seeding statements and removes the class.
    pub(crate) async fn isolated() -> Self {
        Self::open(mint().as_str().to_owned(), mint().as_str().to_owned()).await
    }

    /// Opens the lane over `workspace` and `fleet`, seeding both.
    async fn open(workspace: String, fleet: String) -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let queue = afd_dragonfly::test_util::connect_live(&dragonfly_config())
            .await
            .expect("the lane's Dragonfly must be reachable");

        let lane = Self {
            inbox: Inbox::new(
                pool.clone(),
                queue.clone(),
                afd_admission::Admissions::for_tests(pool.clone(), queue.clone()),
            ),
            _database: database,
            pool,
            queue,
            fleet: Uuid7::parse(&fleet).expect("a fixture fleet id is well formed"),
            workspace: Uuid7::parse(&workspace).expect("a fixture workspace id is well formed"),
            tenant: Uuid7::parse(TENANT).expect("the fixture tenant id is well formed"),
        };
        lane.seed_fleet().await;
        lane
    }

    /// One pooled connection, for the fixture's own reads and writes.
    pub(super) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant, workspace and fleet every gate hangs from.
    async fn seed_fleet(&self) {
        let mut connection = self.connection().await;
        // Shared, so written under `ON CONFLICT` rather than through
        // `TenantSeed`, whose insert refuses a duplicate: whichever lane runs
        // first writes it, and every later one finds it there.
        sqlx::query(
            "INSERT INTO identity.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'fixture', now(), now())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        // Named after its own id, which `WorkspaceSeed` does by default:
        // `uq_workspaces_tenant_id_name` is unique per tenant, and every
        // isolated lane shares the tenant, so a constant name would collide.
        WorkspaceSeed::new(self.workspace.as_str(), self.tenant.as_str())
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");

        FleetSeed::new(
            self.fleet.as_str(),
            self.workspace.as_str(),
            self.tenant.as_str(),
            FleetStatus::Active.as_str(),
        )
        .name("fixture-fleet")
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("seeding a fleet");
    }
}

/// The lane's Dragonfly configuration.
pub(crate) fn dragonfly_config() -> DragonflyConfig {
    let url = ProcessEnv.get(DRAGONFLY_URL_KNOB).unwrap_or_else(|| {
        panic!("{DRAGONFLY_URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(ProcessEnv.get(DRAGONFLY_CA_KNOB).map(Into::into))
        .with_request_timeout(Duration::from_secs(5))
}

/// A queue that will not answer: a loopback port nobody listens on.
///
/// `Dragonfly::unreachable` opens no socket, and this address makes sure the
/// first command opens none either. A handle over the LANE's Dragonfly would
/// connect lazily and succeed, which is a live queue wearing a dead name; the
/// lane's Dragonfly is shared by every test binary, so it is never the one taken
/// down.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A handle over a Dragonfly that will not take a single command.
pub(crate) fn dead_queue() -> Dragonfly {
    Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail")
}

/// A fresh identifier, so no two fixtures can name each other's rows.
///
/// Through `afd_db`'s minting helper rather than a local entropy draw: this
/// crate has no reason to depend on the crypto crate, and the helper already
/// shapes a value both `ck_*_id_uuidv7` and [`Uuid7`] accept.
pub(crate) fn mint() -> Uuid7 {
    Uuid7::parse(&afd_db::test_util::mint_id()).expect("a minted identifier is well formed")
}
