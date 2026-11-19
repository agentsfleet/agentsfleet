//! Private conversation bytes bind to the same native task and file generation.

use super::RootRecovery;
use crate::Result;
use af_wire::task::TaskContext;
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
    root: RootRecovery,
}

impl TaskRecovery {
    pub(crate) fn capture(
        task: TaskContext<'static>,
        model: &str,
        provider: &str,
        root: RootRecovery,
    ) -> Result<Self> {
        root.check()?;
        let history = History {
            format: 2,
            task_id: task.task_id.to_string(),
            goal_digest: task.goal_digest.to_string(),
            model: model.to_owned(),
            provider: provider.to_owned(),
            root,
        };
        if !af_wire::decision::serialized_within(&history, 64 * 1024 * 1024) {
            return Err(crate::error::recovery_invalid(
                "private task history exceeds its byte limit",
            ));
        }
        let history = Zeroizing::new(serde_json::to_vec(&history)?);
        Ok(Self { task, history })
    }

    pub(crate) fn restore(
        bytes: &[u8],
        task: &TaskContext<'_>,
        model: &str,
        provider: &str,
        scrub: &af_secrets::Scrub,
    ) -> Result<RootRecovery> {
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(crate::error::recovery_invalid(
                "private task history exceeds its byte limit",
            ));
        }
        let mut history: History = serde_json::from_slice(bytes)
            .map_err(|source| crate::error::recovery_source("private task history", source))?;
        if history.format != 2 || history.task_id != task.task_id {
            return Err(crate::error::recovery_invalid(
                "private task history version or identity differs",
            ));
        }
        history.root.check()?;
        if !history.root.folds.is_empty() {
            return Err(crate::error::recovery_invalid(
                "native task history cannot contain ordinary fold claims",
            ));
        }
        // Goal adoption changes authority, not the identity of authenticated
        // past work. The opening instructions and next goal assessment come
        // from the current native task, never this historical digest.
        let changed = history.model != model
            || history.provider != provider
            || history.goal_digest != task.goal_digest;
        history.root.scrub(scrub, changed)?;
        Ok(history.root)
    }
}
