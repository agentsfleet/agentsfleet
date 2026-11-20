//! Attempt-keyed worker reservations survive held/busy transfers exactly once.

use af_core::id::Uuid7;
use tokio::sync::mpsc;

use super::{Holds, Request};

/// Every kind of work consumes the same local worker budget.
#[derive(Debug)]
pub(crate) enum Occupant {
    Fleet {
        run: Uuid7,
        fleet: Uuid7,
    },
    /// A setup draft's run, which holds no sandbox and so no fleet's hold.
    Setup {
        run: Uuid7,
    },
}

impl Occupant {
    pub(super) fn parts(self) -> (Uuid7, Option<Uuid7>) {
        match self {
            Self::Fleet { run, fleet } => (run, Some(fleet)),
            Self::Setup { run } => (run, None),
        }
    }
}

impl Holds {
    /// Reserves work before a fleet or setup run starts. Park transfers
    /// this same run into a hold; a later drop cannot vacate another attempt.
    pub(crate) fn occupy(&self, occupant: Occupant) -> Occupied {
        let run = match &occupant {
            Occupant::Fleet { run, .. } | Occupant::Setup { run } => run.clone(),
        };
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
