//! The pinned OpenTelemetry semantic registry — one owner for every metric
//! name, unit, attribute key, fixed attribute value, and resource key that
//! leaves this process on the wire, plus the Grafana dashboard that reads them.
//!
//! Pinned sources. Core semantic conventions `v1.43.0` supply resource, unit,
//! naming, HTTP, and `error.type` semantics. The Generative Artificial
//! Intelligence (GenAI) conventions are pinned to upstream commit
//! `2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b`, which is Development status and
//! publishes no schema URL of its own — so only the core schema URL goes on the
//! wire and none is invented for GenAI.
//!
//! A standard name is used only where the measured boundary matches the pinned
//! definition. `ReportRequest` carries token counts cumulative over a whole
//! agent run rather than one provider call, so run totals stay under
//! `agentsfleet.*`; `ReportTelemetry.wall_ms` does bound exactly one sandboxed
//! agent invocation, so the standard agent-duration metric applies.

const std = @import("std");
const Mode = @import("../state/tenant_provider.zig").Mode;

// ---------------------------------------------------------------------------
// Pinned schema identity
// ---------------------------------------------------------------------------

/// The only schema URL that may appear on the wire. The pinned GenAI commit
/// publishes none, so no GenAI schema URL is fabricated beside it.
pub const CORE_SCHEMA_URL = "https://opentelemetry.io/schemas/1.43.0";
pub const SCOPE_NAME = "agentsfleetd";

pub const RESOURCE_SERVICE_NAME = "service.name";
pub const RESOURCE_SERVICE_NAMESPACE = "service.namespace";
pub const RESOURCE_SERVICE_VERSION = "service.version";
pub const RESOURCE_SERVICE_INSTANCE_ID = "service.instance.id";
pub const SERVICE_NAMESPACE = "agentsfleet";

// ---------------------------------------------------------------------------
// Metric names and units
// ---------------------------------------------------------------------------

/// Standard: `wall_ms` bounds exactly one sandboxed agent invocation.
pub const METRIC_INVOKE_AGENT_DURATION = "gen_ai.invoke_agent.duration";
/// Product-specific: the report's token counts are cumulative across every
/// provider call in the invocation, so this is not a GenAI client-call metric.
pub const METRIC_INVOKE_AGENT_TOKEN_USAGE = "agentsfleet.invoke_agent.token.usage";
/// Non-additive subset of the input direction above — never a third total.
pub const METRIC_INVOKE_AGENT_CACHE_READ = "agentsfleet.invoke_agent.cache_read.token.usage";
/// Product-specific billing quantity; nanocredits are not a time unit.
pub const METRIC_BILLING_CREDIT_CONSUMED = "agentsfleet.billing.credit.consumed";
/// Exporter self-observability: samples shed by the ring or the series cap.
pub const METRIC_SAMPLES_DROPPED = "agentsfleet.telemetry.samples_dropped";

pub const UNIT_SECONDS = "s";
pub const UNIT_TOKENS = "{token}";
pub const UNIT_NANOCREDITS = "{nanocredit}";
pub const UNIT_COUNT = "1";
pub const UNIT_BYTES = "By";

// Annotation units for the level (gauge) families. The OpenTelemetry→
// Prometheus name translation drops curly-brace annotations entirely, while a
// bare "1" on a gauge can gain a `_ratio` suffix depending on the store's
// suffix setting — which would silently break every asset query. Braces make
// the exported spelling deterministic.
pub const UNIT_REQUESTS = "{request}";
pub const UNIT_STREAMS = "{stream}";
pub const UNIT_WORKERS = "{worker}";
pub const UNIT_ENTRIES = "{entry}";
pub const UNIT_FLEETS = "{fleet}";
pub const UNIT_CONNECTIONS = "{connection}";
pub const UNIT_LEASES = "{lease}";

// ---------------------------------------------------------------------------
// Runtime family names (M-prefix free; the operator assets query these exact
// spellings in PromQL, so they are carried verbatim — Grafana Cloud's OTLP
// ingest passes an underscore name through unchanged). Families whose owning
// module already exports its name constant (metrics_counters, metrics_otel,
// library_stages, metrics_sensitive_memory, metrics_memory, metrics_runner)
// keep that module as the single source; only names that previously lived as
// literals in the deleted Prometheus renderer are declared here.
// ---------------------------------------------------------------------------

