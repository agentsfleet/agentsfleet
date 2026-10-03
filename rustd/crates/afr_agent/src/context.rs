//! The context budget: how many tool results stay in the model's window, and
//! the point past which the model is asked to answer with no tools offered.

use afd_wire::policy::ContextBudget;
use afr_providers::Message;

/// What a tool result evicted from the window reads as.
pub(crate) const EVICTED: &str =
    "[this output left the context window; call the tool again if it is still needed]";

/// What the model is told once the context cap is reached.
pub(crate) const CAP_REACHED: &str = "The context budget for this run is spent and no tools are offered any more. Answer now with what you have, and say plainly what was not done.";

/// One run's budget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Budget {
    tool_window: usize,
    cap_tokens: u64,
}

impl Budget {
    /// The budget a lease's policy sets. A zero window keeps every result, and
    /// a zero cap is one the daemon could not resolve, so it never trips.
    pub(crate) fn new(budget: &ContextBudget<'_>) -> Self {
        Self {
            tool_window: budget.tool_window as usize,
            cap_tokens: u64::from(budget.context_cap_tokens),
        }
    }

    /// Whether a turn whose prompt took `input_tokens` reached the cap.
    pub(crate) const fn reached(self, input_tokens: u64) -> bool {
        self.cap_tokens > 0 && input_tokens >= self.cap_tokens
    }

    /// Replaces the output of every tool result older than the newest
    /// `tool_window`. The result stays, since a provider wants an answer for
    /// every call it made; only its text leaves the window.
    pub(crate) fn evict(self, messages: &mut [Message]) {
        if self.tool_window == 0 {
            return;
        }
        messages
            .iter_mut()
            .rev()
            .filter_map(|message| match message {
                Message::ToolResult { output, .. } => Some(output),
                Message::User(_) | Message::Assistant { .. } => None,
            })
            .skip(self.tool_window)
            .take_while(|output| output.as_str() != EVICTED)
            .for_each(|output| EVICTED.clone_into(output));
    }
}

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;
