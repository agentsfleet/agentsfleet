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

use afd_core::id::Uuid7;
use afd_cron::{
    Change, EditRequest, Fire, FireTarget, Fired, NewSchedule, Reconciled, Refused,
    Result as CronResult, Schedule, ScheduleService, Schedules,
};
use afd_crypto::entropy::Entropy;
use afd_db::Precondition;
use jiff::Timestamp;

/// Everything the schedules surface and the fire ingress act through.
pub trait FleetSchedules: Send + Sync + std::fmt::Debug + 'static {
    /// Conditionally edits intent and reconciles the fence claimed by that edit.
    /// `now` mints the fence token.
    ///
    /// # Errors
    /// Preserves hidden, stale, invalid and sync-held refusals from the store.
    fn edit(
        &self,
        request: EditRequest<'_>,
        now: Timestamp,
    ) -> impl Future<Output = CronResult<Reconciled>> + Send;

    /// This fleet's schedules, oldest first.
    ///
    /// None for a fleet outside `workspace`, exactly as for a fleet with no
    /// schedules: the caller is proven in `workspace`, and the fleet in its path
    /// is only a claim until it is checked here.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    fn list(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = CronResult<Vec<Schedule>>> + Send;

    /// One schedule of this fleet's.
    ///
    /// # Errors
    /// As [`Self::list`]. A schedule belonging to another fleet, or to a fleet
    /// outside `workspace`, is `Ok(None)`, indistinguishable from one that
    /// never existed.
    fn one(
        &self,
        workspace: &Uuid7,
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
        now: Timestamp,
    ) -> impl Future<Output = CronResult<Result<Reconciled, Refused>>> + Send;

    /// Applies a change and pushes the result upstream.
    ///
    /// `Ok(None)` for a fleet outside `workspace`, and nothing is written or
    /// pushed.
    ///
    /// # Errors
    /// As [`Self::list`].
    fn change(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: Timestamp,
    ) -> impl Future<Output = CronResult<Option<Reconciled>>> + Send;

    /// [`Self::create`] for a fleet, under the lease `guard` proves on the
    /// write's own transaction.
    ///
    /// # Errors
    /// As [`Self::create`]. A lease that no longer holds is
    /// `Ok(Err(Refused::Unheld))`, and nothing is written.
    fn create_guarded(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: Timestamp,
        guard: &dyn Precondition,
    ) -> impl Future<Output = CronResult<Result<Reconciled, Refused>>> + Send;

    /// [`Self::change`] for a fleet, under the lease `guard` proves on the
    /// write's own transaction.
    ///
    /// # Errors
    /// As [`Self::change`]. A lease that no longer holds is
    /// `Ok(Err(Refused::Unheld))`, and nothing is written.
    fn change_guarded(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: Timestamp,
        guard: &dyn Precondition,
    ) -> impl Future<Output = CronResult<Result<Option<Reconciled>, Refused>>> + Send;

    /// Pushes what the row already says, changing nothing.
    ///
    /// `Ok(None)` for a fleet outside `workspace`, as for [`Self::change`].
    ///
    /// # Errors
    /// As [`Self::list`].
    fn sync(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: Timestamp,
    ) -> impl Future<Output = CronResult<Option<Reconciled>>> + Send;

    /// What a signed fire resolves to.
    ///
    /// # Errors
    /// As [`Self::list`].
    fn fire_target(
        &self,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<FireTarget>>> + Send;

    /// Appends one fire, at most once however often it arrives, and retires
    /// a `once` schedule once its fire is admitted.
    ///
    /// The retirement is here rather than in either caller because the
    /// `QStash` callback and a fleet's run-now are both fires, and a `once`
    /// schedule run now must not fire again on its own expression.
    ///
    /// # Errors
    /// Reports a ledger that would not record the fire, and what
    /// [`Self::retire`] reports. Either is retried safely: a repeated fire
    /// replays the admitted event, and a repeated retirement claims a row
    /// already on its way out.
    fn fire(
        &self,
        schedule: &Uuid7,
        target: &FireTarget,
        message_id: &str,
        now: Timestamp,
    ) -> impl Future<Output = CronResult<Fired>> + Send;

    /// Marks `fleet`'s `schedule` in `workspace` for deletion and removes it
    /// from `QStash`: a `once` schedule whose moment has come, fired or not.
    ///
    /// A schedule already gone is retired.
    ///
    /// # Errors
    /// `UZ-SCHED-006` while another syncer holds the row, so the caller answers
    /// non-2xx and the fire is repeated rather than the schedule matching its
    /// moment again next year. Reports a store that would not take the change.
    fn retire(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: Timestamp,
    ) -> impl Future<Output = CronResult<()>> + Send;
}

mod guarded;
mod plane;

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
    fn token(&self, now: Timestamp) -> CronResult<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }

    /// The store beneath the reconciler.
    const fn store(&self) -> &Schedules {
        self.service.store()
    }
}
