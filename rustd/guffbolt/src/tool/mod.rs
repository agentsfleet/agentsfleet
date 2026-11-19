//! The tools a loop offers the model, and what one call hands back.
//!
//! A tool tells the model what it is without any call's context
//! ([`Describe`]) and runs a call with the context its embedder lends it
//! ([`Tool`]), the split Codex draws between a tool's spec and its
//! `ToolExecutor<Invocation>`. [`Tools`] is the set one loop holds: it looks a
//! call up by name and answers a name it holds no tool for with the harness's
//! own refusal, so no embedder writes that sentence twice.

use std::fmt;
use std::future::ready;
use std::pin::Pin;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a call to a tool the loop does not hold reads, after its name.
const NOT_OFFERED: &str = "is not one of this run's tools";
/// What a call from a step cut at the output limit reads, after its name.
const CUT: &str = "was not run: the step reached the model's output limit and its \
                   arguments may be incomplete; call it again with shorter arguments";

/// One call's work, as a tool hands it to the loop.
pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = ToolOutput> + Send + 'a>>;

/// Why a call failed, in the spelling the model reads.
///
/// An embedder keeps its own closed set of codes and converts each into this
/// one, so the harness can refuse a call in the same words without knowing
/// that set. Two codes are equal when they are spelled alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// The model called a tool the loop does not hold.
    pub const NOT_OFFERED: Self = Self("tool_not_offered");
    /// The model's step stopped at its output limit, so the call may have
    /// been cut mid-argument and was not run.
    pub const OUTPUT_LIMIT_REACHED: Self = Self("output_limit_reached");
    /// What the call waited on ended before it answered.
    pub const INTERRUPTED: Self = Self("interrupted");
    /// The run already has as many children running, or started, as its
    /// caps allow, or its children are paused.
    pub const CHILD_CAP_REACHED: Self = Self("child_cap_reached");
    /// A child asked for a tool its parent does not hold.
    pub const CHILD_TOOL_NOT_HELD: Self = Self("child_tool_not_held");
    /// No child of the run has the id named.
    pub const CHILD_NOT_FOUND: Self = Self("child_not_found");
    /// The child ended on a failure of its own, which the output carries.
    pub const CHILD_FAILED: Self = Self("child_failed");
    /// Another call of the same step already asked the person a question.
    pub const QUESTION_PENDING: Self = Self("question_pending");

    /// The code spelled `spelling`.
    #[must_use]
    pub const fn new(spelling: &'static str) -> Self {
        Self(spelling)
    }

    /// The spelling the output text carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The kinds of image a call can hand the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageKind {
    /// Portable Network Graphics.
    Png,
    /// Joint Photographic Experts Group.
    Jpeg,
    /// Graphics Interchange Format.
    Gif,
    /// WebP.
    Webp,
}

impl ImageKind {
    /// The media type the kind is named by.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// An image a call read, for the model to see with that call's result.
///
/// It serializes with the conversation that carries it, so a resumed loop
/// hands the model the image it saw before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// What kind of image it is.
    pub kind: ImageKind,
    /// The bytes, as the file holds them.
    pub data: Bytes,
}

/// What one call hands back to the loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// What the model reads back.
    pub text: String,
    /// The process's exit code, for a call that ran one.
    pub exit_code: Option<i32>,
    /// Why the call failed, for one that did.
    pub error_code: Option<ErrorCode>,
    /// The image the call read, which rides its result alone.
    pub image: Option<Image>,
    /// The question a call asks the person: the loop ends
    /// [`crate::Ending::Waiting`] once the step's other calls ran, and the
    /// call's result is the answer the run is resumed with.
    pub asks: Option<String>,
}

impl ToolOutput {
    /// A call that succeeded with `text`.
    #[must_use]
    pub fn succeeded(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: None,
            error_code: None,
            image: None,
            asks: None,
        }
    }

    /// A call that failed: the model reads `[code] detail`, so the code
    /// reaches it with the reason rather than as a bare string.
    #[must_use]
    pub fn failed(code: impl Into<ErrorCode>, detail: &str) -> Self {
        let code = code.into();
        Self {
            text: format!("[{code}] {detail}"),
            exit_code: None,
            error_code: Some(code),
            image: None,
            asks: None,
        }
    }

    /// A call that asks the person `question`: the model reads the answer,
    /// once the embedder resumes the run with it.
    #[must_use]
    pub fn asks(question: impl Into<String>) -> Self {
        let question = question.into();
        Self {
            text: question.clone(),
            exit_code: None,
            error_code: None,
            image: None,
            asks: Some(question),
        }
    }

    /// A call to `name`, which the loop holds no tool for. The model reads
    /// it as a tool error and the next step still runs.
    #[must_use]
    pub fn not_offered(name: &str) -> Self {
        Self::failed(ErrorCode::NOT_OFFERED, &format!("{name} {NOT_OFFERED}"))
    }

    /// A call to `name` from a step the provider cut at its output limit:
    /// its arguments may be incomplete, so it is never run.
    #[must_use]
    pub fn cut(name: &str) -> Self {
        Self::failed(ErrorCode::OUTPUT_LIMIT_REACHED, &format!("{name} {CUT}"))
    }

    /// The same output, with the image the call read.
    #[must_use]
    pub fn with_image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }
}

