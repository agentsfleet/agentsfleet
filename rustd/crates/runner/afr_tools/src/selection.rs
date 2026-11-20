//! The tools one run was offered.
//!
//! A selection is what a policy named, in the order it named it: the
//! catalog's handlers, the tools the run builds over its own backends,
//! `guffbolt`'s subagent tools, and the entries the provider hosts, borrowed
//! for the run. A child loop holds its parent's tools or fewer; `guffbolt`
//! narrows them, so a selection is the run's alone.

use crate::catalog::Entry;
use crate::runtime::{CatalogTool, Runtime};

/// One tool a selection offers the model as a function.
#[derive(Debug, Clone, Copy)]
pub enum Offered<'c> {
    /// A handler the catalog hosts, run where its published entry says.
    Handler(&'static Entry, &'c CatalogTool),
    /// A tool the run builds over its own backends when it starts
    /// ([`crate::RunTools`]).
    Backed(&'static Entry),
    /// One of `guffbolt`'s subagent tools, by `guffbolt`'s own name: the run
    /// builds it over its children, and it runs beside the loop.
    Built(&'static str),
}

impl Offered<'_> {
    /// The name the model calls it by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Handler(entry, _) | Self::Backed(entry) => entry.name(),
            Self::Built(name) => name,
        }
    }

    /// Where it runs.
    #[must_use]
    pub const fn runtime(self) -> Runtime {
        match self {
            Self::Handler(entry, _) | Self::Backed(entry) => entry.runtime(),
            Self::Built(_) => Runtime::Supervisor,
        }
    }
}

/// The tools one run is offered: the functions the loop runs, and the tools
/// the provider hosts.
#[derive(Debug, Clone, Default)]
pub struct Selection<'c> {
    pub(crate) tools: Vec<Offered<'c>>,
    pub(crate) hosted: Vec<&'static Entry>,
}

impl<'c> Selection<'c> {
    /// Whether any offered tool runs inside the sandbox; a run with none
    /// starts no sandbox.
    #[must_use]
    pub fn needs_sandbox(&self) -> bool {
        self.tools
            .iter()
            .any(|offered| offered.runtime() == Runtime::Sandbox)
    }

    /// The catalog's handler for `name`, when the run was offered one.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&'c CatalogTool> {
        match self.find(name)? {
            Offered::Handler(_entry, handler) => Some(handler),
            Offered::Backed(_) | Offered::Built(_) => None,
        }
    }

    /// Whether `name` is one of the provider-hosted tools offered.
    #[must_use]
    pub fn hosts(&self, name: &str) -> bool {
        self.hosted.iter().any(|entry| entry.name() == name)
    }

    /// Whether the run was offered `name`, as a function or as a tool the
    /// provider hosts.
    #[must_use]
    pub fn offers(&self, name: &str) -> bool {
        self.find(name).is_some() || self.hosts(name)
    }

    /// The functions the run was offered, in the order its policy named
    /// them.
    pub fn offered(&self) -> impl Iterator<Item = Offered<'c>> {
        self.tools.iter().copied()
    }

    /// The provider-hosted tools offered.
    #[must_use]
    pub fn hosted(&self) -> &[&'static Entry] {
        &self.hosted
    }

    /// The function named `name`, when it is one offered.
    fn find(&self, name: &str) -> Option<Offered<'c>> {
        self.offered().find(|offered| offered.name() == name)
    }
}
