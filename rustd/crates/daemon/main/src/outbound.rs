//! The connector answer-delivery worker, spawned here and nowhere else.
//!
//! `afd_outbound` knows what a delivery IS and how a failed one ends; this file
//! knows that this daemon runs one, and what it is built over. The service
//! crate stays unaware that a daemon process exists, and the process decides
//! what it starts.

use af_crypto::entropy::Entropy;
use afd_connector::Grants;
use afd_db::Db;
use afd_outbound::{Posters, SlackPoster, Worker};

use crate::supervisor::Supervisor;

/// The supervised name of the answer-delivery worker.
pub const OUTBOUND_WORKER: &str = "connector:outbound";

/// Starts the answer-delivery worker under `supervisor`.
///
/// It claims owed answers from `thread.deliveries` at start, when `bus` pings
/// it and when a delivery finishes; it runs no timed pass.
///
/// # Errors
/// The ping subscription would not open. The boot is refused, as an unreachable
/// NATS seed refuses it: a worker that never hears a ping would leave an owed
/// answer waiting for a restart.
pub(crate) async fn spawn(
    supervisor: &mut Supervisor,
    database: &Db,
    bus: afd_bus::Handle,
    grants: Grants,
    vendor_client: reqwest::Client,
    slack_api_base: String,
    settings: &crate::preflight::runtime::OperationalSettings,
) -> afd_bus::Result<()> {
    let wake = bus.subscribe(&afd_bus::Subject::DeliveryWake).await?;
    let worker = Worker::new(
        database.clone(),
        wake,
        Posters {
            slack: SlackPoster::new(grants, vendor_client, slack_api_base)
                .with_deadlines(settings.tuning.slack_deadlines()),
        },
        Entropy::new(),
    )
    .with_retries(settings.outbound_retries);
    supervisor.spawn(OUTBOUND_WORKER, move |token| worker.run(token));
    Ok(())
}