/// One tool as the model is offered it: provider-neutral, so each wire maps
/// it onto its own function-calling shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec<'a> {
    /// The name the model calls it by.
    pub name: &'a str,
    /// What the tool does, as the model reads it.
    pub description: &'a str,
    /// The arguments' JSON Schema.
    pub parameters: &'a Value,
}

/// What the model is told about a tool, which needs no call's context.
pub trait Describe: Send + Sync + fmt::Debug {
    /// The tool, as the model is offered it.
    fn spec(&self) -> ToolSpec<'_>;

    /// The name the model calls it by.
    fn name(&self) -> &str {
        self.spec().name
    }
}

/// A tool the loop can call, lent `Cx` for one call.
///
/// The future is written out rather than declared `async`: a call borrows
/// what `Cx` lends for as long as it runs, and only the explicit `'a` says so
/// for every lender at once, which a boxed `async` trait method cannot.
pub trait Tool<Cx>: Describe {
    /// Runs one call. A failure the model caused, or one upstream, is an
    /// output with an error code: the loop goes on and the model reads why.
    fn call<'a>(&'a self, arguments: &'a Value, cx: Cx) -> ToolFuture<'a>
    where
        Cx: 'a;
}

/// The tools one loop holds: those offered to the model, in the order it is
/// told them, and those held unoffered.
///
/// `T` is the embedder's tool object type, such as
/// `dyn for<'call> Tool<&'call Loop>`. An unoffered tool is never named to
/// the model, and a call the model makes to it anyway still reaches it, so
/// the tool answers in its own words why it is unavailable: Codex's hidden
/// exposure.
pub struct Tools<T: ?Sized> {
    offered: Vec<Box<T>>,
    unoffered: Vec<Box<T>>,
}

impl<T: Describe + ?Sized> Tools<T> {
    /// A loop's tools, `offered` in the order the model is told them.
    #[must_use]
    pub fn new(offered: Vec<Box<T>>) -> Self {
        Self {
            offered,
            unoffered: Vec::new(),
        }
    }

    /// Offers `tool` after the others.
    pub fn offer(&mut self, tool: Box<T>) {
        self.offered.push(tool);
    }

    /// Holds `tool` without offering it.
    pub fn hold(&mut self, tool: Box<T>) {
        self.unoffered.push(tool);
    }

    /// The names offered, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.offered.iter().map(|tool| tool.name())
    }

    /// The specs offered, in order.
    pub fn specs(&self) -> impl Iterator<Item = ToolSpec<'_>> {
        self.offered.iter().map(|tool| tool.spec())
    }

    /// The specs held unoffered, in the order they were held: what a
    /// deferred tool's search looks through, and what an embedder offers
    /// once a search loaded it.
    pub fn held_specs(&self) -> impl Iterator<Item = ToolSpec<'_>> {
        self.unoffered.iter().map(|tool| tool.spec())
    }

    /// Runs one call to `name`, lending it `cx`; a name the loop holds no tool
    /// for reads [`ToolOutput::not_offered`].
    ///
    /// Not an `async fn`: it hands back the tool's own future, so what `cx`
    /// lends goes straight into it, and a refusal is ready at once.
    pub fn dispatch<'a, Cx: 'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
        cx: Cx,
    ) -> ToolFuture<'a>
    where
        T: Tool<Cx>,
    {
        match self.find(name) {
            Some(tool) => tool.call(arguments, cx),
            None => Box::pin(ready(ToolOutput::not_offered(name))),
        }
    }

    /// The tool named `name`: an offered one first.
    fn find(&self, name: &str) -> Option<&T> {
        self.offered
            .iter()
            .chain(&self.unoffered)
            .map(Box::as_ref)
            .find(|tool| tool.name() == name)
    }
}

impl<T: Describe + ?Sized> fmt::Debug for Tools<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unoffered: Vec<&str> = self.unoffered.iter().map(|tool| tool.name()).collect();
        f.debug_struct("Tools")
            .field("offered", &self.names().collect::<Vec<_>>())
            .field("unoffered", &unoffered)
            .finish()
    }
}

#[cfg(test)]
mod tests;
