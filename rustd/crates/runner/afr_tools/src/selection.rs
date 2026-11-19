//! The tools one lease, or one child loop, was offered.
//!
//! A selection is what a policy named, in the order it named it: the
//! catalog's handlers, the nested tools the loop runs itself, and the entries
//! the provider hosts, borrowed for the run. A child's selection narrows its
//! parent's, so a child never holds a tool its parent lacks.

use guffbolt::agents::Narrow;

use crate::catalog::Entry;
use crate::nested::{NESTED, Nested};
use crate::runtime::{CatalogTool, Runtime};

/// One tool a selection offers the model as a function.
#[derive(Debug, Clone, Copy)]
pub enum Offered<'c> {
    /// A handler the catalog hosts, run where its published entry says.
    Handler(&'static Entry, &'c CatalogTool),
    /// A nested tool, which the loop runs beside the turn.
    Nested(Nested),
}

impl Offered<'_> {
    /// The published entry it serves.
    #[must_use]
    pub const fn entry(self) -> &'static Entry {
        match self {
            Self::Handler(entry, _handler) => entry,
            Self::Nested(nested) => nested.entry(),
        }
    }
}

/// The tools one lease is offered: the functions the loop runs, and the tools
/// the provider hosts.
#[derive(Debug, Clone, Default)]
pub struct Selection<'c> {
    pub(crate) tools: Vec<Offered<'c>>,
    pub(crate) hosted: Vec<&'static Entry>,
}

impl<'c> Selection<'c> {
    /// Whether any offered tool runs inside the sandbox; a lease with none
    /// starts no sandbox.
    #[must_use]
    pub fn needs_sandbox(&self) -> bool {
        self.tools
            .iter()
            .any(|offered| offered.entry().runtime() == Runtime::Sandbox)
    }

    /// The handler for `name`, when the lease was offered one.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&'c CatalogTool> {
        match self.find(name)? {
            Offered::Handler(_entry, handler) => Some(handler),
            Offered::Nested(_nested) => None,
        }
    }

    /// Whether `name` is one of the provider-hosted tools offered.
    #[must_use]
    pub fn hosts(&self, name: &str) -> bool {
        self.hosted_entry(name).is_some()
    }

    /// The functions the lease was offered, in the order its policy named
    /// them.
    pub fn offered(&self) -> impl Iterator<Item = Offered<'c>> {
        self.tools.iter().copied()
    }

    /// The provider-hosted tools offered.
    #[must_use]
    pub fn hosted(&self) -> &[&'static Entry] {
        &self.hosted
    }

    /// The same selection without every tool `dropped` names.
    #[must_use]
    pub fn without(&self, dropped: &[&Entry]) -> Self {
        let kept = |entry: &Entry| !dropped.iter().any(|gone| **gone == *entry);
        Self {
            tools: (self.tools.iter().copied())
                .filter(|offered| kept(offered.entry()))
                .collect(),
            hosted: (self.hosted.iter().copied())
                .filter(|entry| kept(entry))
                .collect(),
        }
    }

    /// The function named `name`, when it is one offered.
    fn find(&self, name: &str) -> Option<Offered<'c>> {
        self.offered()
            .find(|offered| offered.entry().name() == name)
    }

    /// The hosted entry named `name`, when it is one offered.
    fn hosted_entry(&self, name: &str) -> Option<&'static Entry> {
        self.hosted
            .iter()
            .copied()
            .find(|entry| entry.name() == name)
    }

    pub(crate) fn offers(&self, name: &str) -> bool {
        self.find(name).is_some() || self.hosts(name)
    }
}

/// A child's selection narrows its parent's, hosted entries included, and
/// one at the depth cap holds none of the six nested tools.
impl Narrow for Selection<'_> {
    fn narrowed<'n, N: AsRef<str>>(&self, names: &'n [N]) -> Result<Self, &'n str> {
        let mut narrowed = Self::default();
        for name in names.iter().map(AsRef::as_ref) {
            if narrowed.offers(name) {
                continue;
            }
            match (self.find(name), self.hosted_entry(name)) {
                (Some(offered), _) => narrowed.tools.push(offered),
                (None, Some(entry)) => narrowed.hosted.push(entry),
                (None, None) => return Err(name),
            }
        }
        Ok(narrowed)
    }

    fn without_agents(&self) -> Self {
        self.without(&NESTED)
    }
}
