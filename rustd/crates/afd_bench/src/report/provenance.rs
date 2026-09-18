//! Where a result came from, and whether two results may be compared at all.
//!
//! # A number without provenance grades nothing
//!
//! A result file is read months after the run that made it, by a rubric
//! deciding whether the datastore change was safe. Two files whose numbers
//! differ are evidence only if everything else about the two runs was the
//! same — the same code, the same server build, the same kind of target. Absent
//! that, a 20% regression is as likely to be a different Dragonfly image as a
//! different daemon, and the comparison is not weak, it is meaningless.
//!
//! So every field here REFUSES to be absent:
//! a lane that grades nothing and a lane that grades green must never look
//! alike. [`knobs::required`] raises `VariableUnset` naming the variable, and
//! the run ends before it measures anything.
//!
//! # Ownership is a claim the caller makes, and the grader re-checks
//!
//! `owned` says this run had the target to itself — nobody else's traffic in
//! the numbers, and a reset the run was entitled to perform. It is recorded
//! rather than inferred because only the caller knows: the rig is owned by
//! construction, and a deployed endpoint is not owned no matter what it
//! answers. The grader refuses saturation and fault-injection evidence that
//! was not owned, which is invariant 7 of the spec.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::knobs::{self, Lookup};

/// Variable carrying the commit the lane binary was built from.
pub const REVISION_VARIABLE: &str = "BENCH_REVISION";

/// Variable carrying the datastore image this run drove.
pub const DATASTORE_IMAGE_VARIABLE: &str = "BENCH_DATASTORE_IMAGE";

/// Variable carrying whether the run owns its target outright.
pub const OWNED_VARIABLE: &str = "BENCH_TARGET_OWNED";

/// The one value of [`OWNED_VARIABLE`] that claims ownership.
///
/// A specific word rather than any non-empty value, for the reason
/// `BENCH_LOAD_PRODUCTION` spells its own: an exported leftover would
/// otherwise turn a shared endpoint into an owned one, and ownership is what
/// entitles a run to reset and to inject faults.
pub const OWNED_VALUE: &str = "owned";

/// What every result carries so a later reader can decide what it proves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The commit the lane binary was built from.
    pub revision: String,
    /// The datastore build this run drove, by tag or digest.
    pub datastore_image: String,
    /// Whether this run had its target to itself.
    pub owned: bool,
}

impl Provenance {
    /// Read the provenance of the run now starting.
    ///
    /// # Errors
    ///
    /// [`Error::VariableUnset`](crate::error::Error::VariableUnset) naming the
    /// first variable that is absent or blank. Called before a lane measures
    /// anything, so an incomplete environment costs no run.
    pub fn read(env: Lookup<'_>) -> Result<Self> {
        Ok(Self {
            revision: knobs::required(env, REVISION_VARIABLE)?,
            datastore_image: knobs::required(env, DATASTORE_IMAGE_VARIABLE)?,
            // Anything other than the exact word is not ownership. An unset
            // variable is still required to be SET — to the word or to
            // something else — so that "shared" is a statement somebody made
            // rather than a variable somebody forgot.
            owned: knobs::required(env, OWNED_VARIABLE)? == OWNED_VALUE,
        })
    }
}

/// Provenance for a test whose subject is not provenance.
///
/// `cfg(test)` and not a `Default`: a default would let a production path build
/// a result that describes no run at all, which is the single thing this type
/// exists to prevent. A test that IS about provenance builds its own fields.
#[cfg(test)]
impl Provenance {
    pub(crate) fn for_test() -> Self {
        Self {
            revision: "0000000000000000000000000000000000000000".to_owned(),
            datastore_image: "dragonfly:test".to_owned(),
            owned: true,
        }
    }
}

#[cfg(test)]
mod tests;
