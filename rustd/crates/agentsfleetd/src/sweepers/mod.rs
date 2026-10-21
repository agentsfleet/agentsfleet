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
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::Analytics;
use afd_runner::sweep::{self, census::Census, liveness::Liveness, retention::Retention};

use crate::supervisor::Supervisor;

mod authoring;
mod billing;

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

/// Starts every background sweeper under `supervisor`.
///
/// Called after the datastores are open and before the listener binds: a
/// sweeper touching a pool that is not yet connected would fail its first pass
/// for a reason that has nothing to do with the rows it reads.
pub fn spawn(supervisor: &mut Supervisor, database: &Db, analytics: &Analytics) {
    let liveness = Liveness::new(database.clone(), Entropy::new());
    supervisor.spawn(LIVENESS, move |token| sweep::run(liveness, token));

    let retention = Retention::new(database.clone());
    supervisor.spawn(RETENTION, move |token| sweep::run(retention, token));

    // Reads only: the fleet count the dashboard draws, sampled from the table
    // that owns it rather than moved on every transition.
    let census = Census::new(database.clone());
    supervisor.spawn(FLEET_CENSUS, move |token| sweep::run(census, token));

    authoring::spawn(supervisor, database, analytics);

    // Usage logged without its settlement, and holds outlived by their lease:
    // money the request path owes and could not finish.
    billing::spawn(supervisor, database);
}
