//! Client events become gaps: a reconnect or a slow consumer gaps every local
//! subscription, and a reconnect pays the publish debt as markers.
//!
//! The client's event callback only sets flags and wakes the watcher, so a
//! burst of events coalesces into one gap per cause, and the callback never
//! waits on a lock the publish path holds.

use std::{
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use async_nats::{Client, Event, HeaderMap, Subscriber};
use bytes::Bytes;
use futures_util::StreamExt as _;
use tokio::sync::Notify;

use af_core::error_code::{Coded as _, Logged};
use af_observability::{metrics::label::bus::GapCause as Cause, producers};

use super::{Tuning, debt::Debt, hub};
use crate::Result;

/// What the event callback has seen since the watcher last looked.
#[derive(Debug)]
pub(super) struct Signals {
    deadline: Duration,
    down: AtomicBool,
    /// Set from a disconnect until the reconnect's markers are paid, so no
    /// frame reaches a subscriber ahead of the gap it is owed.
    blocked: AtomicBool,
    reconnected: AtomicBool,
    /// Disconnects seen, so a payer can tell a later one overtook it.
    disconnects: AtomicU64,
    slow: AtomicBool,
    wake: Notify,
    debt: Mutex<Debt>,
}

impl Signals {
    pub(super) fn new(tuning: &Tuning) -> Self {
        Self {
            deadline: tuning.publish_deadline(),
            down: AtomicBool::new(false),
            blocked: AtomicBool::new(false),
            reconnected: AtomicBool::new(false),
            disconnects: AtomicU64::new(0),
            slow: AtomicBool::new(false),
            wake: Notify::new(),
            debt: Mutex::new(Debt::new(tuning.debt_window(), tuning.debt_subjects_max)),
        }
    }

    fn debt(&self) -> MutexGuard<'_, Debt> {
        // Every critical section leaves the sets whole.
        self.debt.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fold one client event into the flags; never blocks on I/O.
    pub(super) fn observe(&self, event: &Event, now: Instant) {
        match event {
            Event::Disconnected => {
                // Publishes fail fast from here until the reconnect's markers
                // are paid, so an operator sees why before the gaps arrive.
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(event = "bus_disconnected", error_code);
                // Under the debt lock, which lifting the block also takes, so
                // a disconnect that lands while markers are paid is never lost.
                let mut debt = self.debt();
                debt.disconnected(now);
                self.disconnects.fetch_add(1, Ordering::AcqRel);
                self.blocked.store(true, Ordering::Release);
                self.down.store(true, Ordering::Release);
            }
            Event::Connected => {
                if self.down.swap(false, Ordering::AcqRel) {
                    af_observability::producers::http::hub_reconnected();
                    self.reconnected.store(true, Ordering::Release);
                    self.wake.notify_one();
                }
            }
            Event::SlowConsumer(_) => {
                // The sid is private to the client, so every subscription gaps.
                self.slow.store(true, Ordering::Release);
                self.wake.notify_one();
            }
            _ => {}
        }
    }

    /// Everything owed, as `pay` takes it; a test stands in for a payment.
    #[cfg(test)]
    pub(super) fn take_debt(&self) -> super::debt::Owed {
        self.debt().take()
    }

    /// The disconnects seen so far, read before paying a reconnect's markers.
    pub(super) fn disconnects(&self) -> u64 {
        self.disconnects.load(Ordering::Acquire)
    }

    /// Lift the publish block once every marker is paid and no disconnect
    /// landed since `before`; answers whether it lifted. A marker still owed
    /// keeps publishes failing fast, so no frame reaches a subscriber ahead
    /// of the gap it is owed.
    pub(super) fn unblock_if_settled(&self, before: u64) -> bool {
        let debt = self.debt();
        let settled = debt.is_settled() && self.disconnects() == before;
        if settled {
            self.blocked.store(false, Ordering::Release);
        }
        settled
    }

    /// Whether a publish must fail now rather than queue behind an outage.
    pub(super) fn blocked(&self) -> bool {
        self.blocked.load(Ordering::Acquire)
    }

    /// Remember a publish, or owe its subject a marker when it failed.
    pub(super) fn record(&self, physical: &str, delivered: bool, now: Instant) {
        let mut debt = self.debt();
        if delivered {
            debt.published(physical, now);
        } else {
            debt.failed(physical);
        }
    }

    /// Take the causes raised since the last call, in the order they apply.
    pub(super) fn raised(&self) -> Vec<Cause> {
        let mut causes = Vec::new();
        if self.reconnected.swap(false, Ordering::AcqRel) {
            causes.push(Cause::Reconnect);
        }
        if self.slow.swap(false, Ordering::AcqRel) {
            causes.push(Cause::SlowConsumer);
        }
        causes
    }
}

/// Tell every reader and return how many were told.
pub(super) fn gap(channels: &hub::Channels, cause: Cause) -> usize {
    let subscriptions = channels.gap_all();
    producers::bus::gaps(cause, u64::try_from(subscriptions).unwrap_or(u64::MAX));
    let cause_label = cause.as_str();
    let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
    tracing::warn!(
        event = "bus_gap_sent",
        error_code,
        cause = cause_label,
        subscriptions
    );
    subscriptions
}

fn marker() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(hub::GAP_HEADER, hub::GAP_VALUE);
    headers
}

