//! The resolved per-execution policy that travels on a lease.
//!
//! Set once per execution and invariant for its lifetime: where the run may
//! reach on the network, which tools it may call, the inline secrets, and the
//! context budget. Pure data — nothing here interprets a value.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

mod http_egress;
pub mod repository;

pub use http_egress::{HttpEgress, PACKAGE_MANAGER_DOMAINS};

/// Provider-name prefix routing a custom OpenAI-compatible endpoint through the
/// compatible-provider path with a configured base URL.
///
/// Never the literal `openai`, which is pinned to its own host and silently
/// drops the base URL.
pub const CUSTOM_PROVIDER_PREFIX: &str = "custom:";

/// Allowed hosts for HTTP tools.
///
/// Restricted mode with no allowed hosts blocks these requests. Sandbox network access also depends on the
/// runner's network policy and registry allowlist.
// Keeps the bare name: `runner::NetworkPolicy` is a different shape with the
// same Rust name and publishes as `RunnerNetworkPolicy`, the name the
// hand-written contract gave it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPolicy<'a> {
    /// HTTP destination scope; omitted means restricted to named hosts.
    #[serde(default)]
    pub http_egress: HttpEgress,
    /// Admit the standard package registry hosts through HTTP tools.
    #[serde(default)]
    pub package_manager_domains: bool,
    /// Exact hostnames HTTP tools may reach.
    #[serde(borrow)]
    pub allow: Vec<Cow<'a, str>>,
    /// Restrict tool requests to safe reads, keeping a credential from crossing
    /// to a host other than its own declared endpoint.
    pub read_only: bool,
    /// Exact prefixes where a read-only tool may use POST for a query.
    #[serde(borrow)]
    pub read_post_paths: Vec<Cow<'a, str>>,
}

/// HTTP methods allowed by a server-defined request rule.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    /// Read a resource.
    Get,
    /// Read a resource's headers only.
    Head,
    /// Submit a body at a server-defined endpoint.
    Post,
    /// Update an existing resource only at a server-defined exact path.
    Patch,
}

/// Whether a request path must equal the authored bytes or begin with them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpPathMatch {
    /// The path must equal the rule's path exactly.
    Exact,
    /// The path must begin with the rule's path.
    Prefix,
}

/// One required top-level JSON field, with exactly one expected value set.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// An allowed HTTP method and path.
///
/// Locked fields must match. When `permitted_fields` is present, only locked and listed
/// fields are allowed; query strings are refused. When absent, other fields and query strings
/// are allowed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Additional allowed field names. An empty list permits only locked fields. When
    /// omitted, other fields and query strings are allowed.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub permitted_fields: Option<Vec<Cow<'a, str>>>,
}

/// The provider-neutral request boundary for one exact host.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// An integration this run may request a short-lived credential for. Contains its identifier,
/// never a stored credential or token.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mintable<'a> {
    /// The placeholder name the fleet references.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The integration the broker mints for. The workspace is server-derived.
    #[serde(borrow)]
    pub integration: Cow<'a, str>,
}

/// Whether a bound repository may be written or only read.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryAccess {
    /// Clone and read only.
    Read,
    /// Clone, read, and push.
    Write,
}

/// The repositories a run is bound to, and how it may use them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextBudget<'a> {
    /// Tool results retained in the working window.
    pub tool_window: u32,
    /// How often a run checkpoints its memory.
    pub memory_checkpoint_every: u32,
    /// Fill fraction at which a stage chunks rather than growing: 0 to 1 of
    /// the context window.
    #[cfg_attr(feature = "openapi", schema(minimum = 0.0, maximum = 1.0))]
    pub stage_chunk_threshold: f32,
    /// The active model, carried through without interpretation.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// The fleet's operating context cap, or zero when unresolved.
    pub context_cap_tokens: u32,
}

/// Everything a single run is permitted to do.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPolicy<'a> {
    /// Where the run may reach on the network.
    #[serde(borrow)]
    pub network_policy: NetworkPolicy<'a>,
    /// Tools the run may call.
    #[serde(borrow)]
    pub tools: Vec<Cow<'a, str>>,
    /// Static secrets substituted at the tool boundary. Free-form by design.
    pub secrets_map: Option<serde_json::Value>,
    /// Integration credentials this run may request when needed.
    #[serde(borrow)]
    pub mintable: Vec<Mintable<'a>>,
    /// Inference provider name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Saved model inference settings, required for runtime provider connections.
    #[serde(default)]
    pub inference: Option<crate::inference::ModelInference>,
    /// Saved physical model context, independent of the fleet's operating cap.
    /// Zero is unresolved and cannot authorize a provider connection.
    #[serde(default)]
    pub model_context_tokens: u32,
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
