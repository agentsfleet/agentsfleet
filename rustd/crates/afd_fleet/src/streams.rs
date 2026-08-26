//! Instance-local registry behind the operator stream overview.
//!
//! The registry owns identities and start times only. The server that owns a
//! live socket keeps that socket outside this type, so an operator response
//! cannot accidentally expose a descriptor or transport handle.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::admin::{FleetStreamItem, FleetStreamsResponse};

/// A cloneable handle to this daemon instance's live stream metadata.
#[derive(Debug, Clone)]
pub struct LiveStreams(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    max_streams: u32,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    next_id: u64,
    entries: BTreeMap<u64, Entry>,
}

#[derive(Debug, Clone)]
struct Entry {
    workspace_id: Uuid7,
    fleet_id: Uuid7,
    started_at: UnixMillis,
}

/// Ownership of one claimed stream slot.
///
/// Dropping the guard removes its row from the operator overview.
#[derive(Debug)]
pub struct StreamRegistration {
    id: u64,
    streams: LiveStreams,
}

impl LiveStreams {
    /// Creates an empty registry with the instance admission ceiling.
    #[must_use]
    pub fn new(max_streams: u32) -> Self {
        Self(Arc::new(Inner {
            max_streams,
            state: Mutex::new(State {
                next_id: 1,
                entries: BTreeMap::new(),
            }),
        }))
    }

    /// Claims one stream slot atomically, or returns `None` at capacity.
    #[must_use]
    pub fn try_register(
        &self,
        workspace_id: &Uuid7,
        fleet_id: &Uuid7,
        started_at: UnixMillis,
    ) -> Option<StreamRegistration> {
        let mut state = self.state();
        if state.entries.len() >= self.0.max_streams as usize {
            return None;
        }
        let id = state.claim_id();
        state.entries.insert(
            id,
            Entry {
                workspace_id: workspace_id.clone(),
                fleet_id: fleet_id.clone(),
                started_at,
            },
        );
        Some(StreamRegistration {
            id,
            streams: self.clone(),
        })
    }

    /// Returns an owned, transport-free snapshot for the operator endpoint.
    #[must_use]
    pub fn overview(&self) -> FleetStreamsResponse<'static> {
        let state = self.state();
        let items = state
            .entries
            .values()
            .map(|entry| FleetStreamItem {
                workspace_id: Cow::Owned(entry.workspace_id.to_string()),
                fleet_id: Cow::Owned(entry.fleet_id.to_string()),
                started_ms: entry.started_at.as_millis(),
            })
            .collect();
        FleetStreamsResponse {
            total: state.entries.len(),
            items,
            max_streams: self.0.max_streams,
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl State {
    fn claim_id(&mut self) -> u64 {
        loop {
            let candidate = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.entries.contains_key(&candidate) {
                return candidate;
            }
        }
    }
}

impl Drop for StreamRegistration {
    fn drop(&mut self) {
        self.streams.state().entries.remove(&self.id);
    }
}

#[cfg(test)]
mod tests;
