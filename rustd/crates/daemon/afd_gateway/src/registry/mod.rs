//! Connection-scoped registration; an old owner's drop cannot remove its replacement.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_core::id::Uuid7;
use dashmap::DashMap;

use crate::{Result, error::ErrorKind, session::SessionHandle};

#[derive(Debug, Default)]
pub(crate) struct Registry {
    sessions: DashMap<Uuid7, Arc<SessionHandle>>,
    draining: AtomicBool,
}

impl Registry {
    /// The host lock orders local insertion with every durable ownership transition.
    pub(crate) async fn register_current(
        self: &Arc<Self>,
        session: &Arc<SessionHandle>,
        authority: &afd_runner::capacity::authority::Authority,
        database: &afd_db::Db,
    ) -> Result<Registration> {
        let mut tx = sqlx::Transaction::begin(database.acquire().await?, None).await?;
        if afd_runner::capacity::lock(&mut tx, (&session.runner, authority))
            .await?
            .is_none()
        {
            return Err(ErrorKind::Authority {
                refusal: afd_runner::capacity::authority::Refusal::Superseded,
            }
            .into());
        }
        let registration = self.register(session)?;
        tx.commit().await?;
        Ok(registration)
    }

    fn register(self: &Arc<Self>, session: &Arc<SessionHandle>) -> Result<Registration> {
        if self.draining.load(Ordering::Acquire) {
            return Err(ErrorKind::Closed.into());
        }
        let runner = session.runner.clone();
        let connection = session.connection.clone();
        if let Some(old) = self.sessions.insert(runner.clone(), Arc::clone(session)) {
            old.supersede();
        }
        if self.draining.load(Ordering::Acquire) {
            session.drain.cancel();
        }
        Ok(Registration {
            registry: Arc::clone(self),
            runner,
            connection,
        })
    }

    pub(crate) fn candidates(
        &self,
        fleets: &std::collections::HashSet<Uuid7>,
    ) -> Vec<Arc<SessionHandle>> {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|entry| {
                let session = Arc::clone(entry.value());
                let state = session.snapshot.lock();
                if !state.ready
                    || state.degraded
                    || state.credits == 0
                    || session.stop.is_cancelled()
                {
                    return None;
                }
                let holds = state.held.iter().any(|fleet| fleets.contains(fleet));
                let key = (!holds, state.last_offer);
                drop(state);
                Some((key, session))
            })
            .collect();
        // Capture each key once: concurrent Ready updates must not change the sort order mid-sort.
        sessions.sort_by_key(|(key, _)| *key);
        sessions.into_iter().map(|(_, session)| session).collect()
    }

    pub(crate) fn begin_drain(&self) {
        self.draining.store(true, Ordering::Release);
        for session in &self.sessions {
            session.drain.cancel();
        }
    }

    pub(crate) fn cancel_all(&self) {
        for session in &self.sessions {
            session.stop.cancel();
        }
    }

    pub(crate) fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }
}

pub(crate) struct Registration {
    registry: Arc<Registry>,
    runner: Uuid7,
    connection: Uuid7,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.registry
            .sessions
            .remove_if(&self.runner, |_, current| {
                current.connection == self.connection
            });
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests;
