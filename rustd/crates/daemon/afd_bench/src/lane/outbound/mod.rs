//! What one delivery worker sustains, and what a slow destination costs the
//! jobs queued behind it.
//!
//! # The shape of a run
//!
//! Owe N answers across D destination fleets, then start the real `Worker` and
//! let it claim and drain them from `fleet.obligations` (M244 §4). Every job is
//! paid by one workspace, so they share one (provider, workspace) lane and
//! deliver one at a time: each job's latency includes the time every earlier
//! job spent, which is exactly the head-of-line cost this lane measures.
//!
//! # The clock starts when the worker does
//!
//! Every job's latency is measured from the LATER of its owe and the worker's
//! start. The seeding and owe loop happen before that instant, and charging
//! them to job zero would give a head-of-line number that scaled with how many
//! jobs were owed behind it.
//!
//! # The window ends at the last settlement, not the next tick
//!
//! The drain waits on the poster's own signal and takes the window's length
//! from the last attempt it made, so a four-job run is not reported over the
//! fifty milliseconds a polling loop happened to sleep.
//!
//! # A drain that did not finish says so
//!
//! The worker turns a dead datastore into pause-and-retry and never returns
//! an error, so a window that reached its deadline with jobs unsettled is the
//! only signal there is. It is recorded as an abort, and the rate is over the
//! jobs that did settle.

pub mod poster;
mod record;

use af_core::id::Uuid7;
use core::time::Duration;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use af_crypto::entropy::Entropy;
use afd_connector::Provider;
use afd_db::Db;
use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed};
use afd_outbound::obligation::{self, Delivery};
use afd_outbound::{Posters, Worker};
use tokio_util::sync::CancellationToken;

use self::poster::{Behaviour, Scripted};
use self::record::{Drained, record, window_end};
use crate::datastores::{Datastores, postgres_transactions};
use crate::error::{ErrorKind, Result, seed_refused};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::knobs::{RETRYABLE_FRACTION_VARIABLE, SLOW_FRACTION_VARIABLE};
use crate::lane::sweep;
use crate::profile::{Parameter, Profile};
use crate::report::{Fixture, Lane, Provenance, Report};

/// A healthy vendor's answer time. One millisecond is well under the ladder's
/// first rung, so the fast population cannot be confused with a retry.
const FAST_ANSWER: Duration = Duration::from_millis(1);

/// A slow vendor's answer time: long enough that the jobs behind it visibly
/// wait, short enough that a window of seconds still drains.
const SLOW_ANSWER: Duration = Duration::from_millis(250);

/// How many destinations the jobs are spread over.
const DESTINATIONS: u64 = 16;

/// The reply address every bench job carries. The scripted poster keys its
/// lanes on the fleet and never reads this, so one value serves them all.
const REPLY_ADDRESS: &str = r#"{"channel_id":"C0BENCH","thread_ts":"1700000000.000100"}"#;

/// The provider every queued job names, so the worker routes it to the one
/// poster this build ships.
const PROVIDER: Provider = Provider::Slack;

/// A generated answer; never a tenant's text (RULE PRI).
const ANSWER: &str = "bench answer";

/// What the lost-task refusal calls the worker.
const WORKER_ROLE: &str = "delivery worker";

/// How long to wait for the worker to stop after cancellation.
const STOP_GRACE: Duration = Duration::from_secs(6);

/// The longest the drain waits for a settlement before re-checking the
/// deadline; a bound on how late a deadline is noticed, not a sampling rate.
const SETTLEMENT_WAIT: Duration = Duration::from_millis(250);

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// Jobs to queue.
    pub jobs: u64,
    /// Fraction of destinations scripted slow, in `0..=1`.
    pub slow_fraction: f64,
    /// Fraction of destinations scripted retryable, in `0..=1`.
    pub retryable_fraction: f64,
    /// The longest the drain may run.
    pub window: Duration,
}

