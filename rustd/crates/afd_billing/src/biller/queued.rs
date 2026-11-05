//! Money moved off the request path.
//!
//! A person's message or a fleet's report should never wait on the ledger.
//! [`Queued`] offers the calls whose answer decides nothing to an
//! [`afd_queue::Sink`] and answers at once: the hold a claim places, every
//! settlement and every release. A balance check that can stop a run,
//! [`Biller::reserve`], still waits for its answer, so no budget is weakened.
//!
//! The sink is a seam. [`Queued::in_memory`] carries jobs on a channel inside
//! the daemon; a broker (Amazon SQS, Cloudflare Queues, Upstash `QStash`,
//! Kafka) is another [`afd_queue::Sink`] handed to [`Queued::new`], with
//! [`Mover`] as its consumer. Nothing is lost when a job is: the work's own
//! transaction already wrote its usage lines, and the billing sweepers settle
//! unsettled lines and release finished holds, exactly as they repair a biller
//! that was down. Every call is idempotent on its key, so a retried or
//! redelivered job never charges twice.

use std::sync::Arc;
use std::time::Duration;

use std::hash::{DefaultHasher, Hash, Hasher};

use afd_queue::{Consumer, Memory, Offer, Outcome, Settings, Shard, Sink};

use super::{Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey};
use crate::Result;

/// Jobs each worker holds waiting before a caller moves its own money.
const QUEUE_DEPTH: usize = 1_024;
/// Workers moving money at once; one work's jobs always share a worker.
const WORKERS: usize = 4;
/// Pauses between attempts at a job the biller could not answer.
const RETRY: &[Duration] = &[
    Duration::from_millis(200),
    Duration::from_secs(1),
    Duration::from_secs(5),
];
const EVENT_GAVE_UP: &str = "billing_queue_deferred";

/// One money movement waiting to be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingJob {
    /// Place or raise a hold.
    Reserve(Reservation),
    /// Charge logged lines.
    Settle(Settlement),
    /// Charge what is left and free the hold.
    Release(WorkKey, Release),
}

impl BillingJob {
    /// The work the money is for.
    #[must_use]
    pub const fn work(&self) -> &WorkKey {
        match self {
            Self::Reserve(ask) => &ask.work,
            Self::Settle(settlement) => &settlement.work,
            Self::Release(work, _) => work,
        }
    }
}

/// `inner`, with holds at claim, settlements and releases offered to a sink.
#[derive(Debug)]
pub struct Queued {
    mover: Arc<Mover>,
    sink: Arc<dyn Sink<BillingJob>>,
}

impl Queued {
    /// Money moved by whatever consumes `sink`, normally [`Mover`] over `inner`.
    #[must_use]
    pub fn new(inner: Arc<dyn Biller>, sink: Arc<dyn Sink<BillingJob>>) -> Self {
        Self {
            mover: Arc::new(Mover { inner }),
            sink,
        }
    }

    /// Money moved by a worker in this process. Outside a Tokio runtime there
    /// is no worker, and every movement happens before the call returns.
    #[must_use]
    pub fn in_memory(inner: Arc<dyn Biller>) -> Self {
        let mover = Arc::new(Mover { inner });
        let settings = Settings {
            depth: QUEUE_DEPTH,
            workers: WORKERS,
            retry: RETRY,
        };
        let sink = Arc::new(Memory::start(
            &(Arc::clone(&mover) as Arc<dyn Consumer<BillingJob>>),
            settings,
            by_work(),
        ));
        Self { mover, sink }
    }

    /// Offers `job`; a sink that cannot take it leaves the caller to move the money once.
    async fn offer(&self, job: BillingJob) {
        if let Offer::Refused(job) = self.sink.offer(job)
            && self.mover.consume(&job).await == Outcome::Retry
        {
            self.mover.abandoned(&job);
        }
    }
}

#[async_trait::async_trait]
impl Biller for Queued {
    /// Waits: a run's budget depends on this answer.
    async fn reserve(&self, ask: &Reservation) -> Result<Hold> {
        self.mover.inner.reserve(ask).await
    }

    /// Offered: the claim's gate already admitted the bound, so the hold's
    /// answer only confirms it. Answers as an unreachable biller would.
    async fn reserve_detached(&self, ask: &Reservation) -> Result<Hold> {
        self.offer(BillingJob::Reserve(ask.clone())).await;
        Ok(Hold::Unconfirmed)
    }

    /// Offered: the lines are already logged; charging them waits for no one.
    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        self.offer(BillingJob::Settle(settlement.clone())).await;
        Ok(Receipt::default())
    }

    /// Offered behind the work's settlements, so nothing is freed before it is charged.
    async fn release(&self, work: &WorkKey, why: Release) -> Result<()> {
        self.offer(BillingJob::Release(work.clone(), why)).await;
        Ok(())
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.mover.inner.balance(payer).await
    }
}

/// Routes every job for one work to one worker, so its hold, settlements and
/// release keep the order they were offered in.
fn by_work() -> Shard<BillingJob> {
    Arc::new(|job: &BillingJob| {
        let mut hasher = DefaultHasher::new();
        job.work().id().as_str().hash(&mut hasher);
        hasher.finish()
    })
}

/// The consumer that makes each [`BillingJob`]'s money movement through a biller.
#[derive(Debug)]
pub struct Mover {
    inner: Arc<dyn Biller>,
}

impl Mover {
    /// Moves money through `inner`.
    #[must_use]
    pub const fn new(inner: Arc<dyn Biller>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl Consumer<BillingJob> for Mover {
    async fn consume(&self, message: &BillingJob) -> Outcome {
        let moved = match message {
            BillingJob::Reserve(ask) => self.inner.reserve(ask).await.map(drop),
            BillingJob::Settle(settlement) => self.inner.settle(settlement).await.map(drop),
            BillingJob::Release(work, why) => self.inner.release(work, *why).await,
        };
        if moved.is_ok() {
            Outcome::Done
        } else {
            Outcome::Retry
        }
    }

    fn abandoned(&self, message: &BillingJob) {
        tracing::warn!(
            work_id = message.work().id().as_str(),
            event = EVENT_GAVE_UP,
            "the billing queue could not move this money; the billing sweeper will"
        );
    }
}

#[cfg(test)]
#[path = "queued_tests.rs"]
mod tests;
