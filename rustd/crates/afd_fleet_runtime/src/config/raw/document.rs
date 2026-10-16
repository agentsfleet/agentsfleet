//! The document's two levels: the top, and the `x-agentsfleet` block under it.
//!
//! Both carry a flattened `extra`, which is what lets [`super::super`] tell a
//! runtime key authored one level too high apart from a key that is simply
//! misspelled — two different author mistakes with two different fixes.
//!
//! Under the `openapi` feature these types also publish the Definition's
//! shape. Their `///` lines are that published description; notes for the
//! next maintainer stay in `//` comments, which the schema never reads.

use garde::Validate;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::predicate::{is_branch_name, is_repository, is_token};
use super::{
    Access, Budget, Context, Gates, MAX_BASE_BRANCH_LEN, MAX_CREDENTIAL_LEN, MAX_CREDENTIALS,
    MAX_REFERENCE_LEN, MAX_REPOSITORIES, MAX_REPOSITORY_LEN, MAX_TOOL_LEN, MAX_TOOLS, Network,
    Trigger,
};

/// The `config` member of a Definition: the fleet's name and its runtime settings.
//
// `name` is the only authored key outside the namespaced block; anything else
// found here is either a runtime key that needs indenting or a stray.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionConfig))]
pub(crate) struct Document {
    /// The fleet's name.
    // Its SHAPE is checked by `FleetName`, which is a stronger statement than
    // a length bound; the bound here only stops an absurd value reaching that
    // parser.
    #[garde(inner(length(chars, min = 1, max = MAX_REFERENCE_LEN)))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) name: Option<String>,
    /// Every runtime setting: triggers, tools, credentials, network, budget and gates.
    #[serde(rename = "x-agentsfleet")]
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) runtime: Option<Runtime>,
    // Every top-level key that is neither of the above. Ignored by the schema:
    // the parser refuses each one by name.
    #[serde(flatten)]
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub(crate) extra: Map<String, Value>,
}

/// The `x-agentsfleet` block, which holds every runtime setting.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionRuntime))]
pub(crate) struct Runtime {
    /// What may start a run of this fleet besides a message sent to it. Omit
    /// it, or leave it empty, for a fleet that is only ever talked to.
    // `dive` alone: garde refuses to combine it with `inner`, so the SET's
    // count is proved beside its uniqueness rule in `trigger::parse_set`,
    // where both are one question — "is this a coherent set" — rather than
    // split across two layers.
    // Absent is the empty set, and an explicit null is still refused: it is a
    // list or nothing at all.
    #[serde(default, deserialize_with = "crate::optional::present")]
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub(crate) triggers: Option<Vec<Trigger>>,
    /// The tools this fleet may call. An empty list means the fleet only answers.
    #[garde(inner(
        length(max = MAX_TOOLS),
        inner(length(chars, min = 1, max = MAX_TOOL_LEN), custom(is_token))
    ))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) tools: Option<Vec<String>>,
    /// The workspace secrets this fleet may read, by name.
    // Their CHARSET is `CredentialName`'s.
    #[garde(inner(length(max = MAX_CREDENTIALS), inner(length(chars, min = 1, max = MAX_CREDENTIAL_LEN))))]
    pub(crate) credentials: Option<Vec<String>>,
    /// The hosts this fleet may reach.
    #[garde(dive)]
    pub(crate) network: Option<Network>,
    /// The most this fleet may spend.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) budget: Option<Budget>,
    /// Which tool actions need a person's approval, and which repeated actions stop a run.
    #[garde(dive)]
    pub(crate) gates: Option<Gates>,
    /// The model this fleet runs on, passed to the runner unchanged.
    #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
    pub(crate) model: Option<String>,
    /// Context-size settings for each run.
    #[garde(dive)]
    pub(crate) context: Option<Context>,
    /// The repositories this fleet's GitHub credentials may reach, as `owner/name`.
    /// Set together with `repository_access`.
    // Refuses an empty list: a binding that names nothing is not "every
    // repository", and a token scoped to nothing cannot mint.
    #[garde(inner(
        length(min = 1, max = MAX_REPOSITORIES),
        inner(length(chars, min = 1, max = MAX_REPOSITORY_LEN), custom(is_repository))
    ))]
    pub(crate) repositories: Option<Vec<String>>,
    /// How far those credentials reach. Set together with `repositories`.
    #[garde(skip)]
    pub(crate) repository_access: Option<Access>,
    /// The branch every pull request opens against. Required with `write`, refused with `read`.
    #[garde(inner(length(chars, min = 1, max = MAX_BASE_BRANCH_LEN), custom(is_branch_name)))]
    pub(crate) repository_base: Option<String>,
    // Every key under the block that is none of the above. Ignored by the
    // schema: the parser refuses each one by name.
    #[serde(flatten)]
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub(crate) extra: Map<String, Value>,
}
