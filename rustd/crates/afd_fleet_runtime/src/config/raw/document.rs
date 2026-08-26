//! The document's two levels: the top, and the `x-agentsfleet` block under it.
//!
//! Both carry a flattened `extra`, which is what lets [`super::super`] tell a
//! runtime key authored one level too high apart from a key that is simply
//! misspelled — two different author mistakes with two different fixes.

use garde::Validate;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::predicate::{is_branch_name, is_repository, is_token};
use super::{
    Access, Budget, Context, Gates, MAX_BASE_BRANCH_LEN, MAX_CREDENTIAL_LEN, MAX_CREDENTIALS,
    MAX_REFERENCE_LEN, MAX_REPOSITORIES, MAX_REPOSITORY_LEN, MAX_TOOL_LEN, MAX_TOOLS, Network,
    Trigger,
};

///
/// `name` is the only authored key outside the namespaced block; anything else
/// found here is either a runtime key that needs indenting or a stray.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Document {
    /// The fleet's authored name. Its SHAPE is checked by `FleetName`, which is
    /// a stronger statement than a length bound; the bound here only stops an
    /// absurd value reaching that parser.
    #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
    pub(crate) name: Option<String>,
    /// The namespaced block every runtime key lives under.
    #[serde(rename = "x-agentsfleet")]
    #[garde(dive)]
    pub(crate) runtime: Option<Runtime>,
    /// Every top-level key that is neither of the above.
    #[serde(flatten)]
    #[garde(skip)]
    pub(crate) extra: Map<String, Value>,
}

/// The `x-agentsfleet` block.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Runtime {
    /// What may wake this fleet.
    ///
    /// `dive` alone: garde refuses to combine it with `inner`, so the SET's
    /// arity is proved beside its uniqueness rule in `trigger::parse_set`,
    /// where both are one question — "is this a coherent set" — rather than
    /// split across two layers.
    #[garde(dive)]
    pub(crate) triggers: Option<Vec<Trigger>>,
    /// Which tools it may dispatch.
    ///
    /// May name nothing: a fleet that dispatches no tool is one that only
    /// answers, which is a legitimate thing to author.
    #[garde(inner(
        length(max = MAX_TOOLS),
        inner(length(chars, min = 1, max = MAX_TOOL_LEN), custom(is_token))
    ))]
    pub(crate) tools: Option<Vec<String>>,
    /// Which secrets it may read. Their CHARSET is `CredentialName`'s.
    #[garde(inner(length(max = MAX_CREDENTIALS), inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN))))]
    pub(crate) credentials: Option<Vec<String>>,
    /// Where it may reach on the network.
    #[garde(dive)]
    pub(crate) network: Option<Network>,
    /// What it may spend.
    #[garde(dive)]
    pub(crate) budget: Option<Budget>,
    /// Which actions need a human, and which patterns kill the run.
    #[garde(dive)]
    pub(crate) gates: Option<Gates>,
    /// A skill reference, stored but not resolved.
    #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
    pub(crate) skill: Option<String>,
    /// An opaque model identifier, passed through untouched.
    #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
    pub(crate) model: Option<String>,
    /// Context-budget overrides.
    #[garde(dive)]
    pub(crate) context: Option<Context>,
    /// Which repositories its credentials may reach.
    ///
    /// Refuses an empty list: a binding that names nothing is not "every
    /// repository", and a token scoped to nothing cannot mint.
    #[garde(inner(
        length(min = 1, max = MAX_REPOSITORIES),
        inner(length(chars, min = 1, max = MAX_REPOSITORY_LEN), custom(is_repository))
    ))]
    pub(crate) repositories: Option<Vec<String>>,
    /// How far that reach goes.
    #[garde(skip)]
    pub(crate) repository_access: Option<Access>,
    /// The trusted base branch a write binding opens against.
    #[garde(inner(length(chars, min = 1, max = MAX_BASE_BRANCH_LEN), custom(is_branch_name)))]
    pub(crate) repository_base: Option<String>,
    /// Every key under the block that is none of the above.
    #[serde(flatten)]
    #[garde(skip)]
    pub(crate) extra: Map<String, Value>,
}
