//! The claim loop: Postgres holds every owed answer, and the worker claims lane heads.
//!
//! It claims when it starts, when it is pinged, when a delivery finishes, and when a
//! claim another replica held lapses. It runs no timed pass (Indy, Nov 21, 2026): the
//! one sleep is to that lapse, read from Postgres.
//!
//! ```text
//!   worker starts ────────────────────────────────┐
//!   report commits ──► af.delivery.wake (lossy) ──┼─► claim lane heads ─► lanes
//!   a delivery finishes ──────────────────────────┘    (FOR UPDATE SKIP LOCKED)
//! ```
//!
//! # A lost ping waits for the next event
//!
//! The ping is at-most-once. A ping the bus failed to send is owed, and the bus
//! pays it as a gap on the subscription, which the worker answers with a claim.
//! A full batch claims again as soon as the lanes have room, and every finished
//! job rings the loop, so a backlog drains without a timer. The daemon opens the
//! subscription before the worker starts, and a subscription it cannot open refuses
//! the boot. A reconnect keeps the subscription and gaps it; it closes only when the
//! bus shuts down, and the worker then claims on finished deliveries and lapses alone.
//!
//! # A dead replica's claim is retaken when it lapses
//!
//! Its claim stops being renewed and lapses after `CLAIM_TTL`. Nothing announces
//! that, so after a claim leaves room the worker reads the earliest live claim's
//! expiry and sleeps to it once.
//!
//! # The claim is the ordering, and the lanes are the concurrency
//!
//! Postgres hands out at most one held row per (provider, workspace) lane, so
//! two answers to one thread cannot be in flight together on any replica. The
//! lanes bound how many vendor calls this replica runs at once, and a claim
//! takes no more rows than they can start: a claimed row left waiting for a
//! slot renews nothing, and its claim could lapse to another replica.

use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use af_crypto::entropy::Entropy;
use afd_bus::Subscription;
use afd_db::Db;
use tokio_util::sync::CancellationToken;

use crate::delivery;
use crate::lanes::Lanes;
use crate::poster::{Deliver, Posters};

/// Logged around each claim. A claim runs on every ping and finished
/// delivery, so both halves sit at `debug`.
const EVENT_CLAIM_STARTED: &str = "outbound_claim_started";
const EVENT_CLAIM_COMPLETED: &str = "outbound_claim_completed";
/// Logged when a claim could not be made; the next ping or finished delivery
/// claims again.
const EVENT_CLAIM_FAILED: &str = "outbound_claim_failed";
/// Logged when pings stop reaching this worker; only finished deliveries, lapses and
/// the next start claim for it then.
const EVENT_WAKE_CLOSED: &str = "outbound_wake_closed";
/// The lapse read failed: the loop keeps the lapse it had, and the next claim reads again.
const EVENT_LAPSE_UNREAD: &str = "outbound_lapse_unread";

/// What triggered a claim, for the claim counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    /// A committed report's ping.
    Ping,
    /// The worker's start, a finished delivery, or a full batch draining a
    /// backlog.
    Scan,
}

/// The outbound worker: claims owed answers and hands them to the lanes.
#[derive(Debug)]
pub struct Worker<S> {
    database: Db,
    wake: Subscription,
    posters: Posters<S>,
    entropy: Entropy,
    renew: Duration,
    retries: crate::retry::RetryPolicy,
}

