//! The attribute keys this daemon emits, spelled once.
//!
//! Mirrors `observability/semconv.zig`, which exists for the reason this does:
//! a key that differs by one character between two emitters is two series in
//! the backend, and nothing reports that as an error.
//!
//! # What is here, and what deliberately is not
//!
//! Keys and the closed values a key is spelled WITH. Not family names — those
//! live in `docs/metrics.census.tsv`, which is the single source of truth for
//! the export, and a second copy here would be a second thing to drift. The
//! Zig file carries both because it has no census reader; this crate has one.
//!
//! Codes and attributes are still added as the milestone that emits them
//! lands. What arrives here now is exactly what the delivery span carries and
//! what the census's label columns name — everything else stays out until it
//! has a caller, because an unreferenced constant is dead code that looks like
//! coverage (RULE NDC).

pub mod provider;

#[cfg(test)]
mod tests;

/// The instrumentation scope, and the default `service.name`.
pub const SCOPE_NAME: &str = "agentsfleetd";

/// The namespace every signal from this product carries.
pub const SERVICE_NAMESPACE: &str = "agentsfleet";

/// Resource key for the namespace above.
pub const RESOURCE_SERVICE_NAMESPACE: &str = "service.namespace";

/// The request method, upper-case, as OpenTelemetry spells it.
pub const ATTR_HTTP_REQUEST_METHOD: &str = "http.request.method";

/// The matched route TEMPLATE — never a concrete path.
///
/// This is the low-cardinality half of the contract. A real path carries
/// workspace, fleet and lease identifiers, so exporting one would put tenant
/// identity into span attributes AND give the backend a distinct route value
/// per request, which is the same thing as having no route dimension at all.
pub const ATTR_HTTP_ROUTE: &str = "http.route";

/// The response status code.
pub const ATTR_HTTP_RESPONSE_STATUS_CODE: &str = "http.response.status_code";

// ---------------------------------------------------------------------------
// The delivery span's vocabulary
// ---------------------------------------------------------------------------

/// The span a settled control-plane delivery is recorded under.
///
/// A CUSTOM span, not a GenAI client span: the runner produces none and
/// propagates no trace context, so this process cannot honestly claim to be one
/// half of a distributed agent trace.
pub const SPAN_FLEET_DELIVERY: &str = "fleet.delivery";

/// The operation this process observes: one sandboxed agent invocation.
pub const OPERATION_INVOKE_AGENT: &str = "invoke_agent";

/// Which operation a signal describes.
pub const ATTR_OPERATION_NAME: &str = "gen_ai.operation.name";

/// The agent identity behind a run.
///
/// The FLEET, in this product: one fleet is one agent identity, which is why
/// nothing here carries a second identifier for the same fact.
pub const ATTR_AGENT_ID: &str = "gen_ai.agent.id";

/// The model vendor, in OpenTelemetry's own well-known spelling.
///
/// Never a configured spelling — see [`provider::normalize`] for why a vendor
/// this daemon cannot map is omitted rather than exported as though standard.
pub const ATTR_PROVIDER_NAME: &str = "gen_ai.provider.name";

/// The model a run was issued against, exactly as the catalogue spells it.
pub const ATTR_REQUEST_MODEL: &str = "gen_ai.request.model";

/// Prompt tokens for the whole run, cached input included.
pub const ATTR_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";

/// Completion tokens for the whole run.
pub const ATTR_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";

/// Whether the sandbox was this platform's or the tenant's own.
pub const ATTR_EXECUTION_POSTURE: &str = "agentsfleet.execution.posture";

/// The workspace a run belongs to.
pub const ATTR_WORKSPACE_ID: &str = "agentsfleet.workspace.id";

/// The tenant whose wallet a run drew on.
pub const ATTR_TENANT_ID: &str = "agentsfleet.tenant.id";

/// The event a run executed.
pub const ATTR_EVENT_ID: &str = "agentsfleet.event.id";

/// Which direction a token count is: prompt, or completion.
pub const ATTR_TOKEN_TYPE: &str = "gen_ai.token.type";

/// Which class of debit a charge was.
pub const ATTR_CHARGE_TYPE: &str = "agentsfleet.billing.charge.type";

/// The coarse verdict on a run that did not finish cleanly.
///
/// Absent on success. The granular failure class stays off this key on
/// purpose: it multiplies the per-model series budget, and the durable event
/// row already carries it exactly.
pub const ATTR_ERROR_TYPE: &str = "error.type";

/// Every key the fleet-delivery span carries, in the order it writes them.
///
/// Declared rather than derived so the span's shape is a value a test can hold
/// against what the span actually emitted. A key added to the emitter and not
/// here — or here and not emitted — fails `the_delivery_span_carries_every_declared_key`.
pub const DELIVERY_SPAN_KEYS: &[&str] = &[
    ATTR_OPERATION_NAME,
    ATTR_AGENT_ID,
    ATTR_PROVIDER_NAME,
    ATTR_REQUEST_MODEL,
    ATTR_USAGE_INPUT_TOKENS,
    ATTR_USAGE_OUTPUT_TOKENS,
    ATTR_EXECUTION_POSTURE,
    ATTR_WORKSPACE_ID,
    ATTR_TENANT_ID,
    ATTR_EVENT_ID,
];

// ---------------------------------------------------------------------------
// The census's label columns
// ---------------------------------------------------------------------------

/// Why something was refused, dropped, or suppressed.
pub const LABEL_REASON: &str = "reason";

/// Which of the three OTLP signals a measurement is about.
pub const LABEL_SIGNAL: &str = "signal";

/// Which attribute an omission was about, by its own wire key.
pub const LABEL_ATTRIBUTE: &str = "attribute";

/// The runner a measurement is attributed to, or the overflow bucket.
///
/// The one caller-supplied label value in the whole contract, which is why
/// [`crate::runner`] admits it against a fixed slot table before it is written.
pub const LABEL_RUNNER_ID: &str = "runner_id";

/// How an operation ended.
pub const LABEL_OUTCOME: &str = "outcome";

/// Which library read surface a measurement is about.
pub const LABEL_SURFACE: &str = "surface";

/// Which stage of a library read a measurement times.
pub const LABEL_STAGE: &str = "stage";

/// How a pool acquire ended.
///
/// Distinct from [`LABEL_OUTCOME`] because a starving pool is a process-wide
/// fact and an outcome is a request's: sharing one key would let a dashboard
/// add them up.
pub const LABEL_POOL_RESULT: &str = "pool_result";

/// What the catalogue cache did for a read.
pub const LABEL_CACHE: &str = "cache";

/// Every label key the census's own columns name.
///
/// The vocabulary's completeness claim, and what
/// `every_census_label_resolves_to_a_constant` grades the census against in
/// both directions: a column with no constant here is a string literal about
/// to be written by hand, and a constant with no column is a key nothing
/// exports.
pub const CENSUS_LABEL_KEYS: &[&str] = &[
    ATTR_REQUEST_MODEL,
    LABEL_REASON,
    LABEL_SIGNAL,
    LABEL_ATTRIBUTE,
    LABEL_RUNNER_ID,
    LABEL_OUTCOME,
    LABEL_SURFACE,
    LABEL_STAGE,
    LABEL_POOL_RESULT,
    LABEL_CACHE,
];
