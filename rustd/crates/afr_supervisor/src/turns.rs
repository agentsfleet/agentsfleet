//! One run per fleet at a time, however many workers there are.
//!
//! A coordinator task owns the table of which fleets are busy and who waits
//! for each; workers talk to it over a channel, so nothing is shared behind a
//! lock. The coordinator hands out a [`Turn`] by value. Dropping it — run
//! finished, run cancelled, or the grant never read because its claimer gave
//! up — tells the coordinator the fleet is free, which is what keeps a fleet
//! from staying busy forever on a lost grant.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};

use afd_core::id::Uuid7;
use tokio::sync::{mpsc, oneshot};

/// What a worker asks of the coordinator.
#[derive(Debug)]
enum Request {
    /// Wait for `fleet` to be free, then hold it.
    Claim(Uuid7, oneshot::Sender<Turn>),
    /// `fleet` is free again.
    Release(Uuid7),
}

/// Asks for turns.
#[derive(Debug, Clone)]
pub struct FleetTurns {
    requests: mpsc::UnboundedSender<Request>,
}

/// One fleet, held; dropping it frees the fleet.
#[derive(Debug)]
pub struct Turn {
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
    pub fn start() -> (Self, impl Future<Output = ()> + Send + 'static) {
        let (requests, receiver) = mpsc::unbounded_channel();
        let coordinator = coordinate(receiver, requests.downgrade());
        (Self { requests }, coordinator)
    }

    /// Waits until `fleet` is free, and holds it until the turn is dropped.
    ///
    /// `None` only when the coordinator has already stopped.
    pub async fn claim(&self, fleet: &Uuid7) -> Option<Turn> {
        let (granted, grant) = oneshot::channel();
        self.requests
            .send(Request::Claim(fleet.clone(), granted))
            .ok()?;
        grant.await.ok()
    }
}

/// Grants turns until every handle and turn is gone.
async fn coordinate(
    mut requests: mpsc::UnboundedReceiver<Request>,
    own: mpsc::WeakUnboundedSender<Request>,
) {
    let mut busy: HashMap<Uuid7, VecDeque<oneshot::Sender<Turn>>> = HashMap::new();
    while let Some(request) = requests.recv().await {
        match request {
            Request::Claim(fleet, granted) => match busy.entry(fleet) {
                Entry::Occupied(mut waiting) => waiting.get_mut().push_back(granted),
                Entry::Vacant(free) => {
                    let fleet = free.key().clone();
                    free.insert(VecDeque::new());
                    grant(&own, fleet, granted);
                }
            },
            Request::Release(fleet) => match busy.get_mut(&fleet).and_then(VecDeque::pop_front) {
                Some(next) => grant(&own, fleet, next),
                None => drop(busy.remove(&fleet)),
            },
        }
    }
}

/// Hands `fleet` to a claimer. A claimer that already gave up drops the turn
/// unread, and its drop frees the fleet for the next.
fn grant(own: &mpsc::WeakUnboundedSender<Request>, fleet: Uuid7, granted: oneshot::Sender<Turn>) {
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
