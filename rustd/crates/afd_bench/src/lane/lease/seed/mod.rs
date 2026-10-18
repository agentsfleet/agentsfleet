//! Building the precondition a lease poll needs, once per fleet.
//!
//! # Three rows, a group, and a mark
//!
//! A fleet is leasable only when all of these hold, and any one missing makes
//! the assignment pass look broken when it is the fixture that is:
//!
//! 1. `identity.tenants`, `identity.workspaces` and `fleet.fleets` exist AND
//!    AGREE, and the fleet carries the setup revision Save selects.
//! 2. The fleet's stream has a consumer group, created before any read.
//! 3. The readiness index carries a mark, or the poll returns before Postgres.
//!
//! No store verb creates a fleet: that is the tenant plane's job, so the rows
//! are written through `afd_db`'s shared seeds, the ones `afd_fleet`'s own
//! lease suites seed with. A schema change reaches the bench with them.
//!
//! # Why the run prefix is in the NAME and not the identifier
//!
//! Every table here CHECKs the UUID version nibble, so an identifier cannot
//! carry a text prefix and still be accepted. The prefix therefore lives in the
//! `name` column of all three rows, and `sweep` deletes by name. The
//! identifiers stay schema-legal and the sweep still recognises only its own
//! work.
//!
//! # Placement tags keep concurrent runs apart
//!
//! `Leases::select` has no workspace or tenant in it — it peeks the GLOBAL
//! readiness set and filters candidates by `required_tags <@ labels`. A fleet
//! seeded with no tag is placeable by every other poller on the same datastore,
//! including another bench run. One tag per run costs one array element and
//! leaves the assignment pass under test rather than around it.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed};
use afd_dragonfly::{Dragonfly, FleetStreams, ReadyIndex};
use afd_runner::Runners;
use afd_runner::heartbeat::holds::prove;
use afd_wire::event::{Entry, EventType};
use afd_wire::runner::{
    AssignedPolicy, CapabilityReport, HeartbeatRequest, HeldFleets, NetworkPolicy, RegisterRequest,
    SandboxTier,
};
use jiff::Timestamp;

use crate::error::{Result, seed_refused};
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

/// The event type every seeded event carries: a steer is a chat.
const EVENT_TYPE: EventType = EventType::Chat;

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
pub const SEEDED_AT: Timestamp = Timestamp::constant(SEEDED_AT_SECOND, 0);

/// [`SEEDED_AT`] in seconds since the epoch: 2026-01-01T00:00:00Z.
const SEEDED_AT_SECOND: i64 = 1_767_225_600;