pub const METRIC_API_BACKPRESSURE_REJECTIONS = "agentsfleet_api_backpressure_rejections_total";
pub const METRIC_API_IN_FLIGHT_REQUESTS = "agentsfleet_api_in_flight_requests";
pub const METRIC_SSE_BACKPRESSURE_REJECTIONS = "agentsfleet_sse_backpressure_rejections_total";
pub const METRIC_SSE_IN_FLIGHT_STREAMS = "agentsfleet_sse_in_flight_streams";
pub const METRIC_SSE_DROPPED_FRAMES = "agentsfleet_sse_dropped_frames_total";
pub const METRIC_SSE_HUB_RECONNECTS = "agentsfleet_sse_hub_reconnects_total";
pub const METRIC_WORKER_RUNNING = "agentsfleet_worker_running";
pub const METRIC_FLEET_TRIGGERED = "agentsfleet_fleet_triggered_total";
pub const METRIC_SIGNUP_BOOTSTRAPPED = "agentsfleet_signup_bootstrapped_total";
pub const METRIC_SIGNUP_REPLAYED = "agentsfleet_signup_replayed_total";
pub const METRIC_SIGNUP_FAILED = "agentsfleet_signup_failed_total";
pub const METRIC_REDIS_POOL_ACTIVE = "agentsfleet_redis_pool_active";
pub const METRIC_REDIS_POOL_IDLE = "agentsfleet_redis_pool_idle";
pub const METRIC_REDIS_POOL_DIALS = "agentsfleet_redis_pool_dials_total";
pub const METRIC_REDIS_POOL_OVERFLOW_DIALS = "agentsfleet_redis_pool_overflow_dials_total";
pub const METRIC_REDIS_POOL_POISONED = "agentsfleet_redis_pool_poisoned_connections_total";
pub const METRIC_REDIS_POOL_RECONNECTS = "agentsfleet_redis_pool_reconnects_total";
pub const METRIC_REDIS_POOL_FORCED_CLOSES = "agentsfleet_redis_pool_forced_closes_total";
pub const METRIC_REDIS_POOL_ACQUIRE_TIMEOUTS = "agentsfleet_redis_pool_acquire_timeouts_total";

/// Names a payload may never emit: superseded product spellings plus GenAI
/// client-call metrics whose measured boundary this process cannot observe.
/// `test_semantic_registry_matches_pinned_sources` asserts none is live.
pub const REJECTED_METRIC_NAMES = [_][]const u8{
    "gen_ai.client.token.usage",
    "gen_ai.client.operation.duration",
    "agentsfleet.credit.drained_nanos",
    "agentsfleet.tokens.processed",
    "agentsfleet.run.duration_ms",
};

// ---------------------------------------------------------------------------
// Attribute keys
// ---------------------------------------------------------------------------

pub const ATTR_OPERATION_NAME = "gen_ai.operation.name";
pub const ATTR_PROVIDER_NAME = "gen_ai.provider.name";
pub const ATTR_REQUEST_MODEL = "gen_ai.request.model";
pub const ATTR_TOKEN_TYPE = "gen_ai.token.type";
pub const ATTR_AGENT_ID = "gen_ai.agent.id";
pub const ATTR_USAGE_INPUT_TOKENS = "gen_ai.usage.input_tokens";
pub const ATTR_USAGE_OUTPUT_TOKENS = "gen_ai.usage.output_tokens";
pub const ATTR_ERROR_TYPE = "error.type";

pub const ATTR_EXECUTION_POSTURE = "agentsfleet.execution.posture";
pub const ATTR_CHARGE_TYPE = "agentsfleet.billing.charge.type";
pub const ATTR_EVENT_ID = "agentsfleet.event.id";
pub const ATTR_WORKSPACE_ID = "agentsfleet.workspace.id";
pub const ATTR_TENANT_ID = "agentsfleet.tenant.id";

pub const ATTR_HTTP_REQUEST_METHOD = "http.request.method";
pub const ATTR_HTTP_ROUTE = "http.route";
pub const ATTR_HTTP_RESPONSE_STATUS_CODE = "http.response.status_code";

