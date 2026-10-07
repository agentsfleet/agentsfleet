//! The tools one lease, or one child loop, was offered.
//!
//! A selection is the catalog's handlers and hosted entries a policy named,
//! borrowed for the run. A child's selection narrows its parent's, so a
//! child never holds a tool its parent lacks.

use crate::catalog::Entry;
use crate::runtime::{Runtime, Tool};

/// provider hosts.
#[derive(Debug, Clone, Default)]
pub struct Selection<'c> {
    pub(crate) tools: Vec<&'c dyn Tool>,
    pub(crate) hosted: Vec<&'static Entry>,
}

impl<'c> Selection<'c> {
    /// Whether any offered tool runs inside the sandbox; a lease with none
    /// starts no sandbox.
    #[must_use]
    pub fn needs_sandbox(&self) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.runtime() == Runtime::Sandbox)
    }

    /// The handler for `name`, when the lease was offered one.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&'c dyn Tool> {
        self.tools.iter().copied().find(|tool| tool.name() == name)
    }

    /// Whether `name` is one of the provider-hosted tools offered.
    #[must_use]
    pub fn hosts(&self, name: &str) -> bool {
        self.hosted_entry(name).is_some()
    }

    /// The handlers the lease was offered.
    pub fn tools(&self) -> impl Iterator<Item = &'c dyn Tool> + '_ {
        self.tools.iter().copied()
    }

    /// The provider-hosted tools offered.
    #[must_use]
    pub fn hosted(&self) -> &[&'static Entry] {
        &self.hosted
    }

    /// The same selection narrowed to `names`, for a child that may hold no
    /// more than its parent.
    ///
    /// # Errors
    /// The first name this selection does not offer; nothing is narrowed.
    pub fn narrowed<'n, S: AsRef<str>>(&self, names: &'n [S]) -> Result<Self, &'n str> {
        let mut narrowed = Self::default();
        for name in names.iter().map(AsRef::as_ref) {
            if narrowed.offers(name) {
                continue;
            }
            match (self.tool(name), self.hosted_entry(name)) {
                (Some(tool), _) => narrowed.tools.push(tool),
                (None, Some(entry)) => narrowed.hosted.push(entry),
                (None, None) => return Err(name),
            }
        }
        Ok(narrowed)
    }

    /// The same selection without every tool `dropped` names.
    #[must_use]
    pub fn without(&self, dropped: &[&Entry]) -> Self {
        let kept = |entry: &Entry| !dropped.iter().any(|gone| **gone == *entry);
        Self {
            tools: (self.tools.iter().copied())
                .filter(|tool| kept(tool.entry()))
                .collect(),
            hosted: (self.hosted.iter().copied())
                .filter(|entry| kept(entry))
                .collect(),
        }
    }

    /// The hosted entry named `name`, when it is one offered.
    fn hosted_entry(&self, name: &str) -> Option<&'static Entry> {
        self.hosted
            .iter()
            .copied()
            .find(|entry| entry.name() == name)
    }

    pub(crate) fn offers(&self, name: &str) -> bool {
        self.tool(name).is_some() || self.hosts(name)
    }
}
