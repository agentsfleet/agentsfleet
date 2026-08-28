//! The seam the event-history routes act through.
//!
//! One trait over both listings and the single read, because they are one
//! store and a suite that stubbed them apart would be stubbing an
//! implementation detail.
//!
//! # Why the workspace listing takes an optional fleet
//!
//! The console's Live Wall drills from a workspace into one fleet without
//! changing endpoint, and the per-fleet route answers the same question with
//! the fleet fixed. Both bind the same argument to one statement, so the two
//! cannot disagree about a fleet's history — which two statements would
//! eventually do.

use afd_core::id::Uuid7;
use afd_events::{Cursor, EventRow, Filter, History, Result as EventResult};

/// Everything the event-history routes act through.
pub trait WorkspaceEvents: Send + Sync + std::fmt::Debug + 'static {
    /// One page of a workspace's history, newest first.
    ///
    /// `fleet` narrows the page to one fleet without changing the statement.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a row this build cannot
    /// read. The cursor and the window are resolved by the handler, so nothing
    /// here is the caller's fault.
    fn page_for_workspace(
        &self,
        workspace: &Uuid7,
        fleet: Option<&Uuid7>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventRow>>> + Send;

    /// One page of a single fleet's history, newest first.
    ///
    /// # Errors
    /// As [`Self::page_for_workspace`].
    fn page_for_fleet(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventRow>>> + Send;

    /// One event, inside this workspace and fleet.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. An event belonging to
    /// another workspace is `Ok(None)`, indistinguishable from one that never
    /// existed — the scope is an authorization, not a filter.
    fn one(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        event_id: &str,
    ) -> impl Future<Output = EventResult<Option<EventRow>>> + Send;
}

/// The production reader answers all three directly.
impl WorkspaceEvents for History {
    fn page_for_workspace(
        &self,
        workspace: &Uuid7,
        fleet: Option<&Uuid7>,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventRow>>> + Send {
        Self::page_for_workspace(self, workspace, fleet, filter, cursor, limit)
    }

    fn page_for_fleet(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        filter: &Filter,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventRow>>> + Send {
        Self::page_for_fleet(self, workspace, fleet, filter, cursor, limit)
    }

    fn one(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        event_id: &str,
    ) -> impl Future<Output = EventResult<Option<EventRow>>> + Send {
        Self::detail(self, workspace, fleet, event_id)
    }
}
