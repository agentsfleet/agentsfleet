//! Attempt-keyed worker reservations survive held/busy transfers exactly once.

use af_core::id::Uuid7;
use tokio::sync::mpsc;

use super::{Holds, Request};

/// One run of a fleet, which consumes a worker of the local budget.
#[derive(Debug)]
pub(crate) struct Occupant {
    /// The run.
    pub(crate) run: Uuid7,
    /// The fleet it runs, whose hold a park transfers into.
    pub(crate) fleet: Uuid7,
}

impl Holds {
    /// Reserves work before a fleet's run starts. Park transfers this same
    /// run into a hold; a later drop cannot vacate another attempt.
    pub(crate) fn occupy(&self, occupant: Occupant) -> Occupied {
        let run = occupant.run.clone();
        let _stopped = self.requests.send(Request::Occupy { occupant });
        Occupied {
            requests: self.requests.clone(),
            run,
        }
    }
}

/// Dropping the reservation releases only its attempt, including unwind/cancel.
#[derive(Debug)]
pub(crate) struct Occupied {
    requests: mpsc::UnboundedSender<Request>,
    run: Uuid7,
}

impl Drop for Occupied {
    fn drop(&mut self) {
        let _stopped = self.requests.send(Request::Vacate {
            run: self.run.clone(),
        });
    }
}
