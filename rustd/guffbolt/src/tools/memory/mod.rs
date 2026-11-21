//! The four memory tools, over the [`MemoryStore`] each is built with.
//!
//! Each reads or writes its store and words its answer from what the store
//! did, so a tool never assumes where memory lives: a process's own map
//! ([`LocalMemory`]), or a service that keeps it for later runs and shares it
//! between agents.

mod local;

use std::fmt;
use std::pin::Pin;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::{ErrorCode, ToolOutput};
use crate::tools::{Context, Handler};

pub use self::local::LocalMemory;

/// The category a memory stored with none is kept under: read first by
/// every later run.
pub const CORE_CATEGORY: &str = "core";
/// The most memories one recall answers with.
pub const RECALL_LIMIT_MAX: usize = 50;
/// How many entries a recall answers with when the model names no limit.
const RECALL_DEFAULT: usize = 5;
/// What a recall that matched nothing reads back.
const RECALLED_NOTHING: &str = "nothing remembered matches";
/// What a list over an empty memory reads back.
const LISTED_NOTHING: &str = "nothing remembered";
/// What a forget of an unknown key reads back.
const FORGOT_NOTHING: &str = "nothing remembered under";
/// What names the writer of a shared entry.
const SHARED_BY: &str = "shared by";
/// What a forget reads back after the key.
const FORGOT: &str = "is forgotten for the rest of this run; the stored copy stays \
                      until a store under the same key replaces it";

/// One store call's work.
pub type MemoryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, MemoryRefused>> + Send + 'a>>;

/// Who reads a stored memory, as the model names it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    /// Only this agent.
    #[default]
    Private,
    /// Every agent granted to read shared memory, which names this agent as
    /// the writer.
    Shared,
}

/// A memory to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    /// Its key; storing under it again replaces what it held.
    pub key: String,
    /// What to remember.
    pub content: String,
    /// Its category: [`CORE_CATEGORY`] when the model named none.
    pub category: String,
    /// Who reads it.
    pub reach: Reach,
}

/// One remembered entry, as a recall or a list reads it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    /// Its key.
    pub key: String,
    /// What it holds.
    pub content: String,
    /// Its category.
    pub category: String,
    /// Who wrote it, for an entry another agent shared.
    pub writer: Option<String>,
}

/// What a forget did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgot {
    /// The entry is gone for the rest of the run.
    ForThisRun,
    /// No entry of this run's had the key.
    Unknown,
}

/// A store call refused: the code and the sentence the model reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRefused {
    /// The code, such as [`crate::tools::code::MEMORY_FULL`].
    pub code: ErrorCode,
    /// The sentence after it.
    pub detail: String,
}

/// Where a run's memory is kept.
///
/// Every method takes `&self`: a store shared by a run's calls keeps its own
/// lock, held for one read or write.
pub trait MemoryStore: Send + Sync + fmt::Debug {
    /// Stores `memory`, replacing what its key held.
    fn store(&self, memory: Memory) -> MemoryFuture<'_, ()>;

    /// The entries whose key or content holds `query`, ignoring case: key
    /// matches first, then content matches, each newest first, at most
    /// `limit`; an empty query holds in every entry.
    fn recall<'a>(&'a self, query: &'a str, limit: usize) -> MemoryFuture<'a, Vec<Remembered>>;

    /// Every entry, or every entry in `category`, newest first.
    fn list<'a>(&'a self, category: Option<&'a str>) -> MemoryFuture<'a, Vec<Remembered>>;

    /// Forgets this run's entry under `key`.
    fn forget<'a>(&'a self, key: &'a str) -> MemoryFuture<'a, Forgot>;
}

/// `memory_store`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Store {
    /// A stable key, such as `incident:42:findings`; storing under it again
    /// replaces what it held.
    key: String,
    /// What to remember.
    content: String,
    /// `core` (the default) is read first by every later run; `daily` expires
    /// after a retention sweep; any other label is kept by recency.
    category: Option<String>,
    /// `private` (the default) keeps it to this agent; `shared` lets other
    /// agents granted shared memory recall it, naming the writer.
    visibility: Option<Reach>,
}

/// `memory_recall`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recall {
    /// Text to look for in each memory's key and content, ignoring case;
    /// empty matches every memory.
    query: String,
    /// The most memories to answer with: 5 when absent, never more than 50.
    limit: Option<usize>,
}