impl Parameters {
    /// Refuse anything outside the profile's bounds, before a connection opens.
    ///
    /// # Errors
    ///
    /// A cap, a floor, or a window under the warmup floor, each named.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Jobs, self.jobs)?;
        profile.check_window(self.window)
    }
}

/// Run the lane and return the report it measured.
///
/// The run removes the entries it queued before returning, by the ids it was
/// handed; the caller's prefix sweep is the fallback for a run that failed
/// before it could.
///
/// # Errors
///
/// A cap refusal, a datastore that would not answer, or a lost worker.
pub async fn run(
    profile: Profile,
    provenance: Provenance,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;

    let behaviours = script(prefix, parameters)?;
    let poster = Scripted::new(behaviours.clone(), FAST_ANSWER, SLOW_ANSWER);
    let destinations: Vec<&String> = behaviours.keys().collect();

    let payer = parents(&stores.database, prefix, &destinations).await?;
    let (mut ledger, queued_at) =
        owe_all(&stores.database, &payer, &destinations, parameters.jobs).await?;

    let drained = drain(stores, poster.clone(), parameters).await?;
    let ids = queued_at
        .keys()
        .map(|id| Uuid7::parse(id))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ledger.swept(sweep::outbound_obligations(&stores.database, &ids).await?);

    let mut report = Report::new(Lane::Outbound, profile, provenance);
    report.created = true;
    report.parameter(Parameter::Jobs.name(), parameters.jobs);
    report.parameter(
        SLOW_FRACTION_VARIABLE,
        fraction_of(DESTINATIONS, parameters.slow_fraction),
    );
    report.parameter(
        RETRYABLE_FRACTION_VARIABLE,
        fraction_of(DESTINATIONS, parameters.retryable_fraction),
    );
    record(
        &mut report,
        &drained,
        &poster,
        &queued_at,
        &behaviours,
        parameters.jobs,
    )?;
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// The paying tenant and workspace, and one fleet per destination; answers
/// the payer, whose one lane every job shares.
async fn parents(database: &Db, prefix: &RunPrefix, destinations: &[&String]) -> Result<Uuid7> {
    let now = af_core::clock::now();
    let tenant = crate::lane::lease::seed::identifier(KIND_TENANT, 0)?;
    let payer = crate::lane::lease::seed::identifier(KIND_PAYER, 0)?;
    let mut connection = database.acquire().await?;
    TenantSeed::new(&tenant)
        .name(&prefix.name("tenant"))
        .at(now)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    WorkspaceSeed::new(&payer, &tenant)
        .name(&prefix.name("workspace"))
        .at(now)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    for (index, destination) in destinations.iter().enumerate() {
        FleetSeed::new(
            &Uuid7::parse(destination)?,
            &payer,
            &tenant,
            afd_fleet::lease::sql::FLEET_STATUS_ACTIVE,
        )
        .name(&prefix.name(&format!("fleet-{index}")))
        .at(now)
        .insert(&mut *connection)
        .await
        .map_err(seed_refused)?;
    }
    Ok(payer)
}

/// Owe `jobs` answers dealt round-robin over `destinations`, noting when each
/// row was owed and counting each in the fixture ledger.
async fn owe_all(
    database: &Db,
    payer: &Uuid7,
    destinations: &[&String],
    jobs: u64,
) -> Result<(FixtureLedger, HashMap<String, Instant>)> {
    let mut ledger = FixtureLedger::new();
    let mut queued_at = HashMap::new();
    let entropy = Entropy::new();
    let mut connection = database.acquire().await?;
    for (index, destination) in (0..jobs).zip(destinations.iter().cycle()) {
        let fleet = Uuid7::parse(destination)?;
        let event = crate::lane::lease::seed::identifier(KIND_ANSWERED, index)?;
        let row = entropy.uuid7(af_core::clock::now())?;
        let token = entropy.uuid7(af_core::clock::now())?;
        obligation::owe(
            &mut connection,
            &row,
            &token,
            Delivery {
                fleet_id: &fleet,
                workspace_id: payer,
                provider: PROVIDER,
                destination: REPLY_ADDRESS,
                event_id: &event,
                answer: ANSWER,
            },
        )
        .await?;
        queued_at.insert(row.as_str().to_owned(), Instant::now());
        ledger.created(1);
    }
    Ok((ledger, queued_at))
}

/// Assign a behaviour to each destination by the requested fractions.
///
/// A `BTreeMap`, so the order jobs are dealt to destinations — and therefore
/// which position in each cycle the slow one occupies — is a function of the
/// parameters and not of a hash seed that changes per process.
/// The identifier kinds this lane mints: a destination fleet, the paying
/// workspace, and an answered event. Distinct from the lease seeds' kinds.
const KIND_DESTINATION: u32 = 0x0d1;
const KIND_PAYER: u32 = 0x0d2;
const KIND_ANSWERED: u32 = 0x0d3;
const KIND_TENANT: u32 = 0x0d4;

pub(crate) fn script(
    _prefix: &RunPrefix,
    parameters: Parameters,
) -> Result<BTreeMap<String, Behaviour>> {
    let slow = fraction_of(DESTINATIONS, parameters.slow_fraction);
    let retryable = fraction_of(DESTINATIONS, parameters.retryable_fraction);
    (0..DESTINATIONS)
        .map(|index| {
            let behaviour = if index < slow {
                Behaviour::Slow
            } else if index < slow + retryable {
                Behaviour::Retryable
            } else {
                Behaviour::Fast
            };
            Ok((
                crate::lane::lease::seed::identifier(KIND_DESTINATION, index)?.to_string(),
                behaviour,
            ))
        })
        .collect()
}

/// How many of `total` a fraction selects, rounded down and clamped.
pub(crate) fn fraction_of(total: u64, fraction: f64) -> u64 {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sixteen destinations times a clamped fraction is a small non-negative integer"
    )]
    {
        (total as f64 * fraction.clamp(0.0, 1.0)).floor() as u64
    }
}