/// Attribute keys that must never reach an OTLP **metric** point. Workspace and
/// tenant identity stay queryable in Postgres, which is the exact money truth;
/// putting them on a metric creates series that outlive the process guard.
pub const METRIC_FORBIDDEN_ATTRS = [_][]const u8{
    ATTR_WORKSPACE_ID,
    ATTR_TENANT_ID,
    ATTR_EVENT_ID,
    "workspace",
    "model",
    "posture",
    "direction",
};

// ---------------------------------------------------------------------------
// Fixed attribute values
// ---------------------------------------------------------------------------

/// The single operation this process observes: one sandboxed agent invocation.
pub const OPERATION_INVOKE_AGENT = "invoke_agent";
/// Span name for the settled control-plane delivery observation. It is a custom
/// span, not a runner trace: no runner span or trace context exists to join.
pub const SPAN_FLEET_DELIVERY = "fleet.delivery";

/// `gen_ai.token.type` — input already includes cached input; cache detail is
/// reported by its own subset metric, never as a third additive direction.
pub const TokenType = enum {
    input,
    output,

    pub fn label(self: TokenType) []const u8 {
        return @tagName(self);
    }
};

/// `agentsfleet.billing.charge.type` — the three committed debit classes. This
/// is the telemetry classification, distinct from the durable ledger's
/// `charge_type` (which records both renewal slices and the final settle as
/// `stage` rows); the metric separates them so an operator can see renewal
/// drain apart from terminal settlement.
pub const ChargeClass = enum {
    receive,
    renewal,
    settle,

    pub fn label(self: ChargeClass) []const u8 {
        return @tagName(self);
    }
};

/// `error.type` on the agent-duration histogram. Absent on a clean run; the
/// coarse failure verdict otherwise. The granular `FailureClass` deliberately
/// stays off this metric — it multiplies the per-model series budget below,
/// and it is already carried exactly by the durable event row and the capped
/// `agentsfleet_runner_failures_total` Prometheus family.
pub const ErrorType = enum {
    fleet_error,

    pub fn label(self: ErrorType) []const u8 {
        return @tagName(self);
    }
};

const ERROR_TYPE_SLOTS: usize = 2; // absent on success, or the one value above

// ---------------------------------------------------------------------------
// Provider normalization
// ---------------------------------------------------------------------------

/// Exact well-known `gen_ai.provider.name` values at the pinned commit. A
/// configured provider that does not map to one of these omits the attribute
/// rather than exporting a private spelling as though it were standard.
pub const WELL_KNOWN_PROVIDERS = [_][]const u8{
    "anthropic",
    "aws.bedrock",
    "azure.ai.inference",
    "azure.ai.openai",
    "cohere",
    "deepseek",
    "gcp.gemini",
    "gcp.gen_ai",
    "gcp.vertex_ai",
    "groq",
    "ibm.watsonx.ai",
    "mistral_ai",
    "openai",
    "perplexity",
    "x_ai",
};

/// Map a stored provider identifier onto its exact well-known name, or null
/// when no exact mapping exists. Never truncates and never invents a value.
/// Position of a stored provider identifier within the well-known table, or
/// null when no exact mapping exists. The metric writer resolves its interned
/// value index from this ordinal, so the one walk here replaces both the walk
/// below and a second walk over every declared closed value.
pub fn providerOrdinal(stored: []const u8) ?u16 {
    // Case-insensitive because the identifier reaches us unvalidated from the
    // Command-Line Interface (CLI) provider option, where "Anthropic" and
    // "anthropic" name the same provider. The emitted value is always the table's
    // canonical spelling, so tolerating case here removes a false omission
    // without ever putting a non-standard spelling on the wire. ASCII-only is
    // correct: every well-known name is ASCII.
    for (WELL_KNOWN_PROVIDERS, 0..) |known, i| {
        if (std.ascii.eqlIgnoreCase(stored, known)) return @intCast(i);
    }
    return null;
}

pub fn normalizeProvider(stored: []const u8) ?[]const u8 {
    const ordinal = providerOrdinal(stored) orelse return null;
    return WELL_KNOWN_PROVIDERS[ordinal];
}

/// Counted off the resolver's own enum so the derived series budget below can
/// never drift from the postures actually emitted.
const POSTURE_COUNT: usize = @typeInfo(Mode).@"enum".fields.len;
const TOKEN_TYPE_COUNT: usize = @typeInfo(TokenType).@"enum".fields.len;
const CHARGE_CLASS_COUNT: usize = @typeInfo(ChargeClass).@"enum".fields.len;