async fn mark(client: &Client, physical: String, deadline: Duration) -> Result<()> {
    let publish = client.publish_with_headers(physical, marker(), Bytes::new());
    tokio::time::timeout(deadline, publish).await??;
    Ok(())
}

/// Send each owed marker once; what fails stays owed for the next reconnect.
pub(super) async fn pay(client: &Client, signals: &Signals, everyone: &str) -> usize {
    let owed = signals.debt().take();
    let mut paid = 0;
    for subject in owed.subjects {
        match mark(client, subject.clone(), signals.deadline).await {
            Ok(()) => paid += 1,
            Err(failure) => {
                mark_failed(&failure);
                signals.debt().failed(&subject);
            }
        }
    }
    if owed.everyone {
        match mark(client, everyone.to_owned(), signals.deadline).await {
            Ok(()) => paid += 1,
            Err(failure) => {
                mark_failed(&failure);
                signals.debt().everyone();
            }
        }
    }
    if paid > 0 {
        tracing::info!(event = "bus_gap_marked", markers = paid);
    }
    paid
}

/// A marker that would not publish stays owed; the watcher retries it.
fn mark_failed(failure: &crate::Error) {
    let Logged { error_code, reason } = failure.logged();
    tracing::warn!(event = "bus_gap_mark_failed", error_code, reason);
}

/// Pay what a reconnect owes and lift the block if nothing overtook it;
/// answers when to try again, or `None` once settled.
async fn settle(
    client: &Client,
    signals: &Signals,
    everyone: &str,
) -> Option<tokio::time::Instant> {
    let before = signals.disconnects();
    pay(client, signals, everyone).await;
    (!signals.unblock_if_settled(before)).then(|| tokio::time::Instant::now() + signals.deadline)
}

/// Turn raised causes into gaps and markers until the adapter shuts down.
pub(super) async fn watch(
    client: Client,
    channels: Arc<hub::Channels>,
    signals: Arc<Signals>,
    mut everyone: Subscriber,
    everyone_subject: String,
) {
    let mut retry: Option<tokio::time::Instant> = None;
    loop {
        tokio::select! {
            () = signals.wake.notified() => {
                for cause in signals.raised() {
                    gap(&channels, cause);
                    if cause == Cause::Reconnect {
                        retry = settle(&client, &signals, &everyone_subject).await;
                    }
                }
            }
            () = sleep_until_some(retry) => {
                retry = settle(&client, &signals, &everyone_subject).await;
            }
            next = everyone.next() => {
                if next.is_some() {
                    gap(&channels, Cause::PublishDebt);
                } else {
                    // Without the broadcast no hub hears a debt overflow, and
                    // a later reconnect is never gapped: say so before stopping.
                    let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                    tracing::warn!(event = "bus_gap_watcher_stopped", error_code);
                    return;
                }
            }
        }
    }
}

/// Sleeps until `at`, or forever when there is nothing to retry.
async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
