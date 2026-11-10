//! The attribute keys this daemon emits, spelled once.
//!
//! Spelled once because a key that differs by one character between two
//! emitters is two series in the backend, and nothing reports that as an
//! error.
//!
//! # What is here, and what deliberately is not
//!
//! Keys and the closed values a key is spelled WITH. Not family names — those
//! live in `docs/metrics.census.tsv`, which is the single source of truth for
//! the export, and a second copy here would be a second thing to drift.
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

/// Resource key for the build a signal came from.
pub const RESOURCE_SERVICE_VERSION: &str = "service.version";

/// Resource key for WHICH replica a signal came from.
///
/// Emitted only when a deployment supplies one. A fabricated identity would
/// multiply every series by the replica count without being trustworthy, and
/// an absent one leaves replicas publishing cumulative sums under a single
/// series identity — which a store reads as counter resets.
pub const RESOURCE_SERVICE_INSTANCE_ID: &str = "service.instance.id";

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
/// A CUSTOM span, not a `GenAI` client span. The runner opens its own `GenAI`
/// spans under [`RUNNER_SCOPE_NAME`], but no trace context crosses the wire
/// between the two, so this process cannot honestly claim to be one half of a
/// distributed agent trace.
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

/// The serving provider, using its standard spelling or a compiled public identity.
///
/// [`provider::normalize`] rejects arbitrary names and private endpoint URLs.
pub const ATTR_PROVIDER_NAME: &str = "gen_ai.provider.name";

/// The model a run was issued against, exactly as the catalogue spells it.
pub const ATTR_REQUEST_MODEL: &str = "gen_ai.request.model";

/// Prompt tokens for the whole run, cached input included.
pub const ATTR_USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";

/// Completion tokens for the whole run.
pub const ATTR_USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";

/// Prompt tokens a model turn read from the provider's cache.
pub const ATTR_USAGE_CACHE_READ_TOKENS: &str = "gen_ai.usage.cache_read.input_tokens";

/// Prompt tokens a model turn wrote to the provider's cache.
pub const ATTR_USAGE_CACHE_CREATION_TOKENS: &str = "gen_ai.usage.cache_creation.input_tokens";

/// Whether the sandbox was this platform's or the tenant's own.
pub const ATTR_EXECUTION_POSTURE: &str = "agentsfleet.execution.posture";

/// The workspace a run belongs to.
pub const ATTR_WORKSPACE_ID: &str = "agentsfleet.workspace.id";

/// The tenant whose wallet a run drew on.
pub const ATTR_TENANT_ID: &str = "agentsfleet.tenant.id";

/// The event a run executed.
pub const ATTR_EVENT_ID: &str = "agentsfleet.event.id";

/// Event creation to lease issue, both instants on the lease row.
pub const ATTR_DELIVERY_QUEUE_WAIT_MS: &str = "agentsfleet.delivery.queue_wait_ms";

/// The runner's memory read, when the lease carried no window.
pub const ATTR_DELIVERY_MEMORY_MS: &str = "agentsfleet.delivery.memory_ms";

/// The model's turns, as the runner measured them.
pub const ATTR_DELIVERY_MODEL_MS: &str = "agentsfleet.delivery.model_ms";

/// The tools those turns called, as the runner measured them.
pub const ATTR_DELIVERY_TOOLS_MS: &str = "agentsfleet.delivery.tools_ms";

/// How long the run's sandbox calls waited on the build.
pub const ATTR_DELIVERY_SANDBOX_WAIT_MS: &str = "agentsfleet.delivery.sandbox_wait_ms";

/// The runner's hold-or-destroy up to the start of its report post.
pub const ATTR_DELIVERY_SETTLE_MS: &str = "agentsfleet.delivery.settle_ms";

/// The report's receipt to the `event_complete` publish, on this daemon.
pub const ATTR_DELIVERY_COMPLETE_MS: &str = "agentsfleet.delivery.complete_ms";

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
    ATTR_DELIVERY_QUEUE_WAIT_MS,
    ATTR_DELIVERY_MEMORY_MS,
    ATTR_DELIVERY_MODEL_MS,
    ATTR_DELIVERY_TOOLS_MS,
    ATTR_DELIVERY_SANDBOX_WAIT_MS,
    ATTR_DELIVERY_SETTLE_MS,
    ATTR_DELIVERY_COMPLETE_MS,
];

// ---------------------------------------------------------------------------
// The daemon verb spans' vocabulary
// ---------------------------------------------------------------------------

