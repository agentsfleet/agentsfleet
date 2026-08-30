# Adversarial review brief — `afd_observability` metrics pipeline (M181_001 §3)

You are reviewing a DESIGN before it is implemented. You have no repository
access; everything you need is in this document. Attack the design. The
required output format is at the end — concrete advice with named APIs and
sample code, no hedging, no "consider exploring".

## System context

`agentsfleet` is replacing a production Zig daemon (`agentsfleetd`) with a Rust
rewrite, crate by crate, behind parity gates. Production: 3 Fly.io machines
(shared-cpu-4x, 4GB), private-network only, ingress via Cloudflare Tunnel.
Telemetry is PUSH-ONLY — the daemon has no `/metrics` endpoint, nothing scrapes
it, and the architecture documents pin that. All three signals (metrics,
traces, logs) leave over OTLP/HTTP. Target topology: daemon →
`http://otel-{env}.internal:4318` (an OTel Collector app on the same private
network) → vendor fan-out in collector config. Invariant: the collector's
exporters are `otlphttp` ONLY; a backend without native OTLP intake is not a
supported backend.

The Zig daemon hand-rolled its whole metrics layer (~1,450 lines: instruments,
delta windows, label-dimension products, cardinality caps, payload encoding)
because Zig has no OpenTelemetry SDK. The Rust side must NOT port that; it uses
`opentelemetry_sdk` (already a workspace dependency at 0.32,
`features = ["trace", "rt-tokio"]`; the `metrics` feature is being added).
Parity binds at the WIRE only: family names, label keys, value types — never
code shape.

This milestone (§3) builds the crate-local pipeline: registry, instruments,
Views, a drop-counting exporter wrapper, the crate's first error type. The OTLP
transport + boot wiring is a LATER milestone — do not review transport
construction beyond the seam it plugs into.

## Hard constraints (violating any of these is a review finding)

1. **71 metric families, byte-stable names and label keys.** The contract is a
   census table in an architecture document (not the Zig source); the Zig suite
   already pins its registry against that census in both directions. The Rust
   registry must be graded against the same census by test.
2. **Telemetry can never slow a request.** Hot-path recording must be atomic
   ops; export is background; a full queue DROPS and counts the drop. Prior
   art in-crate: a span exporter wrapper holding `SpanDrops(Arc<AtomicUsize>)`,
   `Relaxed` ordering, documented as "export that cannot slow a request down,
   and says so when it loses spans". The metrics side needs the same property
   and a matching counter.
3. **No double-count.** The Zig exporter deliberately does NOT retry OTLP
   posts: "OTLP JSON has no idempotency key; replaying delta metrics can
   double-count" (recorded decision). Whatever the Rust export path does must
   preserve this or argue precisely why not.
4. **Rust error standard (repo-wide, enforced):** one error type per crate +
   `pub type Result<T, E = Error>` beside it; compose with `#[from]`; `map_err`
   only to ADD context the call site alone knows; `source()` returns the cause,
   never yourself; not every variant has a cause. The crate currently has ZERO
   fallible functions; this work adds the first.
5. **Logging standard:** `tracing` emits with an `event` field (snake_case
   verb_noun), fields hoisted into locals, per-iteration paths at `debug`,
   boundary operations emit `_started`/`_completed`|`_failed` pairs. Endpoint
   values are logged as `source=env:NAME`, never the value.
6. **No new dependencies in §3** beyond the `metrics` feature flag on the
   already-locked `opentelemetry_sdk`. (The OTLP transport dep is a separate,
   later, explicitly-recorded decision.)
7. File length caps: ≤350 lines/file, ≤50 lines/function.

## The Zig registry being replaced (shape, verbatim semantics)

```zig
pub const MetricKind = enum { sum, histogram, gauge };
pub const Temporality = enum { delta, cumulative };
pub const Scale = enum { none, millis_to_seconds, nanos_to_seconds };

pub const MetricMeta = struct {
    name: []const u8,          // wire family name
    unit: []const u8,
    kind: MetricKind,
    monotonic: bool = false,
    temporality: Temporality = .delta,
    bounds: []const u64 = &.{},   // histogram bucket bounds
    scale: Scale = .none,          // unit conversion applied at record time
    max_series: usize = 1,         // worst-case label-product, derived from dims
    streamed: bool = false,        // per-runner families, 4096 slots + `_other` overflow
    cost: bool = false,            // shares a 256-series budget (COST_SERIES_BUDGET)
    evented: bool = false,         // sampled through a lock-free ring
    live_read: bool = false,       // read at flush time by a collect hook (pool
                                   // stats, RSS probe) — absence keeps the family
                                   // out of the window rather than faking a zero
};
```

