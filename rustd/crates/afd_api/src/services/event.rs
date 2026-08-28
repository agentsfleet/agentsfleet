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
use afd_events::{Cursor, EventDetailRow, EventRow, Filter, History, Result as EventResult, Steer};

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

    /// One page of a fleet's chat thread, newest first, bodies included.
    ///
    /// The caller asks for one row more than it will serve — see
    /// [`afd_events::History::thread_page`] on why has-more is a fact here and
    /// not a guess.
    ///
    /// # Errors
    /// As [`Self::page_for_fleet`].
    fn thread_for_fleet(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventDetailRow>>> + Send;

    /// One event, inside this workspace and fleet, bodies included.
    ///
    /// [`EventDetailRow`] rather than the listing's row, because this is the
    /// only read that carries `request_json` and `response_text`. A page of up
    /// to two hundred rows would pay for both on every one of them; an expanded
    /// row is asked for one.
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
    ) -> impl Future<Output = EventResult<Option<EventDetailRow>>> + Send;
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

    fn thread_for_fleet(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> impl Future<Output = EventResult<Vec<EventDetailRow>>> + Send {
        Self::thread_page(self, workspace, fleet, cursor, limit)
    }

    fn one(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        event_id: &str,
    ) -> impl Future<Output = EventResult<Option<EventDetailRow>>> + Send {
        Self::detail(self, workspace, fleet, event_id)
    }
}

/// What the steer verb acts through.
///
/// Its own trait rather than a method on [`WorkspaceEvents`], because it is
/// its own store: the reads hold a Postgres pool, and this holds a Redis
/// connection opened by CONNECTING — which is exactly the seam a suite proving
/// the refusal matrix must not have to construct.
pub trait FleetSteering: Send + Sync + std::fmt::Debug + 'static {
    /// Puts one message on the fleet's stream, answering with its event id.
    ///
    /// # Errors
    /// Reports a queue that would not take the append.
    fn append(
        &self,
        fleet: &str,
        workspace: &str,
        actor: &str,
        request_json: &str,
    ) -> impl Future<Output = EventResult<String>> + Send;
}

/// The production ingress answers directly.
impl FleetSteering for Steer {
    fn append(
        &self,
        fleet: &str,
        workspace: &str,
        actor: &str,
        request_json: &str,
    ) -> impl Future<Output = EventResult<String>> + Send {
        Self::append(self, fleet, workspace, actor, request_json)
    }
}