/// The span one steer leaves: admission to the announce.
pub const SPAN_FLEET_STEER: &str = "fleet.steer";

/// The span one offer that selected work leaves: claim to the stream write.
pub const SPAN_FLEET_OFFER: &str = "fleet.offer";

/// The span one terminal report leaves: the lease read to the acknowledgement.
pub const SPAN_FLEET_REPORT: &str = "fleet.report";

/// The span one fallback memory hydrate leaves.
pub const SPAN_FLEET_HYDRATE: &str = "fleet.hydrate";

/// The span one memory capture leaves.
pub const SPAN_FLEET_CAPTURE: &str = "fleet.capture";

/// The steer verb's `admit` stage, in whole milliseconds.
pub const ATTR_STEER_ADMIT_MS: &str = "agentsfleet.steer.admit_ms";

/// The steer verb's `announce` stage, in whole milliseconds.
pub const ATTR_STEER_ANNOUNCE_MS: &str = "agentsfleet.steer.announce_ms";

/// The offer verb's `claim` stage, in whole milliseconds.
pub const ATTR_OFFER_CLAIM_MS: &str = "agentsfleet.offer.claim_ms";

/// The offer verb's `admit` stage, in whole milliseconds.
pub const ATTR_OFFER_ADMIT_MS: &str = "agentsfleet.offer.admit_ms";

/// The offer verb's `issue` stage, in whole milliseconds.
pub const ATTR_OFFER_ISSUE_MS: &str = "agentsfleet.offer.issue_ms";

/// The offer verb's `encode` stage, in whole milliseconds.
pub const ATTR_OFFER_ENCODE_MS: &str = "agentsfleet.offer.encode_ms";

/// The offer verb's `send` stage, in whole milliseconds.
pub const ATTR_OFFER_SEND_MS: &str = "agentsfleet.offer.send_ms";

/// The report verb's `load` stage, in whole milliseconds.
pub const ATTR_REPORT_LOAD_MS: &str = "agentsfleet.report.load_ms";

/// The report verb's `prepare` stage, in whole milliseconds.
pub const ATTR_REPORT_PREPARE_MS: &str = "agentsfleet.report.prepare_ms";

/// The report verb's `capture` stage, in whole milliseconds.
pub const ATTR_REPORT_CAPTURE_MS: &str = "agentsfleet.report.capture_ms";

/// The report verb's `commit` stage, in whole milliseconds.
pub const ATTR_REPORT_COMMIT_MS: &str = "agentsfleet.report.commit_ms";

/// The report verb's `announce` stage, in whole milliseconds.
pub const ATTR_REPORT_ANNOUNCE_MS: &str = "agentsfleet.report.announce_ms";

/// The report verb's `ack` stage, in whole milliseconds.
pub const ATTR_REPORT_ACK_MS: &str = "agentsfleet.report.ack_ms";

/// The hydrate verb's `fence` stage, in whole milliseconds.
pub const ATTR_HYDRATE_FENCE_MS: &str = "agentsfleet.hydrate.fence_ms";

/// The hydrate verb's `memory_read` stage, in whole milliseconds.
pub const ATTR_HYDRATE_MEMORY_READ_MS: &str = "agentsfleet.hydrate.memory_read_ms";

/// The capture verb's `fence` stage, in whole milliseconds.
pub const ATTR_CAPTURE_FENCE_MS: &str = "agentsfleet.capture.fence_ms";

/// The capture verb's `memory_write` stage, in whole milliseconds.
pub const ATTR_CAPTURE_MEMORY_WRITE_MS: &str = "agentsfleet.capture.memory_write_ms";

/// Every key a daemon verb span may carry: its identity, then each verb's
/// stages. A verb span writes nothing outside this list.
pub const VERB_SPAN_KEYS: &[&str] = &[
    ATTR_EVENT_ID,
    ATTR_AGENT_ID,
    ATTR_STEER_ADMIT_MS,
    ATTR_STEER_ANNOUNCE_MS,
    ATTR_OFFER_CLAIM_MS,
    ATTR_OFFER_ADMIT_MS,
    ATTR_OFFER_ISSUE_MS,
    ATTR_OFFER_ENCODE_MS,
    ATTR_OFFER_SEND_MS,
    ATTR_REPORT_LOAD_MS,
    ATTR_REPORT_PREPARE_MS,
    ATTR_REPORT_CAPTURE_MS,
    ATTR_REPORT_COMMIT_MS,
    ATTR_REPORT_ANNOUNCE_MS,
    ATTR_REPORT_ACK_MS,
    ATTR_HYDRATE_FENCE_MS,
    ATTR_HYDRATE_MEMORY_READ_MS,
    ATTR_CAPTURE_FENCE_MS,
    ATTR_CAPTURE_MEMORY_WRITE_MS,
];

