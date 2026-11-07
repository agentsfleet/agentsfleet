//! What the nested tools answer, as the model reads it.

use std::fmt::Debug;

use afd_core::error_code;
use afr_tools::{ToolErrorCode, ToolOutput};
use serde::Serialize;

use super::registry::{Kind, Status};

pub(super) const EVENT_ANSWER_ENCODE_FAILED: &str = "nested_answer_encode_failed";

/// What `spawn` answers.
#[derive(Debug, Serialize)]
pub(super) struct Spawned {
    pub(super) child_id: u64,
}

/// What `wait_agent` and `interrupt_agent` answer: where the child is, its
/// answer once done, its detail once failed.
#[derive(Debug, Serialize)]
pub(super) struct State<'a> {
    status: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

impl<'a> State<'a> {
    /// `status`, with what it carries.
    pub(super) const fn of(status: &'a Status) -> Self {
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
    pub(super) const fn named(status: Kind) -> Self {
        Self {
            status,
            answer: None,
            detail: None,
        }
    }
}

/// What `send_input` answers.
#[derive(Debug, Serialize)]
pub(super) struct Accepted {
    pub(super) accepted: bool,
}

/// `value` as a succeeded call's text. These shapes are numbers, names and
/// strings, which JSON always takes; were one refused, the call still did
/// what it says (a spawned child runs on), so it stays succeeded and the
/// model reads the value's debug form rather than nothing.
pub(super) fn json<T: Serialize + Debug>(value: &T) -> ToolOutput {
    let text = serde_json::to_string(value).unwrap_or_else(|refused| unencoded(value, &refused));
    ToolOutput::succeeded(text)
}

/// `value` in its debug form, the refusal logged under the registry code
/// this crate's internal failures carry.
fn unencoded(value: &impl Debug, refused: &serde_json::Error) -> String {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = refused.to_string();
    let event = EVENT_ANSWER_ENCODE_FAILED;
    tracing::warn!(error_code = code, reason, event);
    format!("{value:?}")
}

/// What a call naming a child the run does not have reads.
pub(super) fn not_found(id: u64) -> ToolOutput {
    ToolOutput::failed(
        ToolErrorCode::ChildNotFound,
        &format!("no child of this run has id {id}"),
    )
}
