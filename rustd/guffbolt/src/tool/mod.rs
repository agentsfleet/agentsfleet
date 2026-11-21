//! The tools a loop offers the model, and what one call hands back.
//!
//! A tool tells the model what it is without any call's context
//! ([`Describe`]) and runs a call with the context its embedder lends it
//! ([`Tool`]), a tool's spec kept apart from its executor. [`Tools`] is the
//! set one loop holds: it looks a call up by name and answers a name it holds
//! no tool for with the harness's own refusal, so no embedder writes that
//! sentence twice.

mod output;

use std::fmt;
use std::future::ready;
use std::pin::Pin;

use serde_json::Value;

pub use self::output::{ErrorCode, Image, ImageKind, ToolOutput};

/// One call's work, as a tool hands it to the loop.
pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = ToolOutput> + Send + 'a>>;

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

/// A boxed tool describes itself as the tool it holds, so an embedder can wrap
/// a set it was built as boxes.
impl<T: Describe + ?Sized> Describe for Box<T> {
    fn spec(&self) -> ToolSpec<'_> {
        (**self).spec()
    }

    fn name(&self) -> &str {
        (**self).name()
    }
}

/// A boxed tool runs as the tool it holds.
impl<Cx, T: Tool<Cx> + ?Sized> Tool<Cx> for Box<T> {
    fn call<'a>(&'a self, arguments: &'a Value, cx: Cx) -> ToolFuture<'a>
    where
        Cx: 'a,
    {
        (**self).call(arguments, cx)
    }
}

/// The tools one loop holds: those offered to the model, in the order it is
/// told them, and those held unoffered.
///
/// `T` is the embedder's tool object type, such as
/// `dyn for<'call> Tool<&'call Loop>`. An unoffered tool is never named to
/// the model, and a call the model makes to it anyway still reaches it, so
/// the tool answers in its own words why it is unavailable.
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
        self.offered.iter().map(Describe::name)
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

    /// The tools offered, in order, for an embedder that lends its calls a
    /// context of its own and so wraps each tool in its own type.
    #[must_use]
    pub fn into_offered(self) -> Vec<Box<T>> {
        self.offered
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
        let unoffered: Vec<&str> = self.unoffered.iter().map(Describe::name).collect();
        f.debug_struct("Tools")
            .field("offered", &self.names().collect::<Vec<_>>())
            .field("unoffered", &unoffered)
            .finish()
    }
}

#[cfg(test)]
mod tests;
