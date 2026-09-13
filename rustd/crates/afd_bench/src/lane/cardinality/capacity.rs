//! What the datastores hold at population, one figure per class of state.
//!
//! The ladder above says what a fleet COSTS; this says what the deployment is
//! FULL of, and keeps the classes apart because they are different incidents:
//! a stream backlog and a readiness backlog are drained by different things,
//! retained history and pending work are paid for differently, and a shard
//! with no replica is a risk no byte count shows. Recorded as separate
//! measurements so a grader can budget each one, and never summed.

use afd_admission::Admissions;
use afd_core::clock;
use afd_crypto::entropy::Entropy;
use afd_datastore::Capacity;

use crate::datastores::Datastores;
use crate::error::Result;
use crate::report::{Report, count};

/// How many streams one sample will describe.
///
/// The walk lists every stream and describes this many; the report says both,
/// so a rig at a million fleets reads as a capped sample rather than as ten
/// thousand fleets.
const WALK_CAP: usize = 10_000;

/// Measurement keys, one per class. The `datastore_` and `ledger_` prefixes
/// say which store a figure came from.
const STREAMS: &str = "datastore_streams";
const STREAMS_WALKED: &str = "datastore_streams_walked";
const RETAINED_ENTRIES: &str = "datastore_retained_entries";
const PENDING_ENTRIES: &str = "datastore_pending_entries";
const READY_PARTITIONS: &str = "datastore_ready_partitions";
const READY_MARKS: &str = "datastore_ready_marks";
const PRIMARIES: &str = "datastore_primaries";
const REPLICAS: &str = "datastore_replicas";
const LEDGER_BACKLOG_ROWS: &str = "ledger_backlog_rows";
const LEDGER_BACKLOG_OLDEST_AGE_MS: &str = "ledger_backlog_oldest_age_ms";

/// Every key [`record`] writes, for a grader or a suite that checks the
/// report accounts for each class separately.
pub const MEASUREMENTS: &[&str] = &[
    STREAMS,
    STREAMS_WALKED,
    RETAINED_ENTRIES,
    PENDING_ENTRIES,
    READY_PARTITIONS,
    READY_MARKS,
    PRIMARIES,
    REPLICAS,
    LEDGER_BACKLOG_ROWS,
    LEDGER_BACKLOG_OLDEST_AGE_MS,
];

/// Samples both stores and records every class on `report`.
///
/// # Errors
///
/// A datastore that would not answer, through the lane's own lifts.
pub(super) async fn record(stores: &Datastores, report: &mut Report) -> Result<()> {
    let sample = Capacity::sample(&stores.queue, WALK_CAP).await?;
    report.measurement(STREAMS, count(sample.streams));
    report.measurement(STREAMS_WALKED, count(sample.streams_walked));
    report.measurement(RETAINED_ENTRIES, count(sample.retained_entries));
    report.measurement(PENDING_ENTRIES, count(sample.pending_entries));
    report.measurement(READY_PARTITIONS, count(sample.ready_partitions));
    report.measurement(READY_MARKS, count(sample.ready_marks));
    report.measurement(PRIMARIES, count(sample.primaries));
    report.measurement(REPLICAS, count(sample.replicas));

    // The same ledger shape the daemon holds — see `lane::steer` — read for
    // the one figure the datastore cannot give: rows the queue never took.
    let ledger = Admissions::new(
        stores.database.clone(),
        stores.queue.clone(),
        Entropy::new(),
    );
    let backlog = ledger.backlog(clock::now()).await?;
    report.measurement(LEDGER_BACKLOG_ROWS, count(backlog.rows));
    report.measurement(
        LEDGER_BACKLOG_OLDEST_AGE_MS,
        count(
            backlog
                .oldest_age
                .map_or(0, |age| u64::try_from(age.as_millis()).unwrap_or(u64::MAX)),
        ),
    );
    Ok(())
}
