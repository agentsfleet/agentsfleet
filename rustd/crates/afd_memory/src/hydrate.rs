//! What a run is seeded with, and what a recall past that window finds.
//!
//! The fleet's own entries spend [`HYDRATE_WINDOW_BYTES`] under the window's
//! rule; a granted reader's shared entries then spend [`HYDRATE_SHARED_BYTES`],
//! newest first, each naming the fleet that wrote it.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::memory::{
    HYDRATE_SHARED_BYTES, HYDRATE_WINDOW_BYTES, MemoryHydrateResponse, MemoryRecallResponse,
    SharedMemory,
};

use crate::access::Access;
use crate::error::Result;
use crate::memories::Memories;
use crate::record::{Owner, Record};
use crate::window::{self, Window};

/// A run was seeded with a memory window.
const EVENT_HYDRATED: &str = "memory_hydrated";

impl Memories {
    /// The window that seeds one run of `fleet`.
    ///
    /// # Errors
    /// Refuses a fleet that is gone and reports a store that would not answer.
    pub async fn hydrate(&self, fleet: &Uuid7) -> Result<MemoryHydrateResponse<'static>> {
        let Access { workspace, grants } = self.access(fleet).await?;
        let owner = Owner {
            workspace: &workspace,
            fleet,
        };
        let records = self
            .routes()
            .of(&workspace)
            .store
            .window(owner, grants.read)
            .await?;
        let (own, others): (Vec<Record>, Vec<Record>) = records
            .into_iter()
            .partition(|record| record.written_by(fleet));
        let held = own.len();
        let Window { kept, dropped } = window::select(
            own.into_iter().map(Record::into_delta).collect(),
            HYDRATE_WINDOW_BYTES,
        );
        let mut shared = self.named(&workspace, others).await?;
        shared.truncate(window::prefix_within(
            shared.iter().map(SharedMemory::bytes),
            HYDRATE_SHARED_BYTES,
        ));

        // Hoisted for the `log` bridge's duplicated field expressions. The
        // content is never logged — only the tallies and the scope.
        let fleet_id = fleet.as_str();
        let (hydrated, dropped_count) = (kept.len(), dropped.len());
        let dropped_bytes = window::total_bytes(&dropped);
        let shared_count = shared.len();
        tracing::debug!(
            fleet_id,
            held,
            hydrated,
            dropped = dropped_count,
            dropped_bytes,
            shared = shared_count,
            event = EVENT_HYDRATED,
            "a run was seeded with its memory window; the rest stays durable"
        );
        Ok(MemoryHydrateResponse {
            memory: kept,
            shared,
            publish: grants.publish,
        })
    }

    /// Entries of `fleet`'s holding `query`, and — for a granted reader — the
    /// workspace's shared ones, key matches first, at most `limit` of each.
    ///
    /// # Errors
    /// Refuses a fleet that is gone and reports a store that would not answer.
    pub async fn recall(
        &self,
        fleet: &Uuid7,
        query: &str,
        limit: usize,
    ) -> Result<MemoryRecallResponse<'static>> {
        let Access { workspace, grants } = self.access(fleet).await?;
        let owner = Owner {
            workspace: &workspace,
            fleet,
        };
        let records = self
            .routes()
            .of(&workspace)
            .store
            .search(owner, grants.read, query, limit)
            .await?;
        let (own, others): (Vec<Record>, Vec<Record>) = records
            .into_iter()
            .partition(|record| record.written_by(fleet));
        Ok(MemoryRecallResponse {
            memory: own.into_iter().map(Record::into_delta).collect(),
            shared: self.named(&workspace, others).await?,
        })
    }

    /// `others` as shared entries, each naming its writer; no read of the
    /// workspace's fleets when there is nothing to name.
    async fn named(
        &self,
        workspace: &Uuid7,
        others: Vec<Record>,
    ) -> Result<Vec<SharedMemory<'static>>> {
        if others.is_empty() {
            return Ok(Vec::new());
        }
        let names = self.directory().names(workspace).await?;
        Ok(others
            .into_iter()
            .map(|record| SharedMemory {
                writer_fleet_name: Cow::Owned(
                    names
                        .get(record.fleet.as_str())
                        .cloned()
                        .unwrap_or_default(),
                ),
                writer_fleet_id: Cow::Owned(record.fleet.as_str().to_owned()),
                updated_at: record.updated_at_ms,
                key: Cow::Owned(record.key),
                content: Cow::Owned(record.content),
                category: Cow::Owned(record.category),
            })
            .collect())
    }
}
