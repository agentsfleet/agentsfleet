//! Debug output never includes private history or model-authored notes.
use crate::{Checkpoint, Group, Identity, Summary};
use std::fmt;

impl fmt::Debug for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Summary").finish_non_exhaustive()
    }
}
impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group")
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for Checkpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Checkpoint")
            .field("generation", &self.generation)
            .field("covered_through", &self.covered_through)
            .finish_non_exhaustive()
    }
}
