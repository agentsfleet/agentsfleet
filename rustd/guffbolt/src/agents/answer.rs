//! What the tools that start and steer children answer, as the model reads
//! it: small JSON shapes the embedder encodes, and the refusals in the
//! harness's own words.

use serde::Serialize;

use super::{Agents, Kind, Refused, Status};
use crate::tool::{ErrorCode, ToolOutput};

/// What a child asking for a tool its parent lacks reads, after its name.
const NOT_HELD: &str = "is not one of this run's tools";
/// What a child past the run's caps reads.
const CAP_REACHED: &str =
    "as many children run, or were started, as one run may; wait for one to end";
/// What a delegated child that was interrupted reads back as.
const INTERRUPTED: &str = "was interrupted before it answered";
/// What a delegated child whose end never arrived reads back as; the
/// registry keeps every child, so none is.
const LOST: &str = "ended without a status";

/// What a call that starts a child alongside its parent answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Spawned {
    /// The id the child took, which every later call about it names.
    pub child_id: u64,
}

/// Where a child is, as a wait or an interrupt answers: its answer once
/// done, its detail once failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct State<'a> {
    status: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

impl<'a> State<'a> {
    /// `status`, with what it carries.
    #[must_use]
    pub const fn of(status: &'a Status) -> Self {
        let (answer, detail) = match status {
            Status::Done(answer) => (Some(answer.as_str()), None),
            Status::Failed(detail) => (None, Some(detail.as_str())),
            Status::Running | Status::Interrupted => (None, None),
        };
        Self {
            status: status.kind(),
            answer,
            detail,
        }
    }

    /// A status alone.
    #[must_use]
    pub const fn named(status: Kind) -> Self {
        Self {
            status,
            answer: None,
            detail: None,
        }
    }
}

/// Whether a message for a child was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Accepted {
    /// Whether the child reads it on its next step; one that ended never
    /// does.
    pub accepted: bool,
}

/// One child, as a list of the run's children reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Listed {
    /// Its id.
    pub child_id: u64,
    /// Where it is.
    pub status: Kind,
    /// How many loops it is inside.
    pub depth: u8,
    /// How many calls it made that answered.
    pub calls: u64,
}

/// What a call naming a child the run does not have reads.
#[must_use]
pub fn not_found(id: u64) -> ToolOutput {
    ToolOutput::failed(
        ErrorCode::CHILD_NOT_FOUND,
        &format!("no child of this run has id {id}"),
    )
}

/// A refused child, as the call that asked for it reads it.
impl From<Refused> for ToolOutput {
    fn from(refused: Refused) -> Self {
        let code = refused.code();
        match refused {
            Refused::NotHeld(name) => Self::failed(code, &format!("{name} {NOT_HELD}")),
            Refused::Capped => Self::failed(code, CAP_REACHED),
        }
    }
}

impl<S> Agents<S> {
    /// What a call that handed child `id` its task reads once the child
    /// ends: its answer, or why there is none. It waits as long as the
    /// caller does.
    pub async fn delegated(&self, id: u64) -> ToolOutput {
        match self.wait(id, None).await {
            Some(Status::Done(answer)) => ToolOutput::succeeded(answer),
            Some(Status::Failed(detail)) => ToolOutput::failed(ErrorCode::CHILD_FAILED, &detail),
            Some(Status::Interrupted) => {
                ToolOutput::failed(ErrorCode::INTERRUPTED, &format!("child {id} {INTERRUPTED}"))
            }
            Some(Status::Running) | None => {
                ToolOutput::failed(ErrorCode::CHILD_FAILED, &format!("child {id} {LOST}"))
            }
        }
    }
}
