//! An offer keeps its durable target beside bytes serialized inside the policy borrow.

use af_core::id::Uuid7;
use jiff::Timestamp;

use super::activity::Target;

/// Inputs shared by fleet and authoring placement under one physical host limit.
#[derive(Debug)]
pub struct OfferRequest<'a> {
    /// Enrolled host receiving the offer.
    pub runner: &'a Uuid7,
    /// Fleets whose existing sandboxes this host holds.
    pub held: &'a [Uuid7],
    /// An unreadable or degraded capability verdict refuses new work.
    pub degraded: bool,
    /// Edge time used to mint durable identities.
    pub now: Timestamp,
    /// A recovery drain keeps its position across bounded candidate pages.
    pub recovery: Option<&'a mut RecoveryScan>,
}

/// Disposable progress through one physical session's recovery candidates.
#[derive(Debug, Default)]
pub struct RecoveryScan {
    pub(super) after: Option<RecoveryKey>,
    pub(super) deferred: bool,
    /// Progress beyond a skipped prefix before retrying its older heads.
    pub(super) claims_after_skip: usize,
}

#[derive(Debug)]
pub(super) struct RecoveryKey {
    pub(super) created: Timestamp,
    pub(super) admission: Uuid7,
}

impl RecoveryScan {
    /// Clear the previous attempt's outcome while retaining its position.
    pub fn begin_attempt(&mut self) {
        self.deferred = false;
    }

    /// The attempt reached the page ceiling and can resume after it.
    #[must_use]
    pub const fn deferred(&self) -> bool {
        self.deferred
    }
}

/// Metadata returned with the exact offer, without another run-target query.
#[derive(Debug, Clone)]
pub struct Offered {
    /// Durable run identity for acknowledgements and cancellation.
    pub run_id: Uuid7,
    /// Last confirmed deadline; transport activity cannot extend it.
    pub expires_at: Timestamp,
    /// Fleet activity target; authoring retains its private domain activity owner.
    pub target: Option<Target>,
    /// What this run's report reads, known at issue: a session that keeps it
    /// reports without reading the run again. `None`
    /// for authoring, and after a rebuild from the database.
    pub reported: Option<super::Reported>,
    /// The draft a draft-owned run runs a turn of; `None` for a fleet's.
    pub draft: Option<super::draft::DraftTarget>,
}
