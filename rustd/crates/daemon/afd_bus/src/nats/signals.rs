//! Client events become gaps: a reconnect or a slow consumer gaps every local
//! subscription, and a reconnect pays the publish debt as markers.
//!
//! The client's event callback only sets flags and wakes the watcher, so a
//! burst of events coalesces into one gap per cause, and the callback never
//! waits on a lock the publish path holds.

use std::{
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_nats::{Client, Event, HeaderMap, Subscriber};
use bytes::Bytes;
use futures_util::StreamExt as _;
use tokio::sync::Notify;

use super::{Tuning, debt::Debt, hub};
use crate::Result;

/// Why a subscription was told frames may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The client lost its server and found another.
    Reconnect,
    /// The client's buffer for one subscription overflowed.
    SlowConsumer,
    /// A publisher owed a marker after a failed or interrupted publish.
    PublishDebt,
}

impl Cause {
    /// Bounded label spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reconnect => "reconnect",
            Self::SlowConsumer => "slow_consumer",
            Self::PublishDebt => "publish_debt",
        }
    }
}

/// What the event callback has seen since the watcher last looked.
#[derive(Debug)]
pub(super) struct Signals {
    deadline: Duration,
    down: AtomicBool,
    /// Set from a disconnect until the reconnect's markers are paid, so no
    /// frame reaches a subscriber ahead of the gap it is owed.
    blocked: AtomicBool,
    reconnected: AtomicBool,
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
                self.blocked.store(true, Ordering::Release);
                self.down.store(true, Ordering::Release);
                self.debt().disconnected(now);
            }
            Event::Connected => {
                if self.down.swap(false, Ordering::AcqRel) {
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
    let cause_label = cause.as_str();
    tracing::warn!(event = "bus_gap_sent", cause = cause_label, subscriptions);
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
        if mark(client, subject.clone(), signals.deadline)
            .await
            .is_ok()
        {
            paid += 1;
        } else {
            signals.debt().failed(&subject);
        }
    }
    if owed.everyone {
        if mark(client, everyone.to_owned(), signals.deadline)
            .await
            .is_ok()
        {
            paid += 1;
        } else {
            signals.debt().everyone();
        }
    }
    if paid > 0 {
        tracing::info!(event = "bus_gap_marked", markers = paid);
    }
    paid
}

/// Turn raised causes into gaps and markers until the adapter shuts down.
pub(super) async fn watch(
    client: Client,
    channels: Arc<hub::Channels>,
    signals: Arc<Signals>,
    mut everyone: Subscriber,
    everyone_subject: String,
) {
    loop {
        tokio::select! {
            () = signals.wake.notified() => {
                for cause in signals.raised() {
                    gap(&channels, cause);
                    if cause == Cause::Reconnect {
                        pay(&client, &signals, &everyone_subject).await;
                        signals.blocked.store(false, Ordering::Release);
                    }
                }
            }
            next = everyone.next() => match next {
                Some(_) => {
                    gap(&channels, Cause::PublishDebt);
                }
                None => return,
            },
        }
    }
}
