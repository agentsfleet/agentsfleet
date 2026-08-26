//! §7's scenario: a booted daemon, a funded fleet, and an enrolled runner.
//!
//! # A database per scenario, and the daemon pointed at it
//!
//! Every other live-datastore suite here creates a database per test and drops
//! it, and this one has to as well — but the seam is different, because §7
//! proves a runner against the daemon that SERVES rather than against a store
//! this file built. So the database is created and migrated first and `boot` is
//! handed it through `DATABASE_URL_API`: the daemon opens the fixture's
//! database, not the other way round.
//!
//! It is not tidiness. Sharing the lane database made the assignment pass see
//! TEN candidate fleets — every scenario an earlier run had left behind — and
//! `ORDER BY … created_at ASC` correctly handed back the oldest one, which
//! carried a config from before this file seeded a parseable document. The
//! symptom was `UZ-INTERNAL-003` on a fleet that had just been written
//! correctly, and no amount of re-reading THIS scenario's seed could explain
//! it, because the row being read belonged to a different one.
//!
//! Redis has no per-test equivalent and stays shared. That is safe for the same
//! reason it is safe next door: the candidate scan filters ready marks through
//! `core.fleets` in the daemon's own database, so another scenario's mark
//! cannot survive the join — and [`Scenario::cleanup`] clears this one's anyway,
//! so a bounded peek is never crowded by suites that have finished.
//!
//! # Why the clock is real
//!
//! The store suites freeze time at `ENROLLED_AT` because they call verbs
//! directly and pass the instant in. Here the daemon reads its own clock, so a
//! frozen seed would place the readiness mark and the lease deadline years
//! apart and the assignment pass would correctly find nothing. Everything below
//! is stamped from [`afd_core::clock::now`] for that reason, which is also why
//! nothing here asserts on an absolute instant.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_fleet::Runners;
use afd_redis::{FleetStreams, ReadyIndex};
use agentsfleetd::serve::{Booted, boot};
use agentsfleetd::supervisor::Supervisor;
use sqlx::AssertSqlSafe;

use crate::e2e_db::{admin, fresh_database};
use crate::e2e_seed::{
    DEEP_POOL, enrolment, seed_fleet, seed_model_rate, seed_platform_default, seed_provider_key,
    seed_wallet,
};

use crate::support::{IDENTITY, install_subscriber};

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Where the lane publishes the TLS Redis it brought up.
const REDIS_LANE_KNOB: &str = "TEST_REDIS_URL";

/// Where the lane extracted the Redis certificate authority to.
const REDIS_CA_LANE_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The port that asks the kernel to choose one.
///
/// Bind-and-hold: `boot` binds the listener itself and reports the address it
/// got, so no test allocates a number, closes it, and races another test to
/// re-bind it.
const EPHEMERAL: u16 = 0;

/// Sixty-four hex characters. Boot validates the key; nothing here decrypts.
pub(crate) const GOOD_KEK: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Distinguishes scenarios built by one process, so two never share a fleet.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The provider a seeded catalogue row is filed under.
pub(crate) const PROVIDER: &str = "anthropic";

/// The model a seeded fleet runs, and the catalogue row that prices it.
pub(crate) const MODEL: &str = "claude-fixture";

/// The billing posture a seeded lease records.
pub(crate) const POSTURE: &str = "platform";

/// The actor every seeded event carries.
pub(crate) const ACTOR: &str = "fixture:operator";

/// The type every seeded event carries.
///
/// `chat`, not the `steer` the store suites next door use. `EventType::parse`
/// is a CLOSED set and the pull path ends any delivery it cannot name — those
/// suites call `Leases::select` directly and never reach that check, so their
/// spelling has never had to be one the daemon executes. A §7 scenario does
/// reach it, and an unsupported type is answered as no-work, which is correct
/// behaviour and a fixture defect here.
pub(crate) const EVENT_TYPE: &str = "chat";

