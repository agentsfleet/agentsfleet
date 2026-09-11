//! Building the precondition a lease poll needs, once per fleet.
//!
//! # Three rows, a group, and a mark
//!
//! A fleet is leasable only when all of these hold, and any one missing makes
//! the assignment pass look broken when it is the fixture that is:
//!
//! 1. `core.tenants`, `core.workspaces` and `core.fleets` exist AND AGREE — a
//!    fleet carries its workspace's tenant through a composite key.
//! 2. The fleet's stream has a consumer group, created before any read.
//! 3. The readiness index carries a mark, or the poll returns before Postgres.
//!
//! No store verb creates a fleet: that is the tenant plane's job, so these
//! three rows are inserted directly, exactly as `afd_fleet`'s own lease suites
//! seed them.
//!
//! # Why the run prefix names rows and salts identifiers
//!
//! Every table here CHECKs the UUID version nibble, so an identifier cannot
//! carry a text prefix and still be accepted. The prefix lives in the `name`
//! column so [`sweep`] can find the rows, and it also feeds the UUID entropy so
//! a later process cannot adopt leaked rows from an earlier run. The resulting
//! identifiers remain schema-legal `UUIDv7` values.
//!
//! # Placement tags keep concurrent runs apart
//!
//! `Leases::select` has no workspace or tenant in it — it peeks the GLOBAL
//! readiness set and filters candidates by `required_tags <@ labels`. A fleet
//! seeded with no tag is placeable by every other poller on the same datastore,
//! including another bench run. One tag per run costs one array element and
//! leaves the assignment pass under test rather than around it.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_redis::{FleetStreams, ReadyIndex, Redis};
use afd_runner::Runners;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use sha2::{Digest as _, Sha256};

use crate::error::Result;
use crate::fixture::RunPrefix;

/// Rows one seeded fleet inserts: its tenant, its workspace, and itself.
///
/// Named because the sweep counts rows and the ledger has to count the same
/// unit — see the fixture block in the result file.
pub const ROWS_PER_FLEET: u64 = 3;

/// Rows one enrolled runner inserts.
pub const ROWS_PER_RUNNER: u64 = 1;

/// The actor every bench-written event carries, seeded or steered.
///
/// One spelling, so a reader grepping a stream for bench entries has one
/// thing to grep for.
pub const BENCH_ACTOR: &str = "bench:steer";

/// The event type every seeded event carries.
const EVENT_TYPE: &str = "steer";

/// The body every bench-written event carries.
///
/// Generated content, never a tenant row echoed back: a fixture that copied
/// real request text would put customer data in a bench result (RULE PRI).
pub const BENCH_REQUEST_JSON: &str = "{\"prompt\":\"bench\"}";

/// The clock every seeded row is stamped with, and the instant a poll is given.
///
/// A fixed past instant rather than "now": the candidate query orders by
/// enrolment, and a population seeded across a moving clock would order by the
/// accident of how long seeding took. One constant for every lane, so no two
/// lanes seed against different clocks.
pub const SEEDED_AT: i64 = 1_767_225_600_000;

/// The status a leasable fleet carries.
const FLEET_STATUS: &str = "active";

/// Placeholder markdown a seeded fleet is created with.
const SOURCE_MARKDOWN: &str = "# bench";

/// Empty configuration for a seeded fleet.
const CONFIG_JSON: &str = "{}";

/// Concurrent workers a seeded runner declares.
///
/// One, because this lane measures how fast leases are ISSUED, not how many a
/// runner then executes: a higher ceiling would change what the runner does
/// after the poll and nothing about the poll.
const RUNNER_WORKERS: u32 = 1;

/// Identifier kind, so one index yields three distinct v7-shaped ids.
const KIND_TENANT: u32 = 1;

/// Identifier kind for a workspace.
const KIND_WORKSPACE: u32 = 2;

/// Identifier kind for a fleet.
const KIND_FLEET: u32 = 3;

/// A seeded fleet and the identifiers it was built from.
#[derive(Debug, Clone)]
pub struct SeededFleet {
    /// The fleet a poll can lease.
    pub fleet: String,
    /// Its workspace.
    pub workspace: String,
    /// Its billing tenant.
    pub tenant: String,
}

/// An identifier the schema's `uuidv7` CHECK accepts.
///
/// The run prefix prevents a later process from adopting rows an interrupted
/// process left behind. Kind and index keep every row distinct within a run.
pub(crate) fn identifier(prefix: &RunPrefix, kind: u32, index: u64) -> Result<String> {
    let digest: [u8; 32] =
        Sha256::digest(format!("{}:{kind}:{index}", prefix.as_str()).as_bytes()).into();
    let entropy: [u8; ENTROPY_LEN] =
        std::array::from_fn(|offset| digest.get(offset).copied().unwrap_or_default());
    Uuid7::encode(UnixMillis::from_millis(SEEDED_AT), entropy)
        .map(|id| id.as_str().to_owned())
        .map_err(crate::Error::from)
}

