//! Attempt-keyed worker reservations survive held/busy transfers exactly once.

use af_core::id::Uuid7;
use tokio::sync::mpsc;

use super::{Holds, Request};

/// Every kind of work consumes the same local worker budget.
#[derive(Debug)]
pub(crate) enum Occupant {
    Fleet { lease: Uuid7, fleet: Uuid7 },
    Authoring { lease: Uuid7 },
}

impl Occupant {
    pub(super) fn parts(self) -> (Uuid7, Option<Uuid7>) {
        match self {
            Self::Fleet { lease, fleet } => (lease, Some(fleet)),
            Self::Authoring { lease } => (lease, None),
        }
    }
}

impl Holds {
    /// Reserves work before either fleet or authoring starts. Park transfers
    /// this same lease into a hold; a later drop cannot vacate another attempt.
    pub(crate) fn occupy(&self, occupant: Occupant) -> Occupied {
        let lease = match &occupant {
            Occupant::Fleet { lease, .. } | Occupant::Authoring { lease } => lease.clone(),
        };
        let _stopped = self.requests.send(Request::Occupy { occupant });
        Occupied {
            requests: self.requests.clone(),
            lease,
        }
    }
}

/// Dropping the reservation releases only its attempt, including unwind/cancel.
#[derive(Debug)]
pub(crate) struct Occupied {
    requests: mpsc::UnboundedSender<Request>,
    lease: Uuid7,
}

impl Drop for Occupied {
    fn drop(&mut self) {
        let _stopped = self.requests.send(Request::Vacate {
            lease: self.lease.clone(),
        });
    }
}
