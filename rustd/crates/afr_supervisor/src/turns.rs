//! One run per fleet at a time, however many workers there are.
//!
//! A coordinator task owns the table of which fleets are busy and who waits
//! for each; workers talk to it over a channel, so nothing is shared behind a
//! lock. The coordinator hands out a [`Turn`] by value. Dropping it — run
//! finished, run cancelled, or the grant never read because its claimer gave
//! up — tells the coordinator the fleet is free, which is what keeps a fleet
//! from staying busy forever on a lost grant.

use std::collections::{HashMap, VecDeque};

use afd_core::id::Uuid7;
use tokio::sync::{mpsc, oneshot};

/// One claimer: the fleet it asked for, and where its turn goes.
type Claim = (Uuid7, oneshot::Sender<Turn>);

/// What a worker asks of the coordinator.
#[derive(Debug)]
enum Request {
    /// Wait for the fleet to be free, then hold it.
    Claim(Claim),
    /// The fleet is free again.
    Release(Uuid7),
}

/// Asks for turns.
#[derive(Debug, Clone)]
pub(crate) struct FleetTurns {
    requests: mpsc::UnboundedSender<Request>,
}

/// One fleet, held; dropping it frees the fleet.
#[derive(Debug)]
pub(crate) struct Turn {
    fleet: Option<Uuid7>,
    requests: mpsc::UnboundedSender<Request>,
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(fleet) = self.fleet.take() {
            // A gone coordinator means the pool is gone; nothing is waiting.
            drop(self.requests.send(Request::Release(fleet)));
        }
    }
}

impl FleetTurns {
    /// A handle for workers, and the coordinator the caller must run.
    pub(crate) fn start() -> (Self, impl Future<Output = ()> + Send + 'static) {
        let (requests, receiver) = mpsc::unbounded_channel();
        let coordinator = coordinate(receiver, requests.downgrade());
        (Self { requests }, coordinator)
    }

    /// Waits until `fleet` is free, and holds it until the turn is dropped.
    ///
    /// `None` only when the coordinator has already stopped. The one copy of
    /// the identifier made here is the claimer's own: it rides to the
    /// coordinator and comes back inside the turn.
    pub(crate) async fn claim(&self, fleet: &Uuid7) -> Option<Turn> {
        let (granted, grant) = oneshot::channel();
        self.requests
            .send(Request::Claim((fleet.clone(), granted)))
            .ok()?;
        grant.await.ok()
    }
}

/// Grants turns until every handle and turn is gone.
///
/// A fleet in the table is busy; its queue holds the claims waiting for it.
/// The first claim's identifier becomes the table's key and the turn gets the
/// one copy it needs to release; a waiting claim keeps its own identifier and
/// receives it back in its turn.
async fn coordinate(
    mut requests: mpsc::UnboundedReceiver<Request>,
    own: mpsc::WeakUnboundedSender<Request>,
) {
    let mut busy: HashMap<Uuid7, VecDeque<Claim>> = HashMap::new();
    while let Some(request) = requests.recv().await {
        match request {
            Request::Claim((fleet, granted)) => {
                if let Some(waiting) = busy.get_mut(&fleet) {
                    waiting.push_back((fleet, granted));
                } else {
                    busy.insert(fleet.clone(), VecDeque::new());
                    grant(&own, (fleet, granted));
                }
            }
            Request::Release(fleet) => match busy.get_mut(&fleet).and_then(VecDeque::pop_front) {
                Some(next) => grant(&own, next),
                None => drop(busy.remove(&fleet)),
            },
        }
    }
}

/// Hands a claim its turn. A claimer that already gave up drops the turn
/// unread, and its drop frees the fleet for the next.
fn grant(own: &mpsc::WeakUnboundedSender<Request>, (fleet, granted): Claim) {
    if let Some(requests) = own.upgrade() {
        drop(granted.send(Turn {
            fleet: Some(fleet),
            requests,
        }));
    }
}

#[cfg(test)]
#[path = "turns/tests.rs"]
mod tests;