// ---------------------------------------------------------------------------
// Derived model-attribution budget
// ---------------------------------------------------------------------------

/// Worst-case distinct series one exact (provider, model) pair can create in a
/// single flush window, summed across every metric that carries the model
/// attribute. Derived from the fixed attribute sets above, so adding a posture,
/// token type, or charge class automatically tightens the cap below instead of
/// silently overrunning the flush ceiling.
pub const SERIES_PER_MODEL_PAIR: usize =
    POSTURE_COUNT * ERROR_TYPE_SLOTS // gen_ai.invoke_agent.duration
    + POSTURE_COUNT * TOKEN_TYPE_COUNT // agentsfleet.invoke_agent.token.usage
    + POSTURE_COUNT // agentsfleet.invoke_agent.cache_read.token.usage
    + POSTURE_COUNT * CHARGE_CLASS_COUNT; // agentsfleet.billing.credit.consumed

/// Series that exist regardless of model attribution: one unattributed copy of
/// the shape above (samples whose model was omitted still aggregate) plus the
/// unlabelled exporter self-signal.
pub const RESERVED_SERIES: usize = SERIES_PER_MODEL_PAIR + 1;

/// The distinct (provider, model) pairs that may carry exact model attribution
/// while the flush window still provably fits `series_ceiling`. Callers pass
/// the aggregator's own ceiling so the two can never disagree.
pub fn modelAttributionCap(series_ceiling: usize) usize {
    if (series_ceiling <= RESERVED_SERIES) return 0;
    return (series_ceiling - RESERVED_SERIES) / SERIES_PER_MODEL_PAIR;
}

// ---------------------------------------------------------------------------
// Histogram bucket boundaries
// ---------------------------------------------------------------------------

/// Pinned agent-duration boundaries, held in **milliseconds** because the
/// runner reports integer `wall_ms` and every pinned bound is a whole multiple
/// of 10 ms. Integer bucketing therefore stays exact; the serializer divides by
/// `MILLIS_PER_SECOND` to emit the seconds the `s` unit declares.
/// Pinned agent-invocation boundaries, in the milliseconds the runner reports.
/// These are `gen_ai.invoke_agent.duration`'s own table (0.1s .. 409.6s), NOT
/// `gen_ai.client.operation.duration`'s (0.01s .. 81.92s): an agent invocation
/// runs orders of magnitude longer than one provider call, so the client table
/// would pile every real run into its last buckets and lose the tail entirely.
pub const DURATION_BUCKET_BOUNDS_MS = [_]u64{
    100, 200, 400, 800, 1600, 3200, 6400, // pin test: literal is the contract
    12800, 25600, 51200, 102400, 204800, 409600, // pin test: literal is the contract
};

/// Pinned token-usage boundaries, in tokens.
pub const TOKEN_BUCKET_BOUNDS = [_]u64{
    1, 4, 16, 64, 256, 1024, 4096, // pin test: literal is the contract
    16384, 65536, 262144, 1048576, 4194304, 16777216, 67108864, // pin test: literal is the contract
};

pub const MILLIS_PER_SECOND: u64 = 1000;
pub const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// Widest pinned bound table. The payload sizes ONE bucket array for every
/// histogram, so it must be cut to the longest table — upstream gives duration
/// and token usage different lengths, and sizing to the shorter one would
/// silently truncate the other metric's counts.
pub const MAX_BUCKET_BOUNDS = @max(DURATION_BUCKET_BOUNDS_MS.len, TOKEN_BUCKET_BOUNDS.len);

comptime {
    // Every pinned table must fit the shared array the payload cuts to
    // `MAX_BUCKET_BOUNDS + 1` (bounds plus the trailing +Inf bucket).
    std.debug.assert(DURATION_BUCKET_BOUNDS_MS.len <= MAX_BUCKET_BOUNDS);
    std.debug.assert(TOKEN_BUCKET_BOUNDS.len <= MAX_BUCKET_BOUNDS);
    // Every duration bound must be a whole number of milliseconds that divides
    // evenly into the seconds the unit declares — the premise of integer-ms
    // bucketing against a seconds-unit histogram.
    for (DURATION_BUCKET_BOUNDS_MS) |bound| std.debug.assert(bound % 10 == 0);
}

test {
    _ = @import("semconv_test.zig");
    _ = @import("semantic_schema_test.zig");
}
