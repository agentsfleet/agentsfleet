//! `tool_search`: tools the model is not offered until it looks for them,
//! Codex's deferred loading (`core/src/tools/handlers/tool_search.rs`).
//!
//! A run with many tools, such as every tool of several MCP servers, holds
//! most of them unoffered ([`crate::Tools::hold`]) and keeps their names and
//! descriptions in a [`Deferred`]. The model searches them by what it needs;
//! the ones found are loaded, and the embedder offers every loaded tool from
//! the next model call on ([`Deferred::is_loaded`]). A held tool is callable
//! all along, so a model that already knows a name need not search for it.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::{ToolOutput, ToolSpec};
use crate::tools::{Context, Handler};

/// How many tools a search loads when the model names no limit.
const LIMIT_DEFAULT: usize = 5;
/// The most tools one search loads.
const LIMIT_MAX: usize = 20;
/// What a search that found nothing reads back.
const FOUND_NOTHING: &str = "no deferred tool matches";
/// What a search that loaded tools reads back last.
const LOADED: &str = "These tools are offered from your next step.";

/// The tools a run holds unoffered, and which of them a search loaded.
#[derive(Debug, Default)]
pub struct Deferred {
    /// Each deferred tool's name and description, in the order they were
    /// held.
    entries: Vec<(String, String)>,
    loaded: Mutex<BTreeSet<String>>,
}

impl Deferred {
    /// The deferred tools `specs` describe, such as a run's
    /// [`crate::Tools::held_specs`].
    #[must_use]
    pub fn new<'a>(specs: impl IntoIterator<Item = ToolSpec<'a>>) -> Self {
        Self {
            entries: specs
                .into_iter()
                .map(|spec| (spec.name.to_owned(), spec.description.to_owned()))
                .collect(),
            loaded: Mutex::default(),
        }
    }

    /// Whether a search loaded the tool `name`.
    #[must_use]
    pub fn is_loaded(&self, name: &str) -> bool {
        self.held().contains(name)
    }

    /// Every tool a search loaded, by name.
    #[must_use]
    pub fn loaded(&self) -> Vec<String> {
        self.held().iter().cloned().collect()
    }

    /// The loaded names. Every change is one insert, so a panicking holder
    /// leaves them whole.
    fn held(&self) -> MutexGuard<'_, BTreeSet<String>> {
        self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The deferred tools that best match `query`, at most `limit`: the
    /// more of its words a tool's name and description hold, ignoring case,
    /// the better, ties in the order they were held. Each found is loaded.
    fn search(&self, query: &str, limit: usize) -> Vec<&(String, String)> {
        let words: Vec<String> = query
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect();
        let mut scored: Vec<(usize, usize, &(String, String))> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(order, entry)| {
                let haystack = format!("{} {}", entry.0, entry.1).to_lowercase();
                let score = words
                    .iter()
                    .filter(|word| haystack.contains(word.as_str()))
                    .count();
                (score > 0).then_some((score, order, entry))
            })
            .collect();
        scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        scored.truncate(limit);
        let found: Vec<&(String, String)> = scored.into_iter().map(|(_, _, entry)| entry).collect();
        let mut loaded = self.held();
        for (name, _description) in &found {
            loaded.insert(name.clone());
        }
        drop(loaded);
        found
    }
}

/// `tool_search`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// What the tool should do, in a few words, such as `create github issue`.
    query: String,
    /// The most tools to load: 5 when absent, at most 20.
    #[serde(default)]
    limit: Option<usize>,
}

/// Searches the run's deferred tools and loads the ones found.
#[derive(Debug, Clone)]
pub struct ToolSearch {
    deferred: Arc<Deferred>,
}

impl ToolSearch {
    /// The search over `deferred`, which the embedder also reads to offer
    /// what it loaded.
    #[must_use]
    pub const fn new(deferred: Arc<Deferred>) -> Self {
        Self { deferred }
    }
}

impl Handler for ToolSearch {
    const NAME: &'static str = "tool_search";
    const DESCRIPTION: &'static str = "Search the tools this run holds but has not offered you, \
        by what you need them to do, and load the best matches: each is named with what it \
        does, and offered to you from your next step.";
    type Arguments = Query;

    fn run(
        &self,
        arguments: Query,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let limit = arguments.limit.unwrap_or(LIMIT_DEFAULT).min(LIMIT_MAX);
        let found = self.deferred.search(&arguments.query, limit);
        let answer = if found.is_empty() {
            format!("{FOUND_NOTHING} {}", arguments.query)
        } else {
            let mut lines: Vec<String> = found
                .iter()
                .map(|(name, description)| format!("{name}: {description}"))
                .collect();
            lines.push(LOADED.to_owned());
            lines.join("\n")
        };
        std::future::ready(ToolOutput::succeeded(answer))
    }
}
