//! The seam the schedules surface and the fire ingress act through.
//!
//! One trait over the store and the reconciler together, because they are one
//! decision from a route's side: every write on this surface is "change the row
//! and then tell the scheduler", and a seam that split them would let a handler
//! write a row and forget the push — which is a schedule that exists and never
//! fires, the exact failure the `sync_status` column exists to make visible.
//!
//! # The fire path crosses the same seam
//!
//! [`FleetSchedules::fire_target`] and [`FleetSchedules::fire`] are here rather
//! than in a seam of their own, because they read and write the same two stores
//! the CRUD half does. A second trait would mean a second stub in every suite
//! that arranges either.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_cron::{
    Change, Fire, FireTarget, Fired, NewSchedule, Reconciled, Refused, Result as CronResult,
    Schedule, ScheduleService, Schedules,
};
use afd_crypto::entropy::Entropy;

/// Everything the schedules surface and the fire ingress act through.
pub trait FleetSchedules: Send + Sync + std::fmt::Debug + 'static {
    /// This fleet's schedules, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    fn list(&self, fleet: &Uuid7) -> impl Future<Output = CronResult<Vec<Schedule>>> + Send;

    /// One schedule of this fleet's.
    ///
    /// # Errors
    /// As [`Self::list`]. A schedule belonging to another fleet is `Ok(None)`,
    /// indistinguishable from one that never existed.
    fn one(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<Schedule>>> + Send;

    /// Creates a schedule and registers it upstream.
    ///
    /// Answers the row AND what the push did, because the caller renders both:
    /// a schedule that saved and failed to register is a 201 with a visible
    /// `failed` sync state, not an error.
    ///
    /// # Errors
    /// As [`Self::list`]. A bound the operator hit is `Ok(Err(..))`.
    fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = CronResult<Result<Reconciled, Refused>>> + Send;

    /// Applies a change and pushes the result upstream.
    ///
    /// # Errors
    /// As [`Self::list`].
    fn change(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = CronResult<Option<Reconciled>>> + Send;

    /// Pushes what the row already says, changing nothing.
    ///
    /// # Errors
    /// As [`Self::list`].
    fn sync(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = CronResult<Option<Reconciled>>> + Send;

    /// What a signed fire resolves to.
    ///
    /// # Errors
    /// As [`Self::list`].
    fn fire_target(
        &self,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<FireTarget>>> + Send;

    /// Appends one verified fire, at most once however often it arrives.
    ///
    /// # Errors
    /// Reports a queue that would not take the append.
    fn fire(
        &self,
        schedule: &Uuid7,
        target: &FireTarget,
        message_id: &str,
    ) -> impl Future<Output = CronResult<Fired>> + Send;
}

/// The store, the reconciler and the appender, as one value.
///
/// A struct rather than three fields on the composition root, because the three
/// are only ever used together and a route that held one of them could not
/// finish a write.
#[derive(Debug, Clone)]
pub struct SchedulePlane {
    /// The reconciler, which owns the store.
    service: ScheduleService,
    /// Where a verified fire is appended.
    fire: Fire,
    /// Where a fence token's entropy comes from.
    ///
    /// Held rather than made per call: an entropy source is a handle on the
    /// system's, and building one per request would pay for that handle on
    /// every schedule edit.
    entropy: Entropy,
}

impl SchedulePlane {
    /// Binds the plane to a reconciler and an appender.
    #[must_use]
    pub const fn new(service: ScheduleService, fire: Fire, entropy: Entropy) -> Self {
        Self {
            service,
            fire,
            entropy,
        }
    }

    /// A fence token for one attempt.
    ///
    /// Minted per attempt and never reused: the token is what a finalize proves
    /// this caller still holds the row with, so one that outlived its attempt
    /// would let a stale finalize land on a row another syncer had taken.
    fn token(&self, now: UnixMillis) -> CronResult<Uuid7> {
        let mut bytes = [0_u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }

    /// The store beneath the reconciler.
    const fn store(&self) -> &Schedules {
        self.service.store()
    }
}

impl FleetSchedules for SchedulePlane {
    fn list(&self, fleet: &Uuid7) -> impl Future<Output = CronResult<Vec<Schedule>>> + Send {
        self.store().list(fleet)
    }

    fn one(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<Schedule>>> + Send {
        self.store().one(fleet, schedule)
    }

    async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: UnixMillis,
    ) -> CronResult<Result<Reconciled, Refused>> {
        let token = self.token(now)?;
        match self.store().create(workspace, new, &token, now).await? {
            Err(refused) => Ok(Err(refused)),
            Ok(created) => Ok(Ok(self.service.reconcile(&created, &token, now).await?)),
        }
    }

    async fn change(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: UnixMillis,
    ) -> CronResult<Option<Reconciled>> {
        let token = self.token(now)?;
        let Some(held) = self
            .store()
            .claim_change(fleet, schedule, change, &token, now)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.service.reconcile(&held, &token, now).await?))
    }

    async fn sync(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: UnixMillis,
    ) -> CronResult<Option<Reconciled>> {
        let token = self.token(now)?;
        let Some(held) = self
            .store()
            .claim_current(fleet, schedule, &token, now)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.service.reconcile(&held, &token, now).await?))
    }

    fn fire_target(
        &self,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<FireTarget>>> + Send {
        self.store().fire_target(schedule)
    }

    fn fire(
        &self,
        schedule: &Uuid7,
        target: &FireTarget,
        message_id: &str,
    ) -> impl Future<Output = CronResult<Fired>> + Send {
        self.fire.deliver(schedule, target, message_id)
    }
}