/// Start the real worker, wait for it to settle every job or the deadline,
/// stop it.
async fn drain(stores: &Datastores, poster: Scripted, parameters: Parameters) -> Result<Drained> {
    let transactions_before = postgres_transactions(&stores.database).await?;
    let token = CancellationToken::new();
    let started = Instant::now();
    let worker = tokio::spawn(
        Worker::new(
            stores.database.clone(),
            stores.bus.clone(),
            Posters {
                slack: poster.clone(),
            },
            Entropy::new(),
        )
        .run(token.clone()),
    );

    let deadline = started + parameters.window;
    let settled = settle_then_stop(&poster, &token, worker, deadline, parameters.jobs).await?;

    // The window ends at the last answer this run's jobs received, not at the
    // last attempt's start; the deadline stands in only when nothing was asked.
    let ended = window_end(&poster.seen(), deadline);

    Ok(Drained {
        started,
        ended,
        settled,
        transactions: postgres_transactions(&stores.database)
            .await?
            .saturating_sub(transactions_before),
    })
}

/// Wait until `poster` has settled every job or `deadline` passes, then stop
/// the worker within [`STOP_GRACE`], answering how many had settled.
async fn settle_then_stop(
    poster: &Scripted,
    token: &CancellationToken,
    worker: tokio::task::JoinHandle<()>,
    deadline: Instant,
    jobs: u64,
) -> Result<u64> {
    while Instant::now() < deadline && poster.settled() < jobs {
        poster.settlement(SETTLEMENT_WAIT).await;
    }
    let settled = poster.settled();
    token.cancel();
    tokio::time::timeout(STOP_GRACE, worker)
        .await
        .map_err(|_elapsed| ErrorKind::TaskLost { role: WORKER_ROLE })?
        .map_err(|_joined| ErrorKind::TaskLost { role: WORKER_ROLE })?;
    Ok(settled)
}

#[cfg(test)]
mod tests;
