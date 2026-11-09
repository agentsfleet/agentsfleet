//! The claim loop: Postgres holds every owed answer, and the worker claims
//! lane heads when it is pinged or when its pass comes round (M244 §4).
//!
//! ```text
//!   report commits ──► af.outbound.wake (lossy) ─┐
//!                                                 ├─► claim lane heads ─► lanes
//!   every CLAIM_PASS ─────────────────────────────┘        (FOR UPDATE SKIP LOCKED)
//! ```
//!
//! # A lost ping costs only latency
//!
//! The ping is at-most-once. A replica that misses it claims the row on its
//! next pass, at most [`CLAIM_PASS`] later; a full batch claims again at once,
//! so a backlog drains without waiting for passes.
//!
//! # The claim is the ordering, and the lanes are the concurrency
//!
//! Postgres hands out at most one held row per (provider, workspace) lane, so
//! two answers to one thread cannot be in flight together on any replica. The
//! lanes bound how many vendor calls this replica runs at once.

use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use af_crypto::entropy::Entropy;
use afd_bus::{Handle, Subject, Subscription};
use afd_db::Db;
use tokio_util::sync::CancellationToken;

use crate::lanes::{IN_FLIGHT_DELIVERIES, Lanes};
use crate::obligation;
use crate::poster::{Deliver, Posters};

/// How often a replica claims without being pinged, by default.
pub const CLAIM_PASS: Duration = Duration::from_secs(30);

/// Rows one claim takes: as many as the lanes deliver at once.
const CLAIM_BATCH: i64 = 8;
const _: () = assert!(
    IN_FLIGHT_DELIVERIES == 8,
    "a claim fills the lanes and no more"
);

/// Logged around each claim. A claim polls Postgres every pass, so both halves
/// sit at `debug`.
const EVENT_CLAIM_STARTED: &str = "outbound_claim_started";
const EVENT_CLAIM_COMPLETED: &str = "outbound_claim_completed";
/// Logged when a claim could not be made; the next pass retries.
const EVENT_CLAIM_FAILED: &str = "outbound_claim_failed";
/// Logged when pings stop reaching this worker; its pass carries it alone.
const EVENT_WAKE_SUBSCRIBE_FAILED: &str = "outbound_wake_subscribe_failed";
const EVENT_WAKE_CLOSED: &str = "outbound_wake_closed";

/// What triggered a claim, for the claim counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// A committed report's ping.
    Ping,
    /// The periodic pass, or a full batch draining a backlog.
    Scan,
}

/// The outbound worker: claims owed answers and hands them to the lanes.
#[derive(Debug)]
pub struct Worker<S> {
    database: Db,
    bus: Handle,
    posters: Posters<S>,
    entropy: Entropy,
    pass: Duration,
    renew: Duration,
}

impl<S: Deliver + 'static> Worker<S> {
    /// A worker claiming from `database`, pinged over `bus`.
    #[must_use]
    pub const fn new(database: Db, bus: Handle, posters: Posters<S>, entropy: Entropy) -> Self {
        Self {
            database,
            bus,
            posters,
            entropy,
            pass: CLAIM_PASS,
            renew: obligation::CLAIM_RENEW,
        }
    }

    /// Replaces how often the worker claims unpinged, such as from a
    /// configuration file.
    #[must_use]
    pub const fn with_claim_pass(mut self, pass: Duration) -> Self {
        self.pass = pass;
        self
    }

    /// Replaces how often a delivery renews its claim, capped at
    /// [`obligation::CLAIM_RENEW_MAX`] so a configured interval cannot let a
    /// claim lapse between two renewals.
    #[must_use]
    pub fn with_claim_renew(mut self, renew: Duration) -> Self {
        self.renew = renew.min(obligation::CLAIM_RENEW_MAX);
        self
    }

    /// Claims and delivers until `token` is cancelled, then drains the lanes.
    pub async fn run(self, token: CancellationToken) {
        let Self {
            database,
            bus,
            posters,
            entropy,
            pass,
            renew,
        } = self;
        let mut wake = bus
            .subscribe(&Subject::OutboundWake)
            .await
            .inspect_err(|failure| unpinged(EVENT_WAKE_SUBSCRIBE_FAILED, failure))
            .ok();
        let lanes = Lanes::renewing_every(posters, database.clone(), token.clone(), renew);
        let mut via = Via::Scan;
        while !token.is_cancelled() {
            let claimed = claim_once(&database, &entropy, &lanes, via).await;
            if claimed == Some(CLAIM_BATCH) {
                via = Via::Scan;
                continue;
            }
            via = tokio::select! {
                biased;
                () = token.cancelled() => break,
                () = pinged(&mut wake) => Via::Ping,
                // A finished job may have freed its lane's next row.
                () = lanes.finished() => Via::Scan,
                () = tokio::time::sleep(pass) => Via::Scan,
            };
        }
        lanes.drain().await;
    }
}

/// Claims one batch and dispatches it; answers how many rows it took, or
/// `None` when the claim failed.
async fn claim_once<S: Deliver + 'static>(
    database: &Db,
    entropy: &Entropy,
    lanes: &Lanes<S>,
    via: Via,
) -> Option<i64> {
    let via_label = via.label().as_str();
    tracing::debug!(via = via_label, event = EVENT_CLAIM_STARTED);
    let token = match entropy.uuid7(af_core::clock::now()) {
        Ok(token) => token,
        Err(failure) => {
            report(EVENT_CLAIM_FAILED, &failure.into());
            return None;
        }
    };
    let claimed = match obligation::claim(database, CLAIM_BATCH, &token).await {
        Ok(claimed) => claimed,
        Err(failure) => {
            report(EVENT_CLAIM_FAILED, &failure);
            return None;
        }
    };
    let count = i64::try_from(claimed.len()).unwrap_or(CLAIM_BATCH);
    tracing::debug!(
        via = via_label,
        claimed = count,
        event = EVENT_CLAIM_COMPLETED
    );
    if count > 0 {
        af_observability::producers::outbound::claims(via.label(), count.unsigned_abs());
    }
    for job in claimed {
        lanes.dispatch(Box::new(job)).await;
    }
    Some(count)
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

/// Returns on the next ping, or never when the subscription is gone; the pass
/// then carries the worker alone.
async fn pinged(wake: &mut Option<Subscription>) {
    let Some(subscription) = wake.as_mut() else {
        return std::future::pending().await;
    };
    if let Err(failure) = subscription.recv().await {
        unpinged(EVENT_WAKE_CLOSED, &failure);
        *wake = None;
        std::future::pending::<()>().await;
    }
}

/// Logs that pings stopped; answers now wait up to one claim pass.
fn unpinged(event: &'static str, failure: &afd_bus::Error) {
    let error_code = failure.code().as_str();
    let reason = failure.reason();
    tracing::warn!(error_code, reason, event, "claims wait for the pass");
}

/// Logs a failure the loop continues past.
pub(crate) fn report(event: &'static str, failure: &crate::Error) {
    let Logged { error_code, reason } = failure.logged();
    tracing::warn!(error_code, reason, event);
}

#[cfg(test)]
mod tests;
