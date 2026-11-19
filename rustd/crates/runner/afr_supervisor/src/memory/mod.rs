//! A fleet's durable memory: read at run start, searched past the window
//! when a recall falls short, and written back fenced.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::memory::{
    MemoryDelta, MemoryPushRequest, MemoryRecallRequest, MemoryRecallResponse, SharedMemory,
};
use af_wire::run::RunPayload;
use afr_agent::Checkpoint;
use afr_telemetry::record;

use zeroize::Zeroize;

use crate::client::{Body, ControlPlane};
use crate::error::Result;

/// Reads the fleet's memory, retrying a blip.
///
/// # Errors
/// A refusal, or a retryable failure that outlasted its attempts.
pub(crate) async fn hydrate(plane: &ControlPlane, fleet_id: &Uuid7) -> Result<Body> {
    plane.retrying(|| plane.hydrate(fleet_id)).await
}

/// Counts a memory push that did not land, by why.
fn counted(failure: &crate::Error) {
    record::push_failed(failure.push_failure());
}

/// `memory`, fenced by `run`'s token.
fn push_request<'a>(
    run: &'a RunPayload<'_>,
    memory: Vec<MemoryDelta<'a>>,
) -> MemoryPushRequest<'a> {
    MemoryPushRequest {
        run_id: Cow::Borrowed(&run.run_id),
        fencing_token: run.fencing_token,
        memory,
    }
}

/// What a run learned, held from the run's end until the settle hands it on,
/// and zeroed when dropped.
///
/// The entries are the fleet's memory in clear. Every string the run owns is
/// overwritten before its allocation is freed, in the derive-less shape
/// `af_crypto`'s `zero_on_drop!` uses; a `'static` borrow is no run's data.
#[derive(Default)]
pub(crate) struct CapturedMemory(Vec<MemoryDelta<'static>>);

/// The count of entries and nothing of what they say: the entries are the
/// fleet's memory in clear.
impl std::fmt::Debug for CapturedMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedMemory")
            .field("entries", &self.0.len())
            .finish()
    }
}

impl CapturedMemory {
    /// Borrowed views of the entries, for one request body.
    pub(crate) fn views(&self) -> Vec<MemoryDelta<'_>> {
        self.0.iter().map(MemoryDelta::view).collect()
    }
}

impl From<Vec<MemoryDelta<'static>>> for CapturedMemory {
    fn from(deltas: Vec<MemoryDelta<'static>>) -> Self {
        Self(deltas)
    }
}

impl CapturedMemory {
    /// Overwrites every string the run owns in place and leaves each entry
    /// empty, before any allocation is freed.
    fn scrub(&mut self) {
        for delta in &mut self.0 {
            for text in [&mut delta.key, &mut delta.content, &mut delta.category] {
                match text {
                    // Zeroed in place: the allocation stays, emptied, until
                    // the clear frees it.
                    Cow::Owned(owned) => owned.zeroize(),
                    Cow::Borrowed(_) => *text = Cow::Borrowed(""),
                }
            }
        }
    }
}

impl Zeroize for CapturedMemory {
    fn zeroize(&mut self) {
        self.scrub();
        self.0.clear();
    }
}

impl Drop for CapturedMemory {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Writes one run's memory back mid-run, through the same fenced push.
#[derive(Debug)]
pub(crate) struct RunCheckpoint<'a> {
    plane: &'a ControlPlane,
    fleet_id: &'a Uuid7,
    run: &'a RunPayload<'a>,
}

impl<'a> RunCheckpoint<'a> {
    /// The checkpoint of `run`, for `fleet_id`, pushing through `plane`.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        fleet_id: &'a Uuid7,
        run: &'a RunPayload<'a>,
    ) -> Self {
        Self {
            plane,
            fleet_id,
            run,
        }
    }
}

#[async_trait::async_trait]
impl Checkpoint for RunCheckpoint<'_> {
    /// One attempt, no retry: the push before the report retries, and carries
    /// every entry again.
    async fn push(&self, memory: Vec<MemoryDelta<'static>>) -> afr_agent::Result<()> {
        let request = push_request(self.run, memory);
        (self.plane.capture(self.fleet_id, &request).await)
            .inspect_err(counted)
            .map_err(afr_agent::Error::checkpoint)
    }
}

/// Asks the daemon for memory a run's window missed, fenced by the run's
/// token. Never retried: a recall that gets no answer answers from the
/// window, and a retry would only make the model wait longer for the same.
#[derive(Debug)]
pub(crate) struct Recaller<'a> {
    plane: &'a ControlPlane,
    fleet_id: &'a Uuid7,
    run: &'a RunPayload<'a>,
}

impl<'a> Recaller<'a> {
    /// Recalls for `fleet_id` under `run`.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        fleet_id: &'a Uuid7,
        run: &'a RunPayload<'a>,
    ) -> Self {
        Self {
            plane,
            fleet_id,
            run,
        }
    }
}

#[async_trait::async_trait]
impl afr_memory::Recall for Recaller<'_> {
    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> afr_memory::Result<MemoryRecallResponse<'static>> {
        let request = MemoryRecallRequest {
            run_id: Cow::Borrowed(&self.run.run_id),
            fencing_token: self.run.fencing_token,
            query: Cow::Borrowed(query),
            limit,
        };
        let unanswered =
            |failure: crate::error::Error| afr_memory::Error::unanswered(failure.code());
        let body = self
            .plane
            .recall(self.fleet_id, &request)
            .await
            .map_err(unanswered)?;
        let found = body
            .decode::<MemoryRecallResponse<'_>>()
            .map_err(unanswered)?;
        Ok(MemoryRecallResponse {
            memory: found
                .memory
                .into_iter()
                .map(MemoryDelta::into_owned)
                .collect(),
            shared: found
                .shared
                .into_iter()
                .map(SharedMemory::into_owned)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
