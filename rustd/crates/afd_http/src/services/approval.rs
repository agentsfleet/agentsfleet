//! The HTTP seam the approval inbox acts through.
//!
//! One trait over the queue read, the single read and the decision, because
//! they are one store and a suite that stubbed them apart would be stubbing an
//! implementation detail.
//!
//! # The decision takes an already-terminal status
//!
//! [`WorkspaceApprovals::resolve`] takes [`Decision`], which cannot express `pending`.
//! The handler maps the path's `approve` / `deny` segment onto the two an
//! operator can write, so there is no third spelling for a caller to invent.

use afd_approval::{
    Cursor, Decision, Filter, GateRow, Inbox, Resolution, Result as ApprovalResult,
};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

/// Everything the approval routes act through.
pub trait WorkspaceApprovals: Send + Sync + std::fmt::Debug + 'static {
    /// One page of a workspace's gates, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn page(
        &self,
        workspace: &Uuid7,
        filter: Filter<'_>,
        cursor: Option<Cursor<'_>>,
        limit: i64,
    ) -> impl Future<Output = ApprovalResult<Vec<GateRow>>> + Send;

    /// One gate, inside this workspace.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A gate belonging to another
    /// workspace is `Ok(None)`, indistinguishable from one that never existed.
    fn one(
        &self,
        workspace: &Uuid7,
        gate: &Uuid7,
    ) -> impl Future<Output = ApprovalResult<Option<GateRow>>> + Send;

    /// Answers one gate atomically.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and refuses a non-terminal
    /// outcome.
    fn resolve(
        &self,
        action: &str,
        outcome: Decision,
        by: &str,
        detail: &str,
        fleet: Option<&str>,
        now: UnixMillis,
    ) -> impl Future<Output = ApprovalResult<Resolution>> + Send;
}

/// The production queue answers all three directly.
impl WorkspaceApprovals for Inbox {
    fn page(
        &self,
        workspace: &Uuid7,
        filter: Filter<'_>,
        cursor: Option<Cursor<'_>>,
        limit: i64,
    ) -> impl Future<Output = ApprovalResult<Vec<GateRow>>> + Send {
        Self::page(self, workspace, filter, cursor, limit)
    }

    fn one(
        &self,
        workspace: &Uuid7,
        gate: &Uuid7,
    ) -> impl Future<Output = ApprovalResult<Option<GateRow>>> + Send {
        Self::one(self, workspace, gate)
    }

    fn resolve(
        &self,
        action: &str,
        outcome: Decision,
        by: &str,
        detail: &str,
        fleet: Option<&str>,
        now: UnixMillis,
    ) -> impl Future<Output = ApprovalResult<Resolution>> + Send {
        Self::resolve(self, action, outcome, by, detail, fleet, now)
    }
}
