//! One host's renewals in one statement.
//!
//! The runner sends one renewal frame per host each tick, naming every live
//! lease. Before, each lease ran the per-lease path: a load, a funds check, a
//! budget read, a price read, the host lock, a cancel read, the meter and a
//! line log, about ten statements. Here the frame costs a pre-read and one
//! statement ([`crate::lease::sql::renew::RENEW_BATCH`]), plus a funds check per
//! payer every [`FUNDS_CHECK_TICKS`] ticks and a read for whatever the caches
//! miss.
//!
//! A lease that runs a durable task keeps the per-lease path, which checks task
//! authority at commit. So does a lease whose prices or ceilings moved under the
//! statement: the per-lease path reads them fresh, and the case is rare enough
//! that its statements do not count.

mod ceilings;
mod columns;
mod frame;
mod pricing;
mod rows;
#[cfg(test)]
mod tests;
mod verdict;

use std::collections::HashMap;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use futures_util::StreamExt as _;
use jiff::Timestamp;

use self::verdict::Answer;
use crate::error::Result;
use crate::lease::pull::LeaseService;

/// How many renewal ticks pass between funds checks of one payer.
///
/// A two-tick chat then pays two thirds of a check instead of four statements,
/// and a run overruns its payer's balance by at most this many slices.
pub const FUNDS_CHECK_TICKS: u64 = 3;

/// Whether a session's renewal frame `tick`, counted from 0, checks funds.
///
/// The first frame checks, then every [`FUNDS_CHECK_TICKS`]th, so a reconnect
/// checks at once and a runner cannot dodge the check by reconnecting.
#[must_use]
pub const fn funds_due(tick: u64) -> bool {
    tick.is_multiple_of(FUNDS_CHECK_TICKS)
}

/// How many per-lease renewals one frame runs together.
///
/// The leases the statement does not settle (a durable task's, one whose prices
/// moved, every lease when the statement itself failed) renew on their own
/// path, about ten statements each, all inside the one answer deadline the frame
/// has. Walking them one by one could miss it and fail every lease at once, so
/// up to this many run together; the answers still come back in the frame's
/// order.
const FALLBACK_RENEWALS_IN_FLIGHT: usize = 8;

/// Logged when the frame's pre-read or statement failed.
const EVENT_FRAME_FAILED: &str = "renew_batch_failed";

/// One host's renewal frame, answered lease by lease in its order.
#[derive(Debug)]
pub struct Renewals<'a> {
    /// The host that sent the frame.
    pub runner_id: &'a Uuid7,
    /// Every lease the frame names, with its request lines.
    pub leases: Vec<(Uuid7, RenewRequest)>,
    /// Whether this tick checks each payer's funds.
    pub funds_due: bool,
    /// The instant identifiers this pass mints are drawn at.
    pub now: Timestamp,
    /// What the session kept from each plain lease's offer: when every lease
    /// has its facts and the replica knows each fleet's versions, the frame runs
    /// without its pre-read.
    pub kept: HashMap<Uuid7, crate::lease::Reported>,
}

impl LeaseService {
    /// Renews every lease one host's frame names, and answers each in order.
    ///
    /// # Errors
    /// Each answer is a lease's own: the refusals [`LeaseService::renew`]
    /// answers, or a fault reaching the datastore, which the runner retries at
    /// its next tick.
    pub async fn renew_batch(&self, renewals: Renewals<'_>) -> Vec<Result<Timestamp>> {
        let Renewals {
            runner_id,
            leases,
            funds_due,
            now,
            kept,
        } = renewals;
        // A frame the batch could not read or run leaves every lease to the
        // per-lease path: in an outage each one faults on its own and answers
        // retry, and the runner keeps them until their deadlines.
        let mut answers = match self
            .renew_frame(runner_id, &leases, &kept, funds_due, now)
            .await
        {
            Ok(answers) => answers,
            Err(fault) => {
                let runner = runner_id.as_str();
                let code = fault.code().as_str();
                tracing::warn!(
                    runner_id = runner,
                    error_code = code,
                    event = EVENT_FRAME_FAILED,
                    "the renewal statement did not run; each lease renews on its own path"
                );
                HashMap::new()
            }
        };
        in_order(
            leases,
            FALLBACK_RENEWALS_IN_FLIGHT,
            |(lease_id, request)| {
                let done = answers.remove(&lease_id);
                async move {
                    match done {
                        Some(Answer::Done(answer)) => answer,
                        Some(Answer::PerLease) | None => {
                            Box::pin(self.renew(runner_id, &lease_id, request, now))
                                .await
                                .map(|(lease_expires_at, _charged)| lease_expires_at)
                        }
                    }
                }
            },
        )
        .await
    }
}

/// Runs `work` over `items`, at most `limit` at a time, and answers in the
/// items' order.
async fn in_order<I, T, F, Fut>(items: Vec<I>, limit: usize, work: F) -> Vec<T>
where
    F: FnMut(I) -> Fut,
    Fut: Future<Output = T>,
{
    futures_util::stream::iter(items)
        .map(work)
        .buffered(limit)
        .collect()
        .await
}
