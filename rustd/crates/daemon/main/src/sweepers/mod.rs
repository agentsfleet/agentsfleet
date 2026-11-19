//! The background sweepers, spawned here and nowhere else.
//!
//! `afd_runner::sweep` knows what each pass DOES and how the loop around it
//! behaves; this file knows which ones this daemon runs and what they are built
//! over. The split is [`crate::plane`]'s: the service crate stays unaware that
//! a daemon process exists, and the process decides what it starts.
//!
//! # Every one is supervised, and that is the whole point of spawning here
//!
//! A sweeper reads through a pool. If the process could drop that pool while a
//! pass was mid-statement, the pool would be freed under a task still using it
//! — Invariant C2, and the reason `tokio::spawn` is not called directly
//! anywhere in this daemon. [`Supervisor::spawn`] hands each one a cancellation
//! token and keeps its handle, so shutdown cancels, joins, and only then drops.
//!
use std::sync::Arc;

use af_crypto::entropy::Entropy;
use af_observability::Analytics;
use afd_db::Db;
use afd_runner::sweep::{self, census::Census, liveness::Liveness, retention::Retention};

use crate::supervisor::Supervisor;

mod authoring;
mod billing;
mod checkpoints;
mod expiry;
mod schedule;
mod tasks;

// Public for the same reason [`crate::serve::ACCEPT_LOOP`] is: the boot test
// asserts the supervisor's INVENTORY, and a test spelling these as literals
// would keep passing through a rename — leaving a sweeper unsupervised and the
// assertion still green.

/// The supervised name of the liveness sweeper.
pub const LIVENESS: &str = "sweeper:liveness";

/// The supervised name of the retention sweeper.
pub const RETENTION: &str = "sweeper:retention";

/// The supervised name of the fleet census sweeper.
pub const FLEET_CENSUS: &str = "sweeper:fleet-census";

/// The supervised name of the private authoring recovery worker.
pub const AUTHORING: &str = "sweeper:authoring";

/// The supervised name of the billing sweeper.
pub const BILLING: &str = "sweeper:billing";
/// The supervised name of the deadline sweeper: expired runs' last slices
/// and lapsed setup attempts, on one clock.
pub const EXPIRY: &str = "sweeper:expiry";
/// Native task waits, effect readback and storage cleanup; never a second work queue.
pub const TASKS: &str = "sweeper:tasks";
/// Bounded private checkpoint collection and acknowledged remote deletion.
pub const TASK_STORAGE: &str = "sweeper:task-storage";

/// Starts every background sweeper under `supervisor`.
///
/// Called after the datastores are open and before the listener binds: a
/// sweeper touching a pool that is not yet connected would fail its first pass
/// for a reason that has nothing to do with the rows it reads.
pub fn spawn(
    supervisor: &mut Supervisor,
    database: &Db,
    analytics: &Analytics,
    tasks: afd_fleet::task::Tasks,
    runtime: &crate::preflight::RuntimeSettings,
) {
    let policy = runtime.operations.maintenance;
    let startup = schedule::Startup::new(policy.loops.startup_spread_ms);
    spawn_runner_maintenance(supervisor, database, runtime, &startup);
    tasks::spawn(
        supervisor,
        tasks.clone(),
        &startup,
        policy.loops.durable_tasks,
    );
    checkpoints::spawn(
        supervisor,
        tasks.clone(),
        &startup,
        policy.loops.checkpoint_storage,
    );
    spawn_financial_maintenance(
        supervisor,
        database,
        analytics,
        tasks,
        &startup,
        &policy.loops,
    );
}

fn spawn_runner_maintenance(
    supervisor: &mut Supervisor,
    database: &Db,
    runtime: &crate::preflight::RuntimeSettings,
    startup: &schedule::Startup,
) {
    let policy = runtime.operations.maintenance;
    let liveness = Liveness::new(database.clone(), Entropy::new())
        .with_clock(runtime.clock)
        .with_settings(&policy.liveness);
    let delay = startup.delayed_first_pass(
        LIVENESS,
        std::time::Duration::from_secs(policy.liveness.interval_seconds),
    );
    supervisor.spawn(LIVENESS, move |token| {
        sweep::run_starting_in(liveness, token, delay)
    });

    let retention = Retention::new(database.clone()).with_settings(&policy.retention);
    let delay = startup.delayed_first_pass(
        RETENTION,
        std::time::Duration::from_secs(policy.retention.idle_interval_seconds),
    );
    supervisor.spawn(RETENTION, move |token| {
        sweep::run_starting_in(retention, token, delay)
    });

    // Reads only: the fleet count the dashboard draws, sampled from the table
    // that owns it rather than moved on every transition.
    let period = std::time::Duration::from_secs(policy.loops.fleet_census.interval_seconds);
    let census = Census::new(database.clone()).with_interval(period);
    let delay = startup.delayed_first_pass(FLEET_CENSUS, period);
    supervisor.spawn(FLEET_CENSUS, move |token| {
        sweep::run_starting_in(census, token, delay)
    });
}

fn spawn_financial_maintenance(
    supervisor: &mut Supervisor,
    database: &Db,
    analytics: &Analytics,
    tasks: afd_fleet::task::Tasks,
    startup: &schedule::Startup,
    settings: &crate::config::maintenance::Maintenance,
) {
    // One biller for every sweeper that moves money, so they share one
    // counted, failing-open path and report through the same analytics.
    let biller = crate::billing::biller(database.clone(), analytics.clone());
    authoring::spawn(
        supervisor,
        database,
        analytics,
        Arc::clone(&biller),
        startup,
        settings.authoring_cleanup,
    );

    billing::spawn(
        supervisor,
        database,
        tasks,
        Arc::clone(&biller),
        startup,
        settings.billing,
    );
    expiry::spawn(
        supervisor,
        database,
        analytics,
        biller,
        startup,
        settings.expired_work,
    );
}
