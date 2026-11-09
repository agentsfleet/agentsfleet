//! Request identifiers and worker permits are returned by Drop on every exit path.

use crate::{Result, error::ErrorKind};
use af_wire::protocol::{PENDING_MAX, StreamKind};
use dashmap::DashSet;
use std::{num::NonZeroU64, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Separate operation slots preserve control capacity when reports or activity are slow.
pub(super) struct RequestSlots {
    control: Arc<Semaphore>,
    reports: Arc<Semaphore>,
    work: Arc<Semaphore>,
    activity: Arc<Semaphore>,
}

impl Default for RequestSlots {
    fn default() -> Self {
        Self {
            control: Arc::new(Semaphore::new(
                PENDING_MAX - af_wire::protocol::LEASES_MAX - 24,
            )),
            reports: Arc::new(Semaphore::new(8)),
            work: Arc::new(Semaphore::new(af_wire::protocol::LEASES_MAX)),
            activity: Arc::new(Semaphore::new(16)),
        }
    }
}

impl RequestSlots {
    fn pool(&self, kind: StreamKind) -> &Arc<Semaphore> {
        match kind {
            StreamKind::Control => &self.control,
            StreamKind::Reports => &self.reports,
            StreamKind::Work => &self.work,
            StreamKind::Activity => &self.activity,
        }
    }

    pub(super) fn available(&self, kind: StreamKind) -> bool {
        self.pool(kind).available_permits() > 0
    }

    pub(super) fn admit(&self, kind: StreamKind) -> Option<OwnedSemaphorePermit> {
        Arc::clone(self.pool(kind)).try_acquire_owned().ok()
    }
}

/// A frame is decoded once in its worker; the proof then reserves its correlation id.
#[derive(Default)]
pub(super) struct RequestTracker(DashSet<NonZeroU64>);

impl RequestTracker {
    pub(super) fn register(
        self: &Arc<Self>,
        id: Option<NonZeroU64>,
    ) -> Result<RequestRegistration> {
        if id.is_some_and(|id| !self.0.insert(id)) {
            return Err(ErrorKind::Protocol.into());
        }
        Ok(RequestRegistration {
            tracker: Arc::clone(self),
            id,
        })
    }
}

pub(super) struct RequestRegistration {
    tracker: Arc<RequestTracker>,
    id: Option<NonZeroU64>,
}

impl Drop for RequestRegistration {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.tracker.0.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestSlots, RequestTracker};
    use af_wire::protocol::StreamKind;
    use std::{num::NonZeroU64, sync::Arc};

    /// Every return path releases identity, including an aborted worker unwinding its future.
    #[test]
    fn request_identity_is_exclusive_until_its_registration_drops() {
        let tracker = Arc::new(RequestTracker::default());
        let first = tracker.register(Some(NonZeroU64::MIN));
        assert!(first.is_ok());
        assert!(tracker.register(Some(NonZeroU64::MIN)).is_err());
        drop(first);
        assert!(tracker.register(Some(NonZeroU64::MIN)).is_ok());
    }

    /// Cosmetic work cannot spend the capacity reserved for heartbeat and renewal handling.
    #[test]
    fn activity_pressure_leaves_control_request_capacity_available() {
        let slots = RequestSlots::default();
        let mut held = Vec::new();
        while let Some(permit) = slots.admit(StreamKind::Activity) {
            held.push(permit);
        }
        assert!(!slots.available(StreamKind::Activity));
        assert!(slots.admit(StreamKind::Control).is_some());
        drop(held);
        assert!(slots.available(StreamKind::Activity));
    }
}