/// The body every seeded event carries.
pub(crate) const REQUEST_JSON: &str = r#"{"prompt":"fixture"}"#;

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    std::env::var(knob).unwrap_or_else(|_unset| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// An environment pointing the daemon at `database` and the lane's Redis, on an
/// ephemeral port.
///
/// The database is a parameter rather than the lane knob: each scenario boots
/// the daemon against a database it created, so the two cannot be the same
/// value and passing the knob would silently restore the shared-state bug the
/// module documentation describes.
fn daemon_environment(database: &str) -> MapEnv {
    MapEnv::from_pairs(
        [
            ("DATABASE_URL_API", database),
            ("REDIS_URL_API", lane(REDIS_LANE_KNOB).as_str()),
            ("REDIS_TLS_CA_CERT_FILE", lane(REDIS_CA_LANE_KNOB).as_str()),
            ("ENCRYPTION_MASTER_KEY", GOOD_KEK),
        ]
        .into_iter()
        .chain(IDENTITY),
    )
}

/// The lane's Redis, as a configuration a second client can be built from.
///
/// The activity suite needs a SUBSCRIBER alongside the daemon's own connection,
/// and `Booted` hands out a `Redis` rather than the config it was opened with —
/// so the knobs are read again here rather than reached back through the
/// daemon. Same three values `daemon_environment` passes it, which is what
/// keeps the subscriber pointed at the server the publish lands on.
pub(crate) fn redis_config() -> afd_redis::RedisConfig {
    afd_redis::RedisConfig::from_url(afd_redis::RedisRole::Default, lane(REDIS_LANE_KNOB))
        .with_ca_cert_file(std::env::var(REDIS_CA_LANE_KNOB).ok().map(Into::into))
}

/// A fleet, workspace and tenant no other scenario in this lane will name.
///
/// Process id and a counter, in a version-7 spelling because every one of these
/// columns CHECKs the version nibble — a random UUID is refused by the schema
/// rather than by the code under test.
fn unique_ids() -> (String, String, String) {
    let run = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let id = |slot: u32| format!("0195b4ba-8d3a-7{run:03x}-8abc-{pid:08x}{slot:04x}");
    (id(1), id(2), id(3))
}

/// Everything one end-to-end run needs, and the daemon serving it.
///
/// Holds `Booted` rather than just the address: its pool and queue are the
/// SAME handles the daemon answers through, so a state assertion reads what the
/// request actually wrote instead of what a second connection can see.
pub(crate) struct Scenario {
    /// The daemon under test. Dropped last, after the supervisor is shut down.
    pub(crate) booted: Booted,
    /// The lane's admin URL, for dropping this scenario's database.
    admin_url: String,
    /// The database this scenario created and the daemon was pointed at.
    database_name: String,
    /// Where to send requests, already spelled as an origin.
    pub(crate) base: String,
    /// The fleet holding the seeded event.
    pub(crate) fleet: String,
    /// Its workspace.
    pub(crate) workspace: String,
    /// Its billing tenant.
    pub(crate) tenant: String,
    /// The entry id the append produced.
    pub(crate) event_id: String,
    /// The enrolled runner's durable identifier.
    pub(crate) runner_id: Uuid7,
    /// Its bearer token, revealed once at enrolment.
    pub(crate) token: String,
    /// The instant the seed was stamped with.
    pub(crate) seeded_at: UnixMillis,
}

/// Boots the daemon and seeds one funded fleet with one event and one runner.
///
/// The order is the daemon's own: nothing is put on the stream until the fleet
/// row it joins against exists, because the assignment pass filters candidates
/// through `core.fleets` and a mark with no row makes a correct refusal look
/// like a broken poll.
pub(crate) async fn scenario(supervisor: &mut Supervisor) -> Scenario {
    install_subscriber();
    let admin_url = lane(DATABASE_LANE_KNOB);
    let database_name = format!(
        "afd_e2e_{}_{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let database_url = fresh_database(&admin_url, &database_name).await;

    let booted = boot(&daemon_environment(&database_url), EPHEMERAL, supervisor)
        .await
        .expect("the lane's Postgres and Redis are up");
    let base = format!("http://{}", booted.address);
    let now = afd_core::clock::now();

    let (fleet, workspace, tenant) = unique_ids();
    seed_fleet(&booted, &fleet, &workspace, &tenant, now).await;
    seed_wallet(&booted, &tenant, DEEP_POOL, now).await;
    seed_model_rate(&booted, now).await;
    seed_platform_default(&booted, &workspace, now).await;
    seed_provider_key(&booted, &workspace, now).await;

    // Through the production verb, not an INSERT: enrolment mints the token
    // and stores only its digest, and a seeded row would let the suite present
    // a credential the daemon's own minting never produced.
    let enrolled = Runners::new(booted.database.clone(), Entropy::new())
        .register(&enrolment(), now)
        .await
        .expect("enrolment must succeed");

    let event_id = enqueue(&booted, &fleet, &workspace, now).await;

    Scenario {
        base,
        fleet,
        workspace,
        tenant,
        event_id,
        runner_id: enrolled.runner_id,
        token: enrolled.token.expose().to_owned(),
        seeded_at: now,
        booted,
        admin_url,
        database_name,
    }
}

impl Scenario {
    /// Takes the tenant's wallet to zero.
    ///
    /// A row holding ZERO, not a missing row: the credits gate draws that
    /// distinction deliberately and a tenant with NO wallet is ADMITTED,
    /// because an unprovisioned tenant is an operator gap and refusing every
    /// one of them would turn that into an outage. A test proving the refusal
    /// has to seed the exhausted case, not the absent one.
    pub(crate) async fn drain_wallet(&self) {
        seed_wallet(&self.booted, &self.tenant, 0, self.seeded_at).await;
    }

    /// Drops this scenario's database and clears its readiness mark.
    ///
    /// Both halves, because the two datastores fail differently: a leaked
    /// database is a slow accumulation the lane's reset eventually clears, while
    /// a leaked ready mark competes for the next poll's bounded peek in the SAME
    /// run. Takes `self` so the pools close before the drop — `WITH (FORCE)`
    /// would evict them, and closing is the difference between a clean teardown
    /// and one that relies on eviction.
    pub(crate) async fn cleanup(self) {
        let Self {
            booted,
            admin_url,
            database_name,
            fleet,
            ..
        } = self;

        let index = ReadyIndex::new(booted.queue.clone());
        if let Ok(token) = index.mark(&fleet, &fleet).await {
            let _cleared = index.clear_if_unchanged(&fleet, &token).await;
        }
        drop(booted);

        admin(
            &admin_url,
            AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {database_name} WITH (FORCE)"
            )),
        )
        .await;
    }
}

/// Puts one event on the fleet's stream and marks the fleet ready.
///
/// Both halves: ingress appends and marks in one path, so a mark with no entry
/// is a state the daemon never produces and a fixture that made one would be
/// testing a shape nothing ships.
async fn enqueue(booted: &Booted, fleet: &str, workspace: &str, now: UnixMillis) -> String {
    let streams = FleetStreams::new(booted.queue.clone());
    streams
        .ensure_group(fleet)
        .await
        .expect("the consumer group must exist before a read");
    let created = now.as_millis().to_string();
    let id = streams
        .append(
            fleet,
            &[
                ("type", EVENT_TYPE),
                ("actor", ACTOR),
                ("workspace_id", workspace),
                ("request", REQUEST_JSON),
                ("created_at", &created),
            ],
        )
        .await
        .expect("the event must append");
    // The mark's token is the fleet id, as every producer in this workspace
    // spells it: `clear_if_unchanged` compares it, so a scenario that marked
    // under a different value could not clear its own entry.
    ReadyIndex::new(booted.queue.clone())
        .mark(fleet, fleet)
        .await
        .expect("the readiness mark must land");
    id.as_str().to_owned()
}