/// The cgroup controllers a seeded runner reports it can confine with.
const CGROUP_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

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
/// The character after the second dash must be `7`. Process id plus index in
/// the final group gives four billion distinct fleets per process, which is
/// past anything the cardinality ladder climbs to.
pub(crate) fn identifier(kind: u32, index: u64) -> String {
    format!(
        "0195b4ba-8d3a-7{kind:03x}-8abc-{:04x}{index:08x}",
        std::process::id() & 0xffff
    )
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
/// Whatever Postgres or Dragonfly refused.
pub async fn empty_fleet(
    database: &Db,
    queue: &Dragonfly,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: Timestamp,
) -> Result<SeededFleet> {
    let seeded = identities(index);
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
/// Whatever Postgres or Dragonfly refused, naming which.
pub async fn ready_fleet(
    database: &Db,
    queue: &Dragonfly,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: Timestamp,
) -> Result<SeededFleet> {
    let seeded = identities(index);
    rows(database, prefix, &seeded, tag, now).await?;
    enqueue(queue, &seeded, EVENT_TYPE, now).await?;
    Ok(seeded)
}

/// The three identifiers one index yields.
pub(crate) fn identities(index: u64) -> SeededFleet {
    SeededFleet {
        fleet: identifier(KIND_FLEET, index),
        workspace: identifier(KIND_WORKSPACE, index),
        tenant: identifier(KIND_TENANT, index),
    }
}

/// The tenant, workspace and fleet rows, in the order the keys require.
///
/// The fleet carries the shared fixture's setup revision, whose definition
/// is a runtime config the parser accepts and whose model reference resolves
/// through the platform default, as a saved fleet's does.
pub(crate) async fn rows(
    database: &Db,
    prefix: &RunPrefix,
    seeded: &SeededFleet,
    tag: &str,
    now: Timestamp,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    let (tenant, workspace, fleet) = (
        prefix.name("tenant"),
        prefix.name("workspace"),
        prefix.name("fleet"),
    );
    TenantSeed::new(&seeded.tenant)
        .name(&tenant)
        .at(now)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    WorkspaceSeed::new(&seeded.workspace, &seeded.tenant)
        .name(&workspace)
        .at(now)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    FleetSeed::new(
        &seeded.fleet,
        &seeded.workspace,
        &seeded.tenant,
        afd_fleet::lease::sql::FLEET_STATUS_ACTIVE,
    )
    .name(&fleet)
    .tags(&[tag])
    .native_setup()
    .at(now)
    .insert(&mut *connection)
    .await
    .map_err(seed_refused)
}

/// One event of `event_type` on the fleet's stream, and the readiness mark
/// that makes a poll look at it.
///
/// Both halves, because either alone is a state the daemon never produces:
/// ingress appends and marks in one path.
pub(crate) async fn enqueue(
    queue: &Dragonfly,
    seeded: &SeededFleet,
    event_type: EventType,
    now: Timestamp,
) -> Result<()> {
    let streams = FleetStreams::new(queue.clone());
    streams.ensure_group(&seeded.fleet).await?;
    let created = now.to_string();
    // `Entry::queued_pairs` and NOT a hand-written field list: a field the
    // producer adds arrives here with it, so the reader never drops a seeded
    // entry as one nothing admitted. The event id is minted the way
    // `afd_fleet`'s lease suites mint it. No admission row stands behind it,
    // which the lease path reads as an ordinary answer: an id the ledger
    // never minted has no row to stamp delivered (`afd_fleet` `lease::event`).
    let event_id = afd_db::test_util::mint_id();
    let entry = Entry {
        actor: BENCH_ACTOR,
        event_type: event_type.as_str(),
        workspace_id: seeded.workspace.as_str(),
        request_json: BENCH_REQUEST_JSON,
        created_at: created.as_str(),
    };
    streams
        .append(&seeded.fleet, &entry.queued_pairs(&event_id))
        .await?;
    // The index mints the token, as it does for ingress: a lease poll that
    // drains this fleet clears exactly the generation it peeked.
    ReadyIndex::new(queue.clone()).mark(&seeded.fleet).await?;
    Ok(())
}

/// Enrol one runner carrying this run's placement tag.
///
/// # Errors
///
/// Whatever enrolment refused.
pub async fn runner(database: &Db, host: &str, tag: &str, now: Timestamp) -> Result<Uuid7> {
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
    let runner = runners.register(&request, now).await?.runner_id;
    // The first beat a real host sends before it polls. A host that has never
    // reported reads degraded and may claim nothing, so without it every poll
    // would be refused for a reason this lane is not about.
    let beat = HeartbeatRequest {
        capability_report: Some(capable()),
        selftest: None,
        holds: HeldFleets(Vec::new()),
        closing: false,
    };
    let held = prove(beat.holds.clone());
    runners
        .heartbeat(&runner, &beat, held.as_ref(), now)
        .await?;
    Ok(runner)
}

/// A host that proves every confinement the enrolment asks for.
fn capable() -> CapabilityReport<'static> {
    CapabilityReport {
        build_id: None,
        landlock: true,
        seccomp: true,
        cgroup_controllers: CGROUP_CONTROLLERS.map(Cow::Borrowed).to_vec(),
        bubblewrap: true,
        egress_enforcement: true,
    }
}

#[cfg(test)]
mod tests;