impl<S: Deliver + 'static> Worker<S> {
    /// A worker claiming from `database`, pinged on `wake`: the subscription to
    /// [`afd_bus::Subject::DeliveryWake`] its daemon opened at boot.
    #[must_use]
    pub const fn new(
        database: Db,
        wake: Subscription,
        posters: Posters<S>,
        entropy: Entropy,
    ) -> Self {
        Self {
            database,
            wake,
            posters,
            entropy,
            renew: delivery::CLAIM_RENEW,
            retries: crate::retry::RetryPolicy::DEFAULT,
        }
    }

    /// Applies the checked inline retry budget to every delivery lane.
    #[must_use]
    pub const fn with_retries(mut self, retries: crate::retry::RetryPolicy) -> Self {
        self.retries = retries;
        self
    }

    /// Replaces how often a delivery renews its claim, capped at
    /// [`delivery::CLAIM_RENEW_MAX`] so a configured interval cannot let a
    /// claim lapse between two renewals.
    #[must_use]
    pub fn with_claim_renew(mut self, renew: Duration) -> Self {
        self.renew = renew.min(delivery::CLAIM_RENEW_MAX);
        self
    }

    /// Claims and delivers until `token` is cancelled, then drains the lanes.
    pub async fn run(self, token: CancellationToken) {
        let Self {
            database,
            wake,
            posters,
            entropy,
            renew,
            retries,
        } = self;
        let mut wake = Some(wake);
        let lanes = Lanes::configured(posters, database.clone(), token.clone(), renew, retries);
        let mut lapse: Option<tokio::time::Instant> = None;
        let mut via = Via::Scan;
        while !token.is_cancelled() {
            // With no room, the next finished job rings the loop.
            let room = lanes.room();
            if room > 0 {
                let claiming = tokio::time::Instant::now();
                match claim_once(&database, &entropy, &lanes, via, room).await {
                    Ok(claimed) if claimed == room => {
                        via = Via::Scan;
                        continue;
                    }
                    Ok(_partial) => match next_lapse(&database, &lanes, claiming.elapsed()).await {
                        Ok(next) => lapse = next,
                        // The lapse already known still stands; the next claim reads again.
                        Err(failure) => report(EVENT_LAPSE_UNREAD, &failure),
                    },
                    // The next ping or finished delivery claims again.
                    Err(failure) => report(EVENT_CLAIM_FAILED, &failure),
                }
            }
            via = tokio::select! {
                biased;
                () = token.cancelled() => break,
                () = pinged(&mut wake) => Via::Ping,
                // A finished job may have freed its lane's next row.
                () = lanes.finished() => Via::Scan,
                // A claim another replica stopped renewing is now claimable.
                () = afd_bus::sleep_until_some(lapse) => { lapse = None; Via::Scan }
            };
        }
        lanes.drain().await;
    }
}

/// Claims at most `room` rows and dispatches them; answers how many it took.
///
/// # Errors
/// The claim token could not be drawn, or the ledger would not answer or
/// refused the claim; nothing was taken either way.
async fn claim_once<S: Deliver + 'static>(
    database: &Db,
    entropy: &Entropy,
    lanes: &Lanes<S>,
    via: Via,
    room: usize,
) -> crate::Result<usize> {
    let via_label = via.label().as_str();
    tracing::debug!(via = via_label, event = EVENT_CLAIM_STARTED);
    let token = entropy.uuid7(af_core::clock::now())?;
    // The room is at most the lanes' ceiling, a handful.
    let limit = i64::try_from(room).unwrap_or(i64::MAX);
    let claimed = delivery::claim(database, limit, &token).await?;
    let count = claimed.len();
    {
        let (via, claimed, event) = (&via_label, &count, &EVENT_CLAIM_COMPLETED);
        tracing::debug!(via, claimed, event);
    };
    if count > 0 {
        af_observability::producers::outbound::claims(
            via.label(),
            u64::try_from(count).unwrap_or(u64::MAX),
        );
    }
    for job in claimed {
        lanes.dispatch(Box::new(job)).await;
    }
    Ok(count)
}

impl Via {
    const fn label(self) -> af_observability::metrics::label::outbound::ClaimVia {
        use af_observability::metrics::label::outbound::ClaimVia;
        match self {
            Self::Ping => ClaimVia::Ping,
            Self::Scan => ClaimVia::Scan,
        }
    }
}

/// Returns on the next ping, or never once the subscription is gone.
async fn pinged(wake: &mut Option<Subscription>) {
    let Some(subscription) = wake.as_mut() else {
        return std::future::pending().await;
    };
    if let Err(failure) = subscription.recv().await {
        unpinged(EVENT_WAKE_CLOSED, &failure);
        *wake = None;
    }
}

/// When the earliest claim another holder took lapses, as an instant on this replica's
/// clock; `None` when no other holder holds an open delivery. The deliveries `lanes` hold
/// are this replica's own, renewed while they run, so they are left out. `since` is how
/// long ago the claim this read follows began: a claim that lapsed in between is now.
///
/// # Errors
/// The ledger would not answer.
async fn next_lapse<S: Deliver + 'static>(
    database: &Db,
    lanes: &Lanes<S>,
    since: Duration,
) -> crate::Result<Option<tokio::time::Instant>> {
    let left = delivery::next_lapse(database, &lanes.held(), since).await?;
    Ok(left.map(|left| tokio::time::Instant::now() + left))
}

/// Logs that pings stopped reaching this worker.
fn unpinged(event: &'static str, failure: &afd_bus::Error) {
    let error_code = failure.code().as_str();
    let reason = failure.reason();
    tracing::warn!(error_code, reason, event);
}

/// Logs a failure the loop continues past.
pub(crate) fn report(event: &'static str, failure: &crate::Error) {
    let Logged { error_code, reason } = failure.logged();
    tracing::warn!(error_code, reason, event);
}

#[cfg(test)]
mod tests;