/// The tag that keeps this run's fleets placeable only by this run's runners.
#[must_use]
pub fn placement_tag(prefix: &RunPrefix) -> String {
    prefix.name("tag")
}

/// Seed a fleet's rows and its consumer group, with NOTHING on the stream.
///
/// What the steer lane wants: the fleet has to exist and its group has to
/// exist before a read, but the appends are the thing being measured and must
/// happen inside the window rather than during setup.
///
/// # Errors
///
/// Whatever Postgres or Redis refused.
pub async fn empty_fleet(
    database: &Db,
    queue: &Redis,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: i64,
) -> Result<SeededFleet> {
    let seeded = SeededFleet {
        fleet: identifier(prefix, KIND_FLEET, index)?,
        workspace: identifier(prefix, KIND_WORKSPACE, index)?,
        tenant: identifier(prefix, KIND_TENANT, index)?,
    };
    rows(database, prefix, &seeded, tag, now).await?;
    FleetStreams::new(queue.clone())
        .ensure_group(&seeded.fleet)
        .await?;
    Ok(seeded)
}

/// Seed one ready fleet: three rows, a consumer group, an event, a mark.
///
/// # Errors
///
/// Whatever Postgres or Redis refused, naming which.
pub async fn ready_fleet(
    database: &Db,
    queue: &Redis,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: i64,
) -> Result<SeededFleet> {
    let seeded = SeededFleet {
        fleet: identifier(prefix, KIND_FLEET, index)?,
        workspace: identifier(prefix, KIND_WORKSPACE, index)?,
        tenant: identifier(prefix, KIND_TENANT, index)?,
    };
    rows(database, prefix, &seeded, tag, now).await?;
    enqueue(queue, &seeded, now).await?;
    Ok(seeded)
}

/// The tenant, workspace and fleet rows, in the order the keys require.
async fn rows(
    database: &Db,
    prefix: &RunPrefix,
    seeded: &SeededFleet,
    tag: &str,
    now: i64,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&seeded.tenant)
    .bind(prefix.name("tenant"))
    .bind(now)
    .execute(&mut *connection)
    .await?;

    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&seeded.workspace)
    .bind(&seeded.tenant)
    .bind(prefix.name("workspace"))
    .bind(now)
    .execute(&mut *connection)
    .await?;

    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at, required_tags)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8, $9)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&seeded.fleet)
    .bind(&seeded.workspace)
    .bind(&seeded.tenant)
    .bind(prefix.name("fleet"))
    .bind(SOURCE_MARKDOWN)
    .bind(CONFIG_JSON)
    .bind(FLEET_STATUS)
    .bind(now)
    .bind(vec![tag.to_owned()])
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// One event on the fleet's stream, and the readiness mark that makes a poll
/// look at it.
///
/// Both halves, because either alone is a state the daemon never produces:
/// ingress appends and marks in one path.
async fn enqueue(queue: &Redis, seeded: &SeededFleet, now: i64) -> Result<()> {
    let streams = FleetStreams::new(queue.clone());
    streams.ensure_group(&seeded.fleet).await?;
    let created = now.to_string();
    streams
        .append(
            &seeded.fleet,
            &[
                ("type", EVENT_TYPE),
                ("actor", BENCH_ACTOR),
                ("workspace_id", seeded.workspace.as_str()),
                ("request", BENCH_REQUEST_JSON),
                ("created_at", created.as_str()),
            ],
        )
        .await?;
    // The token is the caller's to mint, and the ingress path uses the entry
    // it just appended. The fleet id serves here: this lane never reads the
    // token back, and a distinct value would only be a second thing to sweep.
    ReadyIndex::new(queue.clone())
        .mark(&seeded.fleet, &seeded.fleet)
        .await?;
    Ok(())
}

/// Enrol one runner carrying this run's placement tag.
///
/// # Errors
///
/// Whatever enrolment refused.
pub async fn runner(database: &Db, host: &str, tag: &str, now: i64) -> Result<Uuid7> {
    let runners = Runners::new(database.clone(), Entropy::new());
    let request = RegisterRequest {
        host_id: host.into(),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::AllowListEgress,
            registry_allowlist: Vec::new(),
            worker_count: RUNNER_WORKERS,
            extra_binds: Vec::new(),
        },
        // The placement tag, and nothing else: the candidate query filters
        // `required_tags <@ labels`, so a runner carrying extra labels would
        // still match and a runner missing this one never will.
        labels: vec![tag.into()],
    };
    Ok(runners
        .register(&request, UnixMillis::from_millis(now))
        .await?
        .runner_id)
}

#[cfg(test)]
mod tests;
