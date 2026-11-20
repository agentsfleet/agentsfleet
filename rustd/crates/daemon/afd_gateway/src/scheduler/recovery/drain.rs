//! Give each credited connection one claim at a time and bound concurrent claims.

use crate::{
    Gateway, Operation, Result,
    connection::{AssignMode, AssignOutcome, ConnectionHandle},
};
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use std::{collections::VecDeque, sync::Arc};
use tokio_util::sync::CancellationToken;

const EXTRA_PAGES_PER_HOST: usize = 4;

struct Host {
    connection: Arc<ConnectionHandle>,
    credits: usize,
    extra_pages: usize,
}

impl Host {
    fn new(connection: Arc<ConnectionHandle>) -> Self {
        let credits = connection.snapshot.lock().credits;
        Self {
            connection,
            credits,
            extra_pages: EXTRA_PAGES_PER_HOST,
        }
    }

    async fn claim(self) -> (Self, Result<AssignOutcome>) {
        let outcome = self.connection.claim(AssignMode::Recovery).await;
        (self, outcome)
    }

    fn ready(&self) -> bool {
        self.credits > 0 && self.connection.snapshot.lock().credits > 0
    }

    fn advance(&mut self, outcome: Result<AssignOutcome>) -> bool {
        match outcome {
            Ok(AssignOutcome::Issued) => self.credits -= 1,
            Ok(AssignOutcome::Deferred) if self.extra_pages > 0 => self.extra_pages -= 1,
            Ok(AssignOutcome::Empty | AssignOutcome::Deferred) => return false,
            Err(failure) => {
                // The connection may have committed a run before its reply was lost.
                Gateway::observe(&failure, Operation::Assign);
                return false;
            }
        }
        self.ready()
    }
}

pub(super) async fn drain(
    connections: Vec<Arc<ConnectionHandle>>,
    claims: usize,
    stop: &CancellationToken,
) {
    let mut queue: VecDeque<_> = connections.into_iter().map(Host::new).collect();
    let mut running = FuturesUnordered::new();
    loop {
        while running.len() < claims {
            let Some(host) = queue.pop_front() else { break };
            if host.ready() {
                running.push(host.claim());
            }
        }
        let next = tokio::select! {
            biased;
            () = stop.cancelled() => return,
            next = running.next() => next,
        };
        let Some((mut host, outcome)) = next else {
            return;
        };
        if host.advance(outcome) {
            queue.push_back(host);
        }
    }
}
