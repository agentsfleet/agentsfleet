//! The generic tools, behind the `tools` feature: what an agent does on a
//! machine and on the network, written once for every embedder.
//!
//! ```text
//!  embedder ──► Context { env, outbound, memory, sessions, … } lent per call
//!                 │
//!  Typed<Handler> ─► parse the arguments ─► run ─► ToolOutput the model reads
//!                 │
//!   env:      shell, exec_command, write_stdin, git, file_*, apply_patch,
//!             image, ls, find, grep, browser_open, browser, screenshot
//!   outbound: http_request, web_fetch, pushover, MCP over HTTP
//!   memory:   memory_store, memory_recall, memory_list, memory_forget
//!   none:     update_plan, ask_user, tool_search, current_time, sleep
//! ```
//!
//! A tool reaches the machine only through [`Env`], the network only
//! through [`Outbound`] and memory only through [`MemoryStore`], so the same
//! tool runs on a developer's laptop over [`LocalEnv`], [`LocalOutbound`]
//! and [`LocalMemory`], and inside an embedder's sandbox behind its egress
//! guard. Every tool is a [`Handler`]: its arguments are a type whose JSON
//! Schema the model is told, so what it is told and what it reads cannot
//! drift. [`Typed`] makes one a [`crate::Tool`] lent a [`Context`].

pub mod ask;
pub mod browser;
pub mod clock;
pub mod code;
pub mod env;
pub mod files;
pub mod mcp;
pub mod memory;
pub mod outbound;
pub mod plan;
pub mod process;
pub mod search;
pub mod web;

mod local;
mod schema;

use std::fmt;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::tool::{Describe, Tool, ToolFuture, ToolOutput, ToolSpec};

pub use self::clock::Wake;
pub use self::env::{Env, LocalEnv};
pub use self::local::Local;
pub use self::memory::{LocalMemory, MemoryStore};
pub use self::outbound::{LocalOutbound, Outbound};
pub use self::process::sessions::Sessions;
pub use self::schema::{NoArguments, Schema};

/// A generic tool as a loop holds it: lent a [`Context`] per call.
pub type ContextTool = dyn for<'c> Tool<Context<'c>>;

/// What a generic tool is lent for one call: where it runs, where it sends,
/// where it remembers, and the run it belongs to.
///
/// The embedder builds one per call from what its run holds; every field is
/// a borrow, so building one costs nothing.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// Where commands and files run; `None` for a call with no environment,
    /// which a tool that needs one refuses.
    pub env: Option<&'a dyn Env>,
    /// Where every network request goes.
    pub outbound: &'a dyn Outbound,
    /// Where memories are kept.
    pub memory: &'a dyn MemoryStore,
    /// The processes the run's calls keep open across calls.
    pub sessions: &'a Sessions,
    /// Whether the model's wire takes an image with a call's result; `image`
    /// refuses before any read when it does not.
    pub image_input: bool,
    /// The directory `git` runs in, relative to the environment's root; the
    /// root when `None`.
    pub repository: Option<&'a str>,
    /// What names the run on the tools' log lines.
    pub run_id: &'a str,
    /// Woken when new input reaches the run, so `sleep` wakes early.
    pub wake: Option<&'a Wake>,
}

/// One tool's behaviour, given arguments that parsed.
///
/// The model is told the JSON Schema of [`Handler::Arguments`] and a call's
/// arguments parse into it, so an argument the type does not name refuses
/// the call (`#[serde(deny_unknown_fields)]`), the way Codex's tools parse
/// theirs. [`Typed`] is the one place a call is parsed. An implementation
/// writes `run` as an `async fn`; its future must be `Send`.
pub trait Handler: Send + Sync + fmt::Debug + 'static {
    /// The name the model calls it by.
    const NAME: &'static str;
    /// What the tool does, as the model reads it.
    const DESCRIPTION: &'static str;
    /// What one call carries.
    type Arguments: DeserializeOwned + JsonSchema + Send;

    /// Runs one call.
    fn run(
        &self,
        arguments: Self::Arguments,
        context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send;
}

/// A [`Handler`] as a loop holds it: its schema, built once.
#[derive(Debug)]
pub struct Typed<H> {
    handler: H,
    schema: Schema,
}

impl<H: Handler> Typed<H> {
    /// `handler`, its schema derived from its arguments' type.
    #[must_use]
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            schema: Schema::of::<H::Arguments>(H::DESCRIPTION),
        }
    }

    /// The handler it runs.
    #[must_use]
    pub const fn handler(&self) -> &H {
        &self.handler
    }
}

impl<H: Handler> Describe for Typed<H> {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(H::NAME)
    }
}

impl<'c, H: Handler> Tool<Context<'c>> for Typed<H> {
    fn call<'a>(&'a self, arguments: &'a Value, cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        Box::pin(async move {
            match parsed(arguments) {
                Ok(arguments) => self.handler.run(arguments, cx).await,
                Err(refused) => refused,
            }
        })
    }
}

/// `arguments` as `A`, or the refusal the model reads when they do not parse
/// as `A`'s schema: [`code::INVALID_ARGUMENTS`] and serde's sentence.
///
/// # Errors
/// The arguments break the schema: a field missing, mistyped, or not named.
pub fn parsed<A: DeserializeOwned>(arguments: &Value) -> Result<A, ToolOutput> {
    A::deserialize(arguments)
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))
}

/// Every generic tool that needs nothing but a [`Context`], boxed.
///
/// They come in the order the model is offered them: the process, file,
/// web, memory, browser, plan, question and clock tools. MCP tools and
/// `tool_search` need what the embedder connected and deferred, so it adds
/// those itself.
#[must_use]
pub fn standard() -> Vec<Box<ContextTool>> {
    vec![
        Box::new(Typed::new(process::Shell)),
        Box::new(Typed::new(process::ExecCommand)),
        Box::new(Typed::new(process::WriteStdin)),
        Box::new(Typed::new(process::Git)),
        Box::new(Typed::new(files::read::FileRead)),
        Box::new(Typed::new(files::hashed::FileReadHashed)),
        Box::new(Typed::new(files::FileWrite)),
        Box::new(Typed::new(files::FileAppend)),
        Box::new(Typed::new(files::FileDelete)),
        Box::new(Typed::new(files::FileEdit)),
        Box::new(Typed::new(files::hashed::FileEditHashed)),
        Box::new(Typed::new(files::apply_patch::ApplyPatch)),
        Box::new(Typed::new(files::image::ImageTool)),
        Box::new(Typed::new(files::listing::Ls)),
        Box::new(Typed::new(files::listing::Find)),
        Box::new(Typed::new(files::listing::Grep)),
        Box::new(Typed::new(web::HttpRequest)),
        Box::new(Typed::new(web::WebFetch)),
        Box::new(Typed::new(web::Pushover)),
        Box::new(Typed::new(memory::MemoryStoreTool)),
        Box::new(Typed::new(memory::MemoryRecall)),
        Box::new(Typed::new(memory::MemoryList)),
        Box::new(Typed::new(memory::MemoryForget)),
        Box::new(Typed::new(browser::BrowserOpen)),
        Box::new(Typed::new(browser::Browse)),
        Box::new(Typed::new(browser::Screenshot)),
        Box::new(Typed::new(plan::UpdatePlan)),
        Box::new(Typed::new(ask::AskUser)),
        Box::new(Typed::new(clock::CurrentTime)),
        Box::new(Typed::new(clock::Sleep)),
    ]
}
