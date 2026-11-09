//! Seed funded fleets and capable runners with matching placement tags.
//! All work is admitted through the production ledger before measurement.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::event::EventType;
use af_wire::runner::{
    AssignedPolicy, CapabilityReport, HeartbeatRequest, HeldFleets, NetworkPolicy, RegisterRequest,
    SandboxTier,
};
use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_bus::Handle;
use afd_db::Db;
use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed};
use afd_runner::Runners;
use afd_runner::heartbeat::holds::prove;
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
/// One spelling, so a reader grepping the ledger for bench rows has one
/// thing to grep for. It is a person's steer actor (`steer:<subject>`), because
/// a live run folds only a person's messages, and the burst measures folds.
pub const BENCH_ACTOR: &str = "steer:bench";

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
    pub fleet: Uuid7,
    /// Its workspace.
    pub workspace: Uuid7,
    /// Its billing tenant.
    pub tenant: Uuid7,
}

/// An identifier the schema's `uuidv7` CHECK accepts.
///
/// The character after the second dash must be `7`. Process id plus index in
/// the final group gives four billion distinct fleets per process, which is
/// past anything the cardinality ladder climbs to.
pub(crate) fn identifier(kind: u32, index: u64) -> Result<Uuid7> {
    Ok(Uuid7::parse(&format!(
        "0195b4ba-8d3a-7{kind:03x}-8abc-{:04x}{index:08x}",
        std::process::id() & 0xffff
    ))?)
}

/// The tag that keeps this run's fleets placeable only by this run's runners.
#[must_use]
pub fn placement_tag(prefix: &RunPrefix) -> String {
    prefix.name("tag")
}

/// Seed the fleet before the steer lane measures admission.
///
/// # Errors
///
/// Whatever Postgres refused.
pub async fn empty_fleet(
    database: &Db,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: Timestamp,
) -> Result<SeededFleet> {
    let seeded = identities(index)?;
    rows(database, prefix, &seeded, tag, now).await?;
    Ok(seeded)
}

/// Seed a fleet with one unsettled admission.
///
/// # Errors
///
/// Whatever Postgres refused.
pub async fn ready_fleet(
    database: &Db,
    bus: &Handle,
    prefix: &RunPrefix,
    tag: &str,
    index: u64,
    now: Timestamp,
) -> Result<SeededFleet> {
    let seeded = identities(index)?;
    rows(database, prefix, &seeded, tag, now).await?;
    enqueue(database, bus, &seeded, EVENT_TYPE).await?;
    Ok(seeded)
}

/// The three identifiers one index yields.
pub(crate) fn identities(index: u64) -> Result<SeededFleet> {
    Ok(SeededFleet {
        fleet: identifier(KIND_FLEET, index)?,
        workspace: identifier(KIND_WORKSPACE, index)?,
        tenant: identifier(KIND_TENANT, index)?,
    })
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

/// One event of `event_type` admitted onto the fleet, through the ledger's own
/// write path.
///
/// `Admissions::admit` is what ingress runs: it commits the `fleet.admissions`
/// row, then publishes a best-effort wake on the Bus. Seeding through it
/// leaves exactly the state ingress leaves, so the lease path's delivery stamp
/// meets a real row, as in production.
///
/// A wake the Bus drops is not an error: the committed row is what the lease
/// path reads, and the run's ledger backlog shows it until a lease delivers it.
pub(crate) async fn enqueue(
    database: &Db,
    bus: &Handle,
    seeded: &SeededFleet,
    event_type: EventType,
) -> Result<String> {
    let admitted = Admissions::new(database.clone(), bus.clone(), Entropy::new())
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Unrepeatable,
            fleet: &seeded.fleet,
            workspace: &seeded.workspace,
            actor: BENCH_ACTOR,
            event_type,
            request_json: BENCH_REQUEST_JSON,
            reply: Reply::None,
        })
        .await?;
    Ok(admitted.stored.id.as_str().to_owned())
}

/// Enrol one runner carrying this run's placement tag.
///
/// # Errors
///
/// Whatever enrolment refused.
pub async fn runner(database: &Db, host: &str, tag: &str, now: Timestamp) -> Result<Uuid7> {
    runner_with(database, host, tag, RUNNER_WORKERS, now).await
}

/// Enrol one runner declaring `workers` concurrent workers.
///
/// The saturation shape's hosts hold several unreported leases at once, so
/// they keep polling under contention rather than going busy after one.
///
/// # Errors
///
/// Whatever enrolment refused.
pub async fn runner_with(
    database: &Db,
    host: &str,
    tag: &str,
    workers: u32,
    now: Timestamp,
) -> Result<Uuid7> {
    Ok(runner_identity(database, host, tag, workers, now).await?.id)
}

/// Enrolled identity retained by an in-process gateway session.
#[derive(Debug, Clone)]
pub(in crate::lane::lease) struct RunnerIdentity {
    pub(in crate::lane::lease) id: Uuid7,
    pub(in crate::lane::lease) digest: afd_auth::directory::Digest,
}

/// Enrol once while retaining the credential digest for guarded native ownership.
///
/// # Errors
/// Preserves enrollment and initial capability failures.
pub(in crate::lane::lease) async fn runner_identity(
    database: &Db,
    host: &str,
    tag: &str,
    workers: u32,
    now: Timestamp,
) -> Result<RunnerIdentity> {
    let runners = Runners::new(database.clone(), Entropy::new());
    let request = RegisterRequest {
        host_id: host.into(),
        assigned_policy: AssignedPolicy {
            durable_tasks_enabled: af_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED,
            decision_model_id: None,
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::AllowListEgress,
            registry_allowlist: Vec::new(),
            worker_count: workers,
            extra_binds: Vec::new(),
        },
        // The placement tag, and nothing else: the candidate query filters
        // `required_tags <@ labels`, so a runner carrying extra labels would
        // still match and a runner missing this one never will.
        labels: vec![tag.into()],
    };
    let enrolled = runners.register(&request, now).await?;
    let digest = enrolled.token.digest().clone();
    let runner = enrolled.runner_id;
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
    Ok(RunnerIdentity { id: runner, digest })
}

/// A host that proves every confinement the enrolment asks for.
fn capable() -> CapabilityReport<'static> {
    CapabilityReport {
        durable_tasks_v1: false,
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
