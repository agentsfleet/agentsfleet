//! The four memory tools, over the lease's
//! [`MemoryBackend`](afr_memory::MemoryBackend).
//!
//! Each reads or writes the backend the fleet is bound to and words its
//! answer from what the backend did, so a tool never assumes which backend
//! holds the fleet's memory.

use std::borrow::Cow;

use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY, RECALL_LIMIT_MAX, Visibility};
use afr_memory::{Forgotten, Recalled};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{Entry, MEMORY_FORGET, MEMORY_LIST, MEMORY_RECALL, MEMORY_STORE};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// How many entries a recall answers with when the model names no limit.
const RECALL_DEFAULT: usize = 5;
/// What a recall that matched nothing reads back.
const RECALLED_NOTHING: &str = "nothing remembered matches";
/// What a list over an empty memory reads back.
const LISTED_NOTHING: &str = "nothing remembered";
/// What a forget of an unknown key reads back.
const FORGOT_NOTHING: &str = "nothing remembered under";
/// What names the fleet that wrote a shared entry.
const SHARED_BY: &str = "shared by";
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
    /// `fleet` (the default) keeps it to this fleet; `workspace` lets every
    /// fleet granted to read shared memory recall it, naming this fleet.
    visibility: Option<Reach>,
}

/// Who reads a stored memory, as the model names it.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Reach {
    /// Only this fleet.
    Fleet,
    /// Every fleet in the workspace granted to read shared memory.
    Workspace,
}

impl From<Reach> for Visibility {
    fn from(reach: Reach) -> Self {
        match reach {
            Reach::Fleet => Self::Fleet,
            Reach::Workspace => Self::Workspace,
        }
    }
}

/// `memory_recall`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Recall {
    /// Text to look for in each memory's key and content, ignoring case;
    /// empty matches every memory.
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
    const DESCRIPTION: &'static str =
        "Remember a fact for later runs of this fleet under a stable key.";
    type Arguments = Store;

    async fn run(&self, arguments: Store, context: ToolContext<'_, '_>) -> ToolOutput {
        let Store {
            key,
            content,
            category,
            visibility,
        } = arguments;
        let stored = format!("stored {key}");
        let delta = MemoryDelta {
            key: Cow::Owned(key),
            content: Cow::Owned(content),
            category: category.map_or(Cow::Borrowed(PINNED_CATEGORY), Cow::Owned),
            visibility: visibility.map(Visibility::from).unwrap_or_default(),
        };
        let stored_entry = context.lease.memory.lock().await.store(delta).await;
        match stored_entry {
            Ok(()) => ToolOutput::succeeded(stored),
            Err(failure) => refused(&failure),
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
        "Read remembered facts whose key or content holds the query, key matches first.";
    type Arguments = Recall;

    async fn run(&self, arguments: Recall, context: ToolContext<'_, '_>) -> ToolOutput {
        let limit = arguments
            .limit
            .unwrap_or(RECALL_DEFAULT)
            .min(RECALL_LIMIT_MAX);
        let memory = context.lease.memory.lock().await;
        match memory.recall(&arguments.query, limit).await {
            Ok(recalled) => ToolOutput::succeeded(
                lines(&recalled, |found| {
                    format!("{} ({}): {}", found.key, label(found), found.content)
                })
                .unwrap_or_else(|| format!("{RECALLED_NOTHING} {}", arguments.query)),
            ),
            Err(failure) => refused(&failure),
        }
    }
}

/// Lists what is remembered.
#[derive(Debug)]
pub(crate) struct MemoryList;

#[async_trait::async_trait]
impl Handler for MemoryList {
    const ENTRY: &'static Entry = &MEMORY_LIST;
    const DESCRIPTION: &'static str =
        "List the keys and categories of every remembered fact, newest first.";
    type Arguments = List;

    async fn run(&self, arguments: List, context: ToolContext<'_, '_>) -> ToolOutput {
        let memory = context.lease.memory.lock().await;
        match memory.list(arguments.category.as_deref()).await {
            Ok(listed) => ToolOutput::succeeded(
                lines(&listed, |found| format!("{} ({})", found.key, label(found)))
                    .unwrap_or_else(|| LISTED_NOTHING.to_owned()),
            ),
            Err(failure) => refused(&failure),
        }
    }
}

/// Forgets one memory for the rest of the run.
#[derive(Debug)]
pub(crate) struct MemoryForget;

#[async_trait::async_trait]
impl Handler for MemoryForget {
    const ENTRY: &'static Entry = &MEMORY_FORGET;
    const DESCRIPTION: &'static str = "Forget a remembered fact for the rest of this run. The stored \
                                       copy stays, including one this run already saved; store the \
                                       key again to replace it.";
    type Arguments = Forget;

    async fn run(&self, arguments: Forget, context: ToolContext<'_, '_>) -> ToolOutput {
        let key = arguments.key;
        let forgotten = context.lease.memory.lock().await.forget(&key).await;
        match forgotten {
            Ok(Forgotten::ForThisRun) => ToolOutput::succeeded(format!("{key} {FORGOT}")),
            Ok(Forgotten::Unknown) => ToolOutput::succeeded(format!("{FORGOT_NOTHING} {key}")),
            Err(failure) => refused(&failure),
        }
    }
}

/// Each entry on its own line as `render` writes it; `None` when there is none.
fn lines(entries: &[Recalled<'_>], render: impl Fn(&Recalled<'_>) -> String) -> Option<String> {
    let rendered: Vec<String> = entries.iter().map(render).collect();
    (!rendered.is_empty()).then(|| rendered.join("\n"))
}

/// An entry's category, and — for one another fleet published — its writer.
fn label(found: &Recalled<'_>) -> String {
    found.writer.as_ref().map_or_else(
        || found.category.to_string(),
        |writer| format!("{}, {SHARED_BY} {writer}", found.category),
    )
}

/// A call the backend refused, with the code the model reads: a store past
/// the push is `memory_full`, a share the fleet may not make is
/// `workspace_memory_not_granted`; anything else the model can correct.
fn refused(failure: &afr_memory::Error) -> ToolOutput {
    let code = if failure.is_full() {
        ToolErrorCode::MemoryFull
    } else if failure.is_not_granted() {
        ToolErrorCode::WorkspaceMemoryNotGranted
    } else {
        ToolErrorCode::InvalidArguments
    };
    ToolOutput::failed(code, &failure.detail())
}

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "memory/shared_tests.rs"]
mod shared_tests;
