//! The lease lane's saturation shape: `make bench-lease BENCH_SHAPE=saturation`.
//!
//! # What it asks
//!
//! What the production poll sustains when requesters outnumber the pool's
//! connections, and whether it stays correct while it does. Every host polls
//! `Plane::claim` — setup chats and fleet leases competing as they do in
//! production — and renews what it holds, while wall viewers read the fleet
//! live set on the same pool. Requesters default to twice the pool size, so
//! the default shape's `fit` guard is the one thing this shape skips on
//! purpose.
//!
//! # What it reports
//!
//! Lease rate and p95, the pool's wait and hold time apart from both, and the
//! live set's read tail; then the invariants in [`invariants`], any one of
//! which refuses the run.

mod invariants;
mod live;
mod probe;
mod production;
mod record;
mod requester;
mod slow;
mod stage;

use std::sync::Arc;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::Fleets;

use self::probe::Pool as _;
use self::production::Production;
use self::record::Window;
use self::requester::Shared;
use super::Parameters;
use super::drain::platform;
use crate::abort::Abort;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::joined;
use crate::profile::Profile;
use crate::report::{Fixture, Lane, Provenance, Report};

pub use self::record::summary;

/// The task role a lost requester is reported under.
const REQUESTER_ROLE: &str = "saturation requester";

/// The task role a lost wall viewer is reported under.
const VIEWER_ROLE: &str = "saturation viewer";

/// The task role a lost slow writer is reported under.
const WRITER_ROLE: &str = "saturation slow writer";

/// What one run needs to hand every stage of it.
struct Run<'a> {
    profile: Profile,
    parameters: Parameters,
    stores: &'a Datastores,
    prefix: &'a RunPrefix,
}

/// Run the saturation shape and return the report it measured.
///
/// # Errors
///
/// A cap refusal, a datastore or path that would not answer, a lost task, or
/// a broken invariant. Never a slow result.
pub async fn run(
    profile: Profile,
    provenance: Provenance,
    parameters: Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    let run = Run {
        profile,
        parameters,
        stores,
        prefix,
    };
    let mut report = Report::new(Lane::LeaseSaturation, profile, provenance);
    record::parameters(&mut report, parameters, stores.pool_size);
    let kek = Arc::new(platform::minted_kek()?);
    let host = Arc::new(Production::new(stores, &kek));
    let mut ledger = FixtureLedger::new();
    let staged = stage::stage(stores, &host.drafts, &kek, prefix, parameters, &mut ledger).await;
    let measured = match staged {
        Ok(staged) => measure(&run, &host, &staged, &mut ledger, &mut report).await,
        Err(refused) => Err(refused),
    };
    let released = platform::release(stores, stage::INDEX_BASE).await;
    report.fixture = Fixture::of(prefix, ledger);
    measured.and(released)?;
    Ok(report)
}

/// The window over a staged population, its invariants judged last.
async fn measure(
    run: &Run<'_>,
    host: &Arc<Production>,
    staged: &stage::Staged,
    ledger: &mut FixtureLedger,
    report: &mut Report,
) -> Result<()> {
    let abort = Arc::new(Abort::new(run.profile.caps().abort_error_rate));
    let started = Instant::now();
    let shared = Arc::new(Shared {
        deadline: started + run.parameters.window,
        leased: 0.into(),
        population: staged.population(),
        abort: Arc::clone(&abort),
    });
    let requesters = spawn_requesters(host, &staged.runners, &shared);
    let viewers = spawn_viewers(run.stores, staged, &shared)?;
    let writer = spawn_writer(run.stores, staged, &shared).await?;
    let mut sampler = run.stores.sampler().await?;
    let pool = probe::watch(&run.stores.database, &mut sampler, &shared).await?;
    let mut window = Window::new(pool)?;
    for task in requesters {
        window.tally.absorb(&joined(task.await, REQUESTER_ROLE)??)?;
    }
    for task in viewers {
        window.reads.absorb(&joined(task.await, VIEWER_ROLE)??)?;
    }
    window.slow_holds = joined(writer.await, WRITER_ROLE)??;
    window.length = started.elapsed();
    let recovered = run.stores.database.serves(run.stores.pool_size).await;
    let tenants: Vec<String> = staged.tenants.iter().map(|t| t.tenant.clone()).collect();
    ledger.created(invariants::issued(run.stores, &tenants).await?);
    let census = invariants::count(
        run.stores,
        run.prefix,
        &tenants,
        window.tally.chat,
        recovered,
    )
    .await?;
    window.record(report, &census);
    report.abort = abort.recorded();
    census.judge()
}

/// One requester per host, each polling until the window closes.
fn spawn_requesters(
    host: &Arc<Production>,
    runners: &[Uuid7],
    shared: &Arc<Shared>,
) -> Vec<tokio::task::JoinHandle<Result<requester::Tally>>> {
    runners
        .iter()
        .map(|runner| {
            let (host, runner, shared) = (Arc::clone(host), runner.clone(), Arc::clone(shared));
            tokio::spawn(async move { requester::drive(&*host, &runner, &shared).await })
        })
        .collect()
}

/// The slow writer over the first chat tenant's drafts, on a connection of
/// its own.
async fn spawn_writer(
    stores: &Datastores,
    staged: &stage::Staged,
    shared: &Arc<Shared>,
) -> Result<tokio::task::JoinHandle<Result<u64>>> {
    let mut locker = stores.sampler().await?;
    let tenant = staged
        .tenants
        .first()
        .map(|seeded| seeded.tenant.clone())
        .unwrap_or_default();
    let shared = Arc::clone(shared);
    Ok(tokio::spawn(async move {
        slow::write(&mut locker, &tenant, &shared).await
    }))
}

/// [`live::READERS`] wall viewers over every chat tenant's workspace.
fn spawn_viewers(
    stores: &Datastores,
    staged: &stage::Staged,
    shared: &Arc<Shared>,
) -> Result<Vec<tokio::task::JoinHandle<Result<live::Reads>>>> {
    let workspaces = staged
        .tenants
        .iter()
        .map(|tenant| Uuid7::parse(&tenant.workspace))
        .collect::<core::result::Result<Arc<[Uuid7]>, _>>()?;
    let fleets = Fleets::new(stores.database.clone(), stores.queue.clone());
    Ok((0..live::READERS)
        .map(|_| {
            let (fleets, workspaces, shared) =
                (fleets.clone(), Arc::clone(&workspaces), Arc::clone(shared));
            tokio::spawn(async move { live::view(&fleets, &workspaces, &shared).await })
        })
        .collect())
}