// ---------------------------------------------------------------------------
// The runner's vocabulary
// ---------------------------------------------------------------------------

/// The service, and the instrumentation scope, everything a runner emits
/// names: a span under it was opened on a runner host, never by the daemon.
pub const RUNNER_SCOPE_NAME: &str = "agentsfleet-runner";

/// The span one lease's work runs under; every runner span sits inside one.
pub const SPAN_RUNNER_LEASE: &str = "runner.lease";

/// Which runner a signal came from: the daemon's id for the runner's row, so
/// each runner in a farm is told apart and joins its daemon record.
pub const ATTR_RUNNER_ID: &str = "agentsfleet.runner.id";

/// The host a runner reported itself on.
pub const ATTR_RUNNER_HOST: &str = "agentsfleet.runner.host";

/// The lease a runner span belongs to.
pub const ATTR_LEASE_ID: &str = "agentsfleet.lease.id";

/// Whether a lease ran in the sandbox its fleet's previous lease left held,
/// rather than a fresh one; absent from a lease that used no sandbox.
pub const ATTR_SANDBOX_REUSED: &str = "agentsfleet.sandbox.reused";

/// The lease's memory hydrate, on the fallback path only.
pub const ATTR_RUNNER_MEMORY_MS: &str = "agentsfleet.runner.memory_ms";

/// The sandbox, from sizing through checkout.
pub const ATTR_RUNNER_SANDBOX_MS: &str = "agentsfleet.runner.sandbox_ms";

/// Inside the sandbox segment: the egress bind.
pub const ATTR_RUNNER_EGRESS_MS: &str = "agentsfleet.runner.egress_ms";

/// Inside the sandbox segment: a held sandbox thawed, or a fresh one built.
pub const ATTR_RUNNER_PREPARE_MS: &str = "agentsfleet.runner.prepare_ms";

/// Inside the sandbox segment: the bound repositories checked out.
pub const ATTR_RUNNER_CHECKOUT_MS: &str = "agentsfleet.runner.checkout_ms";

/// How long the turn's sandbox-side calls waited for the build.
pub const ATTR_RUNNER_SANDBOX_WAIT_MS: &str = "agentsfleet.runner.sandbox_wait_ms";

/// The model's turns, less the tools they called.
pub const ATTR_RUNNER_MODEL_MS: &str = "agentsfleet.runner.model_ms";

/// The tools the model's turns called.
pub const ATTR_RUNNER_TOOLS_MS: &str = "agentsfleet.runner.tools_ms";

/// The settle, from hold-or-destroy to the posted report.
pub const ATTR_RUNNER_REPORT_MS: &str = "agentsfleet.runner.report_ms";

/// Inside the model segment: the run's first answer chunk.
pub const ATTR_RUNNER_FIRST_TOKEN_MS: &str = "agentsfleet.runner.first_token_ms";

/// The lease span's segment keys, in the order the lease's last line logs them.
pub const RUNNER_SEGMENT_KEYS: [&str; 10] = [
    ATTR_RUNNER_MEMORY_MS,
    ATTR_RUNNER_SANDBOX_MS,
    ATTR_RUNNER_EGRESS_MS,
    ATTR_RUNNER_PREPARE_MS,
    ATTR_RUNNER_CHECKOUT_MS,
    ATTR_RUNNER_SANDBOX_WAIT_MS,
    ATTR_RUNNER_MODEL_MS,
    ATTR_RUNNER_TOOLS_MS,
    ATTR_RUNNER_REPORT_MS,
    ATTR_RUNNER_FIRST_TOKEN_MS,
];

/// One model turn.
pub const OPERATION_CHAT: &str = "chat";

/// One tool call.
pub const OPERATION_EXECUTE_TOOL: &str = "execute_tool";

/// The tool a call ran.
pub const ATTR_TOOL_NAME: &str = "gen_ai.tool.name";

/// The call's id within its run.
pub const ATTR_TOOL_CALL_ID: &str = "gen_ai.tool.call.id";