- 71 families in a closed enum. Mixed temporality across families: evented cost
  families are DELTA; most `*_total` counters are CUMULATIVE; gauges for pool/
  in-flight state.
- Names are mixed-convention on purpose (parity, not taste):
  `gen_ai.invoke_agent.duration`, `agentsfleet.invoke_agent.token.usage`
  (dotted, semconv-flavoured) alongside `agentsfleet_api_in_flight_requests`,
  `agentsfleet_redis_pool_idle`, `agentsfleet_runner_executions_total`
  (prometheus-flavoured underscores). Both spellings must survive as-is.
- Label dimensions are closed enums per label key plus at most one dynamic
  dimension (model id, runner id). Cardinality overflow in Zig lands in an
  `_other` LABEL VALUE (e.g. runner slot 4097 records under `runner_id="_other"`),
  NOT the OTel spec's `otel.metric.overflow=true` attribute.
- `scale`: Zig records raw nanos/millis and converts at serialization. The Rust
  plan DELETES this concept — record `f64` seconds at the call site.

## Proposed design (attack this)

### D1 — registry as data

```rust
// registry.rs — one row per census family. &'static everything; no macros.
pub enum Kind { Counter, Histogram, Gauge }
pub enum Temporality { Delta, Cumulative }

pub struct FamilyMeta {
    pub name: &'static str,
    pub unit: &'static str,
    pub kind: Kind,
    pub temporality: Temporality,
    pub bounds: &'static [f64],          // histograms only
    pub label_keys: &'static [&'static str],
    pub max_series: usize,               // per-family cardinality cap
    pub live_read: bool,                 // -> observable instrument w/ callback
}

pub static FAMILIES: &[FamilyMeta] = &[ /* 71 rows */ ];
```

A unit test parses the census markdown table out of the architecture doc and
asserts set-equality with `FAMILIES` (both directions), plus per-family label
keys. (The Zig side already runs the same test against the same doc, so the doc
is the single contract and either implementation drifting fails loudly.)

### D2 — instruments + typed recording surface

Built once from `FAMILIES` via `Meter`:
- `Kind::Counter` → `u64_counter` / `f64_counter`
- `Kind::Histogram` → `f64_histogram` (bounds via View aggregation)
- `Kind::Gauge` + `live_read: false` → `f64_gauge`
- `live_read: true` → `f64_observable_gauge` / `u64_observable_counter` with a
  callback (direct mapping of Zig's flush-time collect hooks — pool snapshot,
  RSS probe)

Exposed to the daemon as a typed struct of handles (compile-time-known fields,
no string lookup at record time):

```rust
pub struct Metrics {
    pub api_in_flight: UpDownCounter<i64>,
    pub runner_executions: Counter<u64>,
    // ... one field per non-observable family
}
```

### D3 — Views pin the wire

One `with_view` closure over the provider builder:
- match instrument by exact name → `Stream` with `allowed_attribute_keys` =
  the family's `label_keys` (anything else recorded is dropped, not exported),
  histogram `Aggregation::ExplicitBucketHistogram { boundaries }` from
  `bounds`, and the per-family cardinality cap.
- Known open problem, stated honestly: the SDK's overflow marker is the
  spec-fixed attribute `otel.metric.overflow=true`; Zig's is an `_other` label
  VALUE. Current position: (a) for the per-runner streamed families, keep the
  `_other` semantics in OUR recording layer (a bounded admission map in front
  of the instrument, overflow records under `runner_id="_other"` — this is
  domain logic, not SDK aggregation), so the SDK cardinality path is never the
  first line of defense; (b) the SDK cap stays as backstop, and if it ever
  fires, its spelling difference is a REGISTERED divergence read by the
  cutover differ, not silently accepted.

### D4 — non-blocking export + drop counter

Wrapper implementing `PushMetricExporter`, delegating to the real transport,
counting failed exports in `MetricDrops(Arc<AtomicUsize>)` — the exact twin of
the existing span wrapper. `PeriodicReader` drives collection (the SDK's
replacement for Zig's hand-rolled flush thread with 50/50/768 wake thresholds
and 5s max interval).

### D5 — error type (first fallible surface in the crate)