/// `memory_list`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct List {
    /// Only memories in this category.
    category: Option<String>,
}

/// `memory_forget`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Forget {
    /// The key to forget.
    key: String,
}

backed! {
    /// Stores one memory for this run and every later one.
    MemoryStoreTool over MemoryStore
}

impl Handler for MemoryStoreTool<'_> {
    const NAME: &'static str = "memory_store";
    const DESCRIPTION: &'static str = "Remember a fact for later runs under a stable key.";
    type Arguments = Store;

    async fn run(&self, arguments: Store, _context: Context<'_>) -> ToolOutput {
        let Store {
            key,
            content,
            category,
            visibility,
        } = arguments;
        let stored = format!("stored {key}");
        let memory = Memory {
            key,
            content,
            category: category.unwrap_or_else(|| CORE_CATEGORY.to_owned()),
            reach: visibility.unwrap_or_default(),
        };
        match self.backend.store(memory).await {
            Ok(()) => ToolOutput::succeeded(stored),
            Err(refused) => refused.into(),
        }
    }
}

backed! {
    /// Finds memories by the words they hold.
    MemoryRecall over MemoryStore
}

impl Handler for MemoryRecall<'_> {
    const NAME: &'static str = "memory_recall";
    const DESCRIPTION: &'static str =
        "Read remembered facts whose key or content holds the query, key matches first.";
    type Arguments = Recall;

    async fn run(&self, arguments: Recall, _context: Context<'_>) -> ToolOutput {
        let limit = arguments
            .limit
            .unwrap_or(RECALL_DEFAULT)
            .min(RECALL_LIMIT_MAX);
        match self.backend.recall(&arguments.query, limit).await {
            Ok(recalled) => ToolOutput::succeeded(
                lines(&recalled, |found| {
                    format!("{} ({}): {}", found.key, label(found), found.content)
                })
                .unwrap_or_else(|| format!("{RECALLED_NOTHING} {}", arguments.query)),
            ),
            Err(refused) => refused.into(),
        }
    }
}

backed! {
    /// Lists what is remembered.
    MemoryList over MemoryStore
}

impl Handler for MemoryList<'_> {
    const NAME: &'static str = "memory_list";
    const DESCRIPTION: &'static str =
        "List the keys and categories of every remembered fact, newest first.";
    type Arguments = List;

    async fn run(&self, arguments: List, _context: Context<'_>) -> ToolOutput {
        match self.backend.list(arguments.category.as_deref()).await {
            Ok(listed) => ToolOutput::succeeded(
                lines(&listed, |found| format!("{} ({})", found.key, label(found)))
                    .unwrap_or_else(|| LISTED_NOTHING.to_owned()),
            ),
            Err(refused) => refused.into(),
        }
    }
}

backed! {
    /// Forgets one memory for the rest of the run.
    MemoryForget over MemoryStore
}

impl Handler for MemoryForget<'_> {
    const NAME: &'static str = "memory_forget";
    const DESCRIPTION: &'static str = "Forget a remembered fact for the rest of this run. The stored \
                                       copy stays, including one this run already saved; store the \
                                       key again to replace it.";
    type Arguments = Forget;

    async fn run(&self, arguments: Forget, _context: Context<'_>) -> ToolOutput {
        let key = arguments.key;
        match self.backend.forget(&key).await {
            Ok(Forgot::ForThisRun) => ToolOutput::succeeded(format!("{key} {FORGOT}")),
            Ok(Forgot::Unknown) => ToolOutput::succeeded(format!("{FORGOT_NOTHING} {key}")),
            Err(refused) => refused.into(),
        }
    }
}

impl From<MemoryRefused> for ToolOutput {
    fn from(refused: MemoryRefused) -> Self {
        Self::failed(refused.code, &refused.detail)
    }
}

/// Each entry on its own line as `render` writes it; `None` when there is none.
fn lines(entries: &[Remembered], render: impl Fn(&Remembered) -> String) -> Option<String> {
    let rendered: Vec<String> = entries.iter().map(render).collect();
    (!rendered.is_empty()).then(|| rendered.join("\n"))
}

/// An entry's category, and — for one another agent shared — its writer.
fn label(found: &Remembered) -> String {
    found.writer.as_ref().map_or_else(
        || found.category.clone(),
        |writer| format!("{}, {SHARED_BY} {writer}", found.category),
    )
}