/// Every key the runner's spans carry: the lease span's identity, then the
/// `GenAI` spans under it.
pub const RUNNER_SPAN_KEYS: &[&str] = &[
    ATTR_RUNNER_ID,
    ATTR_RUNNER_HOST,
    ATTR_LEASE_ID,
    ATTR_SANDBOX_REUSED,
    ATTR_EVENT_ID,
    ATTR_AGENT_ID,
    ATTR_RUNNER_MEMORY_MS,
    ATTR_RUNNER_SANDBOX_MS,
    ATTR_RUNNER_EGRESS_MS,
    ATTR_RUNNER_PREPARE_MS,
    ATTR_RUNNER_CHECKOUT_MS,
    ATTR_RUNNER_SANDBOX_WAIT_MS,
    ATTR_RUNNER_MODEL_MS,
    ATTR_RUNNER_TOOLS_MS,
    ATTR_RUNNER_REPORT_MS,
    ATTR_RUNNER_FIRST_TOKEN_MS,
    ATTR_OPERATION_NAME,
    ATTR_PROVIDER_NAME,
    ATTR_REQUEST_MODEL,
    ATTR_USAGE_INPUT_TOKENS,
    ATTR_USAGE_OUTPUT_TOKENS,
    ATTR_USAGE_CACHE_READ_TOKENS,
    ATTR_USAGE_CACHE_CREATION_TOKENS,
    ATTR_TOOL_NAME,
    ATTR_TOOL_CALL_ID,
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

/// What caused a bus gap, from its closed cause set.
pub const LABEL_CAUSE: &str = "cause";

/// What triggered an outbound claim, from its closed set.
pub const LABEL_VIA: &str = "via";

/// The native runner connection protocol, from its closed transport set.
pub const LABEL_TRANSPORT: &str = "transport";

/// Which bounded native operation produced a measurement.
pub const LABEL_OPERATION: &str = "operation";

/// Which library read surface a measurement is about.
pub const LABEL_SURFACE: &str = "surface";

/// Which stage of a library read a measurement times.
pub const LABEL_STAGE: &str = "stage";

/// Which lifecycle status a fleet count is about.
pub const LABEL_STATUS: &str = "status";

/// Which side of the platform boundary a failed run fell on.
///
/// Carried only by the failing half of a family: a successful run has no fault,
/// so the label is absent rather than written with a third "none" value. A
/// `{fault!="workload"}` matcher therefore selects successes and platform
/// failures together, which is exactly the objective's valid-event set.
pub const LABEL_FAULT: &str = "fault";

/// Which kind of thing a count is split by, where one family counts two
/// causes of the same event — a run started fresh or by reclaim.
pub const LABEL_KIND: &str = "kind";

/// Which product's work a billing count is about: a fleet lease or a setup
/// chat attempt.
pub const LABEL_WORK_TYPE: &str = "work_type";

/// Every label key the census's own columns name.
///
/// The vocabulary's completeness claim, and what
/// `every_census_label_resolves_to_a_constant` grades the census against in
/// both directions: a column with no constant here is a string literal about
/// to be written by hand, and a constant with no column is a key nothing
/// exports.
pub const CENSUS_LABEL_KEYS: &[&str] = &[
    ATTR_TOKEN_TYPE,
    ATTR_HTTP_ROUTE,
    ATTR_HTTP_REQUEST_METHOD,
    LABEL_ROLE,
    LABEL_STATE,
    LABEL_LIMIT,
    LABEL_CAPABILITY,
    LABEL_STATUS_CLASS,
    ATTR_REQUEST_MODEL,
    LABEL_REASON,
    LABEL_SIGNAL,
    LABEL_ATTRIBUTE,
    LABEL_RUNNER_ID,
    LABEL_OUTCOME,
    LABEL_CAUSE,
    LABEL_VIA,
    LABEL_TRANSPORT,
    LABEL_KEY_EXCHANGE,
    LABEL_OPERATION,
    LABEL_SURFACE,
    LABEL_STAGE,
    LABEL_STATUS,
    LABEL_KIND,
    LABEL_FAULT,
    LABEL_WORK_TYPE,
    ATTR_PROVIDER_NAME,
];

/// Role of a database connection pool.
pub const LABEL_ROLE: &str = "role";
/// Finite TLS key-agreement group selected by a native connection.
pub const LABEL_KEY_EXCHANGE: &str = "key_exchange";
/// A closed inventory or connection state.
pub const LABEL_STATE: &str = "state";
/// A model's configured limit kind.
pub const LABEL_LIMIT: &str = "limit";
/// A configured model-route capability.
pub const LABEL_CAPABILITY: &str = "capability";
/// Bounded response class, including cancelled requests.
pub const LABEL_STATUS_CLASS: &str = "status_class";
