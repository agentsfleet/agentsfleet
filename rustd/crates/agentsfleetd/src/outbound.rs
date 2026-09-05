//! The connector answer-delivery worker, spawned here and nowhere else.
//!
//! `afd_outbound` knows what a delivery IS and how a failed one is retried;
//! this file knows that this daemon runs one, and what it is built over. The
//! split is [`crate::sweepers`]': the service crate stays unaware that a daemon
//! process exists, and the process decides what it starts.
//!
//! # Why this one opens a connection the rest of the process does not share
//!
//! Every other Redis caller in this binary goes through the one multiplexed
//! [`Redis`], because sharing a socket is what makes a pool unnecessary. This
//! worker blocks on `XREADGROUP … BLOCK`, and a blocking command on a shared
//! connection is not a slow command — it is a stopped process, since Redis
//! executes a connection's commands in order. So it dials its own, exactly as
//! the pub/sub hub does, and [`afd_redis::Dedicated`] is not `Clone` so nobody
//! can join it there.
//!
//! # A worker that cannot dial does not stop the boot
//!
//! Redis is already proven reachable by the time this runs — the shared handle
//! pinged it, and `/readyz` reports on it. A second dial that fails here is a
//! blip on a connection nothing else needs, and refusing to boot over it would
//! take down the whole API to protect the return leg of one connector. It is
//! logged and skipped: answers queue durably on the stream and the next process
//! that starts delivers them.

use afd_connector::Grants;
use afd_db::Db;
use afd_outbound::{LONGEST_PARK, Posters, SlackPoster, Worker};
use afd_redis::{Dedicated, OutboundQueue, OutboundReader, Redis, RedisConfig, outbound_consumer};

use crate::supervisor::Supervisor;

// Public for the same reason [`crate::sweepers`]'s names are: the boot test
// asserts the supervisor's INVENTORY, and a test spelling this as a literal
// would keep passing through a rename — leaving the worker unsupervised and the
// assertion still green.

/// The supervised name of the connector answer-delivery worker.
pub const OUTBOUND_WORKER: &str = "connector:outbound";

/// Starts the outbound worker under `supervisor`, if it can open its socket.
///
/// Called after the datastores are open and before the listener binds, for the
/// reason the sweepers are: a worker reading through a pool that is not yet
/// connected would fail its first pass for a reason unrelated to the rows.
pub async fn spawn(
    supervisor: &mut Supervisor,
    config: &RedisConfig,
    database: &Db,
    queue: &Redis,
    grants: Grants,
    vendor_client: reqwest::Client,
) {
    let connection = match Dedicated::connect(config, LONGEST_PARK).await {
        Ok(connection) => connection,
        Err(failure) => {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let error_code = failure.code().as_str();
            let reason = failure.to_string();
            tracing::warn!(error_code, reason, event = "outbound_worker_connect_failed");
            return;
        }
    };

    let worker = Worker::new(
        OutboundReader::new(connection, outbound_consumer()),
        OutboundQueue::new(queue.clone()),
        Posters {
            slack: SlackPoster::new(
                database.clone(),
                grants,
                vendor_client,
                afd_outbound::slack::SLACK_API_BASE.to_owned(),
            ),
        },
    );
    supervisor.spawn(OUTBOUND_WORKER, move |token| worker.run(token));
}
