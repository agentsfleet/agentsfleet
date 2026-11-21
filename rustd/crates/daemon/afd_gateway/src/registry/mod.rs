//! Connection-scoped registration; an old owner's drop cannot remove its replacement.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_core::id::Uuid7;
use dashmap::DashMap;

use crate::{Result, connection::ConnectionHandle, error::ErrorKind};

#[derive(Debug, Default)]
pub(crate) struct Registry {
    connections: DashMap<Uuid7, Arc<ConnectionHandle>>,
    draining: AtomicBool,
}

impl Registry {
    /// The host lock orders local insertion with every durable ownership transition.
    pub(crate) async fn register_current(
        self: &Arc<Self>,
        connection: &Arc<ConnectionHandle>,
        authority: &afd_runner::capacity::authority::Authority,
        database: &afd_db::Db,
    ) -> Result<Registration> {
        let mut tx = sqlx::Transaction::begin(database.acquire().await?, None).await?;
        if afd_runner::capacity::lock_host(&mut tx, (&connection.runner, authority), None)
            .await?
            .is_none()
        {
            return Err(ErrorKind::Authority {
                refusal: afd_runner::capacity::authority::Refusal::Superseded,
            }
            .into());
        }
        let registration = self.register(connection)?;
        tx.commit().await?;
        Ok(registration)
    }

    fn register(self: &Arc<Self>, connection: &Arc<ConnectionHandle>) -> Result<Registration> {
        if self.draining.load(Ordering::Acquire) {
            return Err(ErrorKind::Closed.into());
        }
        let runner = connection.runner.clone();
        let connection_id = connection.connection_id.clone();
        if let Some(old) = self
            .connections
            .insert(runner.clone(), Arc::clone(connection))
        {
            old.supersede();
        }
        if self.draining.load(Ordering::Acquire) {
            connection.drain.cancel();
        }
        Ok(Registration {
            registry: Arc::clone(self),
            runner,
            connection_id,
        })
    }

    /// Busy hosts retain the request until returned capacity completes a recovery scan.
    pub(crate) fn request_recovery(&self, recovery: afd_bus::wake::Recovery) {
        for connection in &self.connections {
            connection.snapshot.lock().recovery_pending = Some(recovery);
        }
    }

    /// Every connected runner, for the probe that names the idle tenant ones.
    pub(crate) fn runners(&self) -> Vec<Uuid7> {
        self.connections
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Connections a pass may ask to assign, skipping the `idle` runners.
    pub(crate) fn candidates(
        &self,
        fleets: &std::collections::HashSet<Uuid7>,
        idle: &std::collections::HashSet<Uuid7>,
    ) -> Vec<Arc<ConnectionHandle>> {
        let mut connections: Vec<_> = self
            .connections
            .iter()
            .filter_map(|entry| {
                let connection = Arc::clone(entry.value());
                let state = connection.snapshot.lock();
                if !state.ready
                    || state.degraded
                    || state.credits == 0
                    || connection.stop.is_cancelled()
                    || connection.drain.is_cancelled()
                    || connection.is_superseded()
                    || idle.contains(&connection.runner)
                {
                    return None;
                }
                let holds = state.held.iter().any(|fleet| fleets.contains(fleet));
                let key = (!holds, state.last_assignment);
                drop(state);
                Some((key, connection))
            })
            .collect();
        // Capture each key once: concurrent Ready updates must not change the sort order mid-sort.
        connections.sort_by_key(|(key, _)| *key);
        connections
            .into_iter()
            .map(|(_, connection)| connection)
            .collect()
    }

    pub(crate) fn begin_drain(&self) {
        self.draining.store(true, Ordering::Release);
        for connection in &self.connections {
            connection.drain.cancel();
        }
    }

    pub(crate) fn cancel_all(&self) {
        for connection in &self.connections {
            connection.stop.cancel();
        }
    }

    pub(crate) fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn register_fixture(
        self: &Arc<Self>,
        connection: &Arc<ConnectionHandle>,
    ) -> Result<Registration> {
        self.register(connection)
    }
}

pub(crate) struct Registration {
    registry: Arc<Registry>,
    runner: Uuid7,
    connection_id: Uuid7,
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.registry
            .connections
            .remove_if(&self.runner, |_, current| {
                current.connection_id == self.connection_id
            });
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests;
