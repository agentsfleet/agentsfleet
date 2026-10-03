//! The four memory tools, over the run's [`Memory`](afr_memory::Memory).
//!
//! A store reaches the push the supervisor sends before the report; a forget
//! holds for this run, because the daemon only upserts what is pushed.

use std::borrow::Cow;

use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{Entry, MEMORY_FORGET, MEMORY_LIST, MEMORY_RECALL, MEMORY_STORE};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// How many entries a recall answers with when the model names no limit.
const RECALL_DEFAULT: usize = 5;
/// The most entries one recall answers with.
const RECALL_MAX: usize = 50;
/// What a recall that matched nothing reads back.
const RECALLED_NOTHING: &str = "nothing remembered matches";
/// What a list over an empty memory reads back.
const LISTED_NOTHING: &str = "nothing remembered";
/// What a forget of an unknown key reads back.
const FORGOT_NOTHING: &str = "nothing remembered under";
/// What a forget reads back after the key.
const FORGOT: &str = "is forgotten for the rest of this run; the fleet's stored copy \
                      stays until a store under the same key replaces it";

/// `memory_store`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    /// A stable key, such as `incident:42:findings`; storing under it again
    /// replaces what it held.
    key: String,
    /// What to remember.
    content: String,
    /// `core` (the default) is read first by every later run; `daily` expires
    /// after a retention sweep; any other label is kept by recency.
    category: Option<String>,
}

/// `memory_recall`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recall {
    /// Text to look for in each memory's key, ignoring case; empty matches
    /// every memory.
    query: String,
    /// The most memories to answer with: 5 when absent, never more than 50.
    limit: Option<usize>,
}

/// `memory_list`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct List {
    /// Only memories in this category.
    category: Option<String>,
}

/// `memory_forget`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Forget {
    /// The key to forget.
    key: String,
}

/// Stores one memory for this run and every later one.
#[derive(Debug)]
pub(crate) struct MemoryStore;

#[async_trait::async_trait]
impl Handler for MemoryStore {
    const ENTRY: &'static Entry = &MEMORY_STORE;
    const DESCRIPTION: &'static str = "Remember a fact for later runs of this fleet under a stable key.";
    type Arguments = Store;

    async fn run(&self, arguments: Store, context: ToolContext<'_, '_>) -> ToolOutput {
        let Store {
            key,
            content,
            category,
        } = arguments;
        let stored = format!("stored {key}");
        let delta = MemoryDelta {
            key: Cow::Owned(key),
            content: Cow::Owned(content),
            category: category.map_or(Cow::Borrowed(PINNED_CATEGORY), Cow::Owned),
        };
        match context.lease.memory.store(delta) {
            Ok(()) => ToolOutput::succeeded(stored),
            Err(refused) if refused.is_full() => {
                ToolOutput::failed(ToolErrorCode::MemoryFull, &refused.detail())
            }
            Err(refused) => ToolOutput::failed(ToolErrorCode::InvalidArguments, &refused.detail()),
        }
    }
}

/// Finds memories by the words they hold.
#[derive(Debug)]
pub(crate) struct MemoryRecall;

#[async_trait::async_trait]
impl Handler for MemoryRecall {
    const ENTRY: &'static Entry = &MEMORY_RECALL;
    const DESCRIPTION: &'static str =
        "Read remembered facts whose key holds the query, newest first.";
    type Arguments = Recall;

    async fn run(&self, arguments: Recall, context: ToolContext<'_, '_>) -> ToolOutput {
        let limit = arguments.limit.unwrap_or(RECALL_DEFAULT).min(RECALL_MAX);
        let recalled = context.lease.memory.recall(&arguments.query, limit);
        let text = lines(recalled, |delta| {
            format!("{} ({}): {}", delta.key, delta.category, delta.content)
        });
        ToolOutput::succeeded(
            text.unwrap_or_else(|| format!("{RECALLED_NOTHING} {}", arguments.query)),
        )
    }
}

/// Lists what is remembered.
#[derive(Debug)]
pub(crate) struct MemoryList;

#[async_trait::async_trait]
impl Handler for MemoryList {
    const ENTRY: &'static Entry = &MEMORY_LIST;
    const DESCRIPTION: &'static str = "List the keys and categories of every remembered fact, newest first.";
    type Arguments = List;

    async fn run(&self, arguments: List, context: ToolContext<'_, '_>) -> ToolOutput {
        let listed = context.lease.memory.list(arguments.category.as_deref());
        let text = lines(listed, |delta| format!("{} ({})", delta.key, delta.category));
        ToolOutput::succeeded(text.unwrap_or_else(|| LISTED_NOTHING.to_owned()))
    }
}

/// Forgets one memory for the rest of the run.
#[derive(Debug)]
pub(crate) struct MemoryForget;

#[async_trait::async_trait]
impl Handler for MemoryForget {
    const ENTRY: &'static Entry = &MEMORY_FORGET;
    const DESCRIPTION: &'static str = "Forget a remembered fact for the rest of this run.";
    type Arguments = Forget;

    async fn run(&self, arguments: Forget, context: ToolContext<'_, '_>) -> ToolOutput {
        let key = arguments.key;
        let text = if context.lease.memory.forget(&key) {
            format!("{key} {FORGOT}")
        } else {
            format!("{FORGOT_NOTHING} {key}")
        };
        ToolOutput::succeeded(text)
    }
}

/// Each entry on its own line as `render` writes it; `None` when there is none.
fn lines<'d, 'run: 'd>(
    entries: impl IntoIterator<Item = &'d MemoryDelta<'run>>,
    render: impl Fn(&MemoryDelta<'run>) -> String,
) -> Option<String> {
    let rendered: Vec<String> = entries.into_iter().map(render).collect();
    (!rendered.is_empty()).then(|| rendered.join("\n"))
}

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;
