//! The one trait every memory read and write in `agentsfleetd` goes through.

use std::fmt;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::MemoryDelta;

use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};

/// Where a workspace's memory lives.
///
/// A store holds rows and answers for them; it decides nothing about who may
/// read or publish. The grants are read from `core.fleets` by [`crate::Memories`]
/// and arrive here as `reads`, so a vendor store enforces a grant by honouring
/// one flag in its queries rather than by reaching into the control plane.
///
/// `dyn`, because the store is chosen per workspace at run time and a flip
/// swaps it while the process runs.
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync + fmt::Debug {
    /// The store's name, for the flip's log lines.
    fn name(&self) -> &'static str;

    /// Every entry `owner.fleet` holds, newest first; then, when `reads`, the
    /// workspace-visible entries other fleets in `owner.workspace` hold, newest
    /// first and bounded by the store.
    ///
    /// # Errors
    /// The store cannot answer.
    async fn window(&self, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>>;

    /// Writes each entry under `owner.fleet` at `now`, replacing what its key
    /// held, then sweeps expired scratch notes and evicts past the cap.
    ///
    /// # Errors
    /// The store cannot take the write; none of it lands.
    async fn upsert(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Housekept>;

    /// Entries whose key or content holds `query`, ignoring case: the fleet's
    /// own and, when `reads`, other fleets' shared ones; key matches first,
    /// then newest first, at most `limit` of each.
    ///
    /// # Errors
    /// The store cannot answer.
    async fn search(
        &self,
        owner: Owner<'_>,
        reads: bool,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Record>>;

    /// One operator page under `view`, newest-created first, holding other
    /// fleets' shared entries too when `reads`.
    ///
    /// # Errors
    /// The store cannot answer.
    async fn page(
        &self,
        owner: Owner<'_>,
        reads: bool,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>>;

    /// Removes `owner.fleet`'s entry under `key`, answering whether it held one.
    /// Another fleet's entry under the same key is never touched.
    ///
    /// # Errors
    /// The store cannot answer.
    async fn forget(&self, owner: Owner<'_>, key: &str) -> Result<bool>;

    /// Every entry of every fleet in `workspace`, for a flip's copy.
    ///
    /// # Errors
    /// The store cannot answer.
    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>>;

    /// Writes `record` as it stands — writer, visibility and both instants —
    /// unless this store holds a newer row under the same writer and key.
    ///
    /// # Errors
    /// The store cannot take the write.
    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()>;
}
