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
    /// The prompt, in tokens, that reaches the cap; none when the model's
    /// window is unknown.
    limit: Option<f64>,
}

impl Budget {
    /// The budget a lease's policy sets. A zero window keeps every result.
    /// The cap is reached at the fill fraction a stage chunks at, of the
    /// model's window, as `capabilities.md` §4 measures fill; a zero window is
    /// one the daemon could not resolve, and the runtime bakes in none, so it
    /// never trips.
    pub(crate) fn new(budget: &ContextBudget<'_>) -> Self {
        let window = budget.context_cap_tokens;
        let fill = f64::from(budget.stage_chunk_threshold);
        Self {
            tool_window: budget.tool_window as usize,
            limit: (window > 0).then(|| f64::from(window) * fill),
        }
    }

    /// Whether a turn whose prompt took `input_tokens` reached the cap. A
    /// count past `u32` is past any window.
    pub(crate) fn reached(self, input_tokens: u64) -> bool {
        let input = f64::from(u32::try_from(input_tokens).unwrap_or(u32::MAX));
        self.limit.is_some_and(|limit| input >= limit)
    }

    /// Replaces the output of every tool result older than the newest
    /// `tool_window`, and drops its image. The result stays, since a provider
    /// wants an answer for every call it made; its text and image leave the
    /// window, so an image is not sent again with every later turn.
    pub(crate) fn evict(self, messages: &mut [Message]) {
        if self.tool_window == 0 {
            return;
        }
        messages
            .iter_mut()
            .rev()
            .filter_map(|message| match message {
                Message::ToolResult { output, image, .. } => Some((output, image)),
                Message::User(_) | Message::Assistant { .. } => None,
            })
            .skip(self.tool_window)
            .take_while(|(output, _image)| output.as_str() != EVICTED)
            .for_each(|(output, image)| {
                EVICTED.clone_into(output);
                *image = None;
            });
    }
}

/// When a run writes its memory back mid-run: every `every` calls, never when
/// `every` is zero.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Checkpoints {
    every: u32,
    since: u32,
}

impl Checkpoints {
    /// The cadence a lease's policy sets.
    pub(crate) const fn new(budget: &ContextBudget<'_>) -> Self {
        Self {
            every: budget.memory_checkpoint_every,
            since: 0,
        }
    }

    /// Counts one finished call; `true` when it completes a cadence.
    pub(crate) fn due(&mut self) -> bool {
        if self.every == 0 {
            return false;
        }
        self.since += 1;
        let due = self.since >= self.every;
        if due {
            self.since = 0;
        }
        due
    }
}

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;