```rust
// error.rs
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("metric family {name} declares {got} label keys; the census says {want}")]
    RegistryMismatch { name: &'static str, got: usize, want: usize },
    #[error("the meter provider was already installed")]
    AlreadyInstalled,
    // construction failures from the SDK compose via #[from] where they carry a source
}
```

## Questions you must answer concretely (Q1–Q10)

- **Q1 Temporality mixing.** Families need per-family delta vs cumulative. In
  `opentelemetry_sdk` 0.32, temporality selection sits on the exporter/reader
  (a `Temporality` enum), not per-View. Is per-family temporality expressible
  at all in 0.32 — and if not, which single reader temporality (or
  `LowMemory`?) least distorts a mixed registry whose consumers are Grafana
  and Elastic BEHIND A COLLECTOR, given the collector can run
  `cumulativetodelta`/`deltatocumulative` processors per backend? Name the
  exact API and the exact processor chain you would ship.
- **Q2 Overflow spelling.** Is D3's two-layer answer (domain-level `_other`
  admission in front of the instrument + SDK cap as backstop) the right shape,
  or is a collector `transform` processor rewriting `otel.metric.overflow`
  into the `_other` spelling strictly better? Consider: which failure mode is
  observable where, and which survives the Zig daemon's retirement cleanly.
- **Q3 PeriodicReader mechanics in 0.32.** Which thread does collection run on
  (own thread vs tokio runtime, given `rt-tokio` is enabled for spans)? What
  happens when an observable callback blocks or panics mid-collect? What is
  the exact behavior when the exporter is slower than the interval — queueing,
  skipping, or overlap — and does any of it violate constraint 2?
- **Q4 Retry / double-count.** Does the stock OTLP metric export path (when
  the transport lands) retry on failure by default in the current
  opentelemetry-otlp, and with what idempotency consequences for DELTA
  streams? The Zig decision was no-retry-ever. What exact configuration
  reproduces that, and is dropping retry actually right for CUMULATIVE
  families (a lost cumulative point self-heals; a lost delta doesn't)?
- **Q5 Observable-callback safety.** The `live_read` callbacks read a Redis
  pool snapshot and an RSS probe. The Zig design keeps "absence keeps the
  family out of the window rather than faking a zero." Can an observable
  callback in 0.32 DECLINE to observe (emit nothing) on a failed read, and
  does that produce absent-series or last-value semantics at the collector?
- **Q6 Hot-path cost.** For a counter with a closed label set recorded on
  every request: what is the actual per-record cost shape in 0.32 (attribute
  set hashing? allocation per record? contention on a DashMap?), and is a
  pre-resolved-handle pattern (D2's struct of instruments + pre-built
  `&[KeyValue]` slices as statics) sufficient, or do we need bound instruments
  / cached attribute sets? Cite the mechanism, not vibes.
- **Q7 Census-parsing test.** A unit test parsing a markdown table out of a
  docs file: legitimate contract test or fragility? If fragile, name a better
  single-source-of-truth mechanism that still lets the (temporarily coexisting)
  Zig suite pin against the same source.
- **Q8 max_series arithmetic.** Zig derives a comptime memory bound
  (COST_SERIES_BUDGET=256 + Σ fixed products) and fails the BUILD when
  declarations exceed it. What is the equivalent build-time or startup-time
  assertion in the Rust design, and where does the SDK's own default
  cardinality limit (2000/instrument in recent SDKs — verify the 0.32 number)
  interact with per-family caps?
- **Q9 The `Metrics` struct.** 60+ fields of instrument handles: right shape,
  or should the registry generate accessor indices (array + enum index) to
  keep the struct under the file-length cap and make "family exists but no
  call site records it" detectable by test?
- **Q10 What breaks at the collector.** Given mixed dotted + underscore family
  names and OTLP → collector → Grafana/Elastic via otlphttp only: name any
  normalization either backend applies to OTLP metric names (e.g. Prometheus
  translation in Grafana's pipeline: dots→underscores, unit suffixing) that
  would make the "byte-stable names" claim FALSE at the dashboard even though
  the daemon exported faithfully. This decides whether dashboard continuity is
  assertable at the daemon wire or only at the backend query layer.

## Required output format

For each of Q1–Q10: a verdict (AGREE with design / CHANGE with the exact
replacement), the specific `opentelemetry_sdk` 0.32 API names involved, and —
where you propose a change — compilable-shaped Rust (or collector YAML) for
the replacement. Then a final section: the three highest-risk defects in the
design AS WRITTEN, ranked, each with the failure it produces in production and
the test that would catch it. Do not review style. Do not propose new scope.
