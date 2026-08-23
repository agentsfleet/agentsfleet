//! The resolved per-execution policy that travels on a lease.
//!
//! Set once per execution and invariant for its lifetime: where the run may
//! reach on the network, which tools it may call, the inline secrets, and the
//! context budget. Pure data — nothing here interprets a value.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Provider-name prefix routing a custom OpenAI-compatible endpoint through the
/// compatible-provider path with a configured base URL.
///
/// Never the literal `openai`, which is pinned to its own host and silently
/// drops the base URL.
pub const CUSTOM_PROVIDER_PREFIX: &str = "custom:";

/// Per-execution egress policy. An outbound request must match an entry in
/// `allow` by exact hostname; an empty `allow` denies everything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPolicy<'a> {
    /// Exact hostnames the run may reach.
    #[serde(borrow)]
    pub allow: Vec<Cow<'a, str>>,
    /// Restrict tool requests to safe reads, keeping a credential from crossing
    /// to a host other than its own declared endpoint.
    pub read_only: bool,
    /// Exact prefixes where a read-only tool may use POST for a query.
    #[serde(borrow)]
    pub read_post_paths: Vec<Cow<'a, str>>,
}

/// Methods a daemon-authored request rule may express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    /// Read a resource.
    Get,
    /// Read a resource's headers only.
    Head,
    /// Submit a query body against a read-only endpoint.
    Post,
}

/// Whether a request path must equal the authored bytes or begin with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpPathMatch {
    /// The path must equal the rule's path exactly.
    Exact,
    /// The path must begin with the rule's path.
    Prefix,
}

/// One required top-level JSON field, with exactly one expected value set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpJsonFieldRule<'a> {
    /// The field name the rule locks.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Expected string value, when the rule locks a string.
    #[serde(borrow)]
    pub string_value: Option<Cow<'a, str>>,
    /// Expected boolean value, when the rule locks a boolean.
    pub boolean_value: Option<bool>,
}

/// One method and path admitted at an origin.
///
/// JSON rules lock selected fields; any field the rules do not name stays
/// available for request-specific content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRequestRule<'a> {
    /// The method this rule admits.
    pub method: HttpMethod,
    /// The path this rule admits.
    #[serde(borrow)]
    pub path: Cow<'a, str>,
    /// Whether `path` is matched exactly or as a prefix.
    pub path_match: HttpPathMatch,
    /// Fields whose values the rule locks.
    #[serde(borrow)]
    pub json_fields: Vec<HttpJsonFieldRule<'a>>,
}

/// The provider-neutral request boundary for one exact host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpOriginPolicy<'a> {
    /// The host this boundary governs.
    #[serde(borrow)]
    pub host: Cow<'a, str>,
    /// Credentials admitted at this host.
    #[serde(borrow)]
    pub credential_names: Vec<Cow<'a, str>>,
    /// Requests admitted at this host.
    #[serde(borrow)]
    pub requests: Vec<HttpRequestRule<'a>>,
}

/// One credential the lease grants the run permission to mint on demand.
///
/// Carries the integration id ONLY — never a stored handle and never a token.
/// The child mints a short-lived token at the tool boundary through the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mintable<'a> {
    /// The placeholder name the fleet references.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The integration the broker mints for. The workspace is server-derived.
    #[serde(borrow)]
    pub integration: Cow<'a, str>,
}

/// Whether a bound repository may be written or only read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryAccess {
    /// Clone and read only.
    Read,
    /// Clone, read, and push.
    Write,
}

/// The repositories a run is bound to, and how it may use them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryBinding<'a> {
    /// Repositories the run may reach.
    #[serde(borrow)]
    pub repositories: Vec<Cow<'a, str>>,
    /// Whether the run may write to them.
    pub access: RepositoryAccess,
    /// Branch a write is based on.
    #[serde(borrow)]
    pub base_branch: Cow<'a, str>,
}

/// Context-budget knobs. `model` and `context_cap_tokens` are upstream-populated
/// passthrough — the runner does not interpret `model`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBudget<'a> {
    /// Tool results retained in the working window.
    pub tool_window: u32,
    /// How often a run checkpoints its memory.
    pub memory_checkpoint_every: u32,
    /// Fill fraction at which a stage chunks rather than growing.
    pub stage_chunk_threshold: f32,
    /// The active model, carried through without interpretation.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// The active model's context cap in tokens, or zero when unresolved.
    pub context_cap_tokens: u32,
}

/// Everything a single execution is permitted to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy<'a> {
    /// Where the run may reach on the network.
    #[serde(borrow)]
    pub network_policy: NetworkPolicy<'a>,
    /// Tools the run may call.
    #[serde(borrow)]
    pub tools: Vec<Cow<'a, str>>,
    /// Static secrets substituted at the tool boundary. Free-form by design.
    pub secrets_map: Option<serde_json::Value>,
    /// Credentials the run may mint on demand.
    #[serde(borrow)]
    pub mintable: Vec<Mintable<'a>>,
    /// Inference provider name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider credential for this run. Secret — never logged, never echoed.
    #[serde(borrow)]
    pub api_key: Cow<'a, str>,
    /// Inference host to dial.
    #[serde(borrow)]
    pub inference_host: Cow<'a, str>,
    /// Base URL override for a compatible provider.
    #[serde(borrow)]
    pub base_url: Option<Cow<'a, str>>,
    /// Repositories the run is bound to, when it is bound to any.
    #[serde(borrow)]
    pub repository_binding: Option<RepositoryBinding<'a>>,
    /// Per-host request boundaries.
    #[serde(borrow)]
    pub http_origin_policies: Vec<HttpOriginPolicy<'a>>,
    /// Context-budget knobs for the run.
    #[serde(borrow)]
    pub context: ContextBudget<'a>,
}
