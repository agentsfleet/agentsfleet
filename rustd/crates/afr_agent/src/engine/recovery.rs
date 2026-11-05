//! Private conversation bytes bind to the same native task and file generation.

use crate::Result;
use afd_wire::task::TaskContext;
use afr_providers::{Message, Replay};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Frozen private conversation paired with its native task recovery boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct TaskRecovery {
    /// Current native identity and cursors captured with the conversation.
    pub task: TaskContext<'static>,
    /// Serialized private conversation; never included in the ordinary report.
    pub history: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for TaskRecovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskRecovery")
            .field("task_id", &self.task.task_id)
            .field("history_bytes", &self.history.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    format: u32,
    task_id: String,
    goal_digest: String,
    model: String,
    provider: String,
    messages: Vec<Message>,
}

impl TaskRecovery {
    pub(crate) fn capture(
        task: TaskContext<'static>,
        model: &str,
        provider: &str,
        messages: Vec<Message>,
    ) -> Result<Self> {
        let history = History {
            format: 1,
            task_id: task.task_id.to_string(),
            goal_digest: task.goal_digest.to_string(),
            model: model.to_owned(),
            provider: provider.to_owned(),
            messages,
        };
        if !afd_wire::decision::serialized_within(&history, 64 * 1024 * 1024) {
            return Err(crate::decision::unavailable());
        }
        let history = Zeroizing::new(serde_json::to_vec(&history)?);
        Ok(Self { task, history })
    }

    pub(crate) fn restore(
        bytes: &[u8],
        task: &TaskContext<'_>,
        model: &str,
        provider: &str,
        scrub: &afr_secrets::Scrub,
    ) -> Result<Vec<Message>> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(crate::decision::unavailable());
        }
        let mut history: History = serde_json::from_slice(bytes)?;
        if history.format != 1 || history.task_id != task.task_id {
            return Err(crate::decision::unavailable());
        }
        // Goal adoption changes authority, not the identity of authenticated
        // past work. The opening instructions and next goal assessment come
        // from the current native task, never this historical digest.
        let changed = history.model != model
            || history.provider != provider
            || history.goal_digest != task.goal_digest;
        for message in &mut history.messages {
            match message {
                Message::User(text) | Message::ToolResult { output: text, .. } => {
                    *text = scrub.text(text).into_owned()
                }
                Message::Assistant {
                    text,
                    calls,
                    replay,
                } => {
                    *text = scrub.text(text).into_owned();
                    for call in calls {
                        call.arguments = scrub
                            .clean_json(std::mem::take(&mut call.arguments))
                            .into_inner();
                    }
                    // Signed replay cannot be edited. Drop it when the current
                    // secret policy would change its private contents.
                    let encoded = Zeroizing::new(serde_json::to_string(&*replay)?);
                    if changed || scrub.text(&encoded).as_ref() != encoded.as_str() {
                        *replay = Replay::default();
                    }
                }
            }
        }
        Ok(history.messages)
    }
}
