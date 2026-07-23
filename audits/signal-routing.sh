#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_dir
repo_root="$(cd "$script_dir/.." && pwd)"
readonly repo_root
readonly observability="$repo_root/docs/architecture/observability.md"
readonly runner_fleet="$repo_root/docs/architecture/runner_fleet.md"
readonly build_zon="$repo_root/build.zig.zon"
readonly quality_make="$repo_root/make/quality.mk"
readonly runner_protocol="$repo_root/src/lib/contract/protocol.zig"
readonly runner_activity="$repo_root/src/lib/contract/activity.zig"
readonly log_exporter="$repo_root/src/agentsfleetd/observability/otel_logs.zig"
readonly trace_exporter="$repo_root/src/agentsfleetd/observability/otel_traces.zig"
readonly metric_exporter="$repo_root/src/agentsfleetd/observability/otel_metrics.zig"
readonly metric_aggregate="$repo_root/src/agentsfleetd/observability/otel_metrics_aggregate.zig"
readonly runner_metrics="$repo_root/src/agentsfleetd/observability/metrics_runner.zig"
readonly shared_exporter="$repo_root/src/agentsfleetd/observability/otlp/exporter.zig"
readonly otlp_client="$repo_root/src/agentsfleetd/observability/otlp/Client.zig"
readonly otlp_health="$repo_root/src/agentsfleetd/observability/metrics_otel.zig"
readonly trace_policy="$repo_root/src/agentsfleetd/http/route_trace.zig"
readonly trace_metrics="$repo_root/src/agentsfleetd/observability/metrics_trace.zig"
readonly telemetry_events="$repo_root/src/agentsfleetd/observability/telemetry_events.zig"
readonly preflight="$repo_root/src/agentsfleetd/cmd/preflight.zig"
readonly daemon_main="$repo_root/src/agentsfleetd/main.zig"
readonly http_server="$repo_root/src/agentsfleetd/http/server.zig"
readonly handler_common="$repo_root/src/agentsfleetd/http/handlers/common.zig"
readonly metering="$repo_root/src/agentsfleetd/fleet_runtime/metering.zig"
readonly report_service="$repo_root/src/agentsfleetd/fleet/service_report.zig"
readonly billing_service="$repo_root/src/agentsfleetd/fleet/service_billing.zig"
readonly lease_service="$repo_root/src/agentsfleetd/fleet/service.zig"
readonly heartbeat_handler="$repo_root/src/agentsfleetd/http/handlers/runner/heartbeat.zig"
readonly serve="$repo_root/src/agentsfleetd/cmd/serve.zig"
readonly workspace_lifecycle="$repo_root/src/agentsfleetd/http/handlers/workspaces/lifecycle.zig"
readonly webhook_fleet="$repo_root/src/agentsfleetd/http/handlers/webhooks/fleet.zig"
readonly webhook_github="$repo_root/src/agentsfleetd/http/handlers/webhooks/github.zig"
readonly signup="$repo_root/src/agentsfleetd/http/handlers/auth/identity_events_clerk.zig"

fail() {
  printf 'signal-routing audit failed: %s\n' "$1" >&2
  exit 1
}

require_literal() {
  local file="$1"
  local literal="$2"
  rg -F --quiet -- "$literal" "$file" || fail "missing '$literal' in ${file#"$repo_root/"}"
}

require_absent() {
  local file="$1"
  local literal="$2"
  local status
  [[ -f "$file" ]] || fail "missing scan target ${file#"$repo_root/"}"
  if rg -F --quiet -- "$literal" "$file"; then
    fail "unexpected '$literal' in ${file#"$repo_root/"}"
  else
    status=$?
    [[ "$status" -eq 1 ]] || fail "could not scan ${file#"$repo_root/"} (rg exit $status)"
  fi
}

require_tree_absent() {
  local path="$1"
  local pattern="$2"
  local status
  [[ -d "$path" ]] || fail "missing scan target ${path#"$repo_root/"}"
  if rg --glob '*.zig' --glob '!**/*_test.zig' --quiet -- "$pattern" "$path"; then
    fail "unexpected '$pattern' under ${path#"$repo_root/"}"
  else
    status=$?
    [[ "$status" -eq 1 ]] || fail "could not scan ${path#"$repo_root/"} (rg exit $status)"
  fi
}

check_inventory() {
  require_literal "$observability" "| \`agentsfleetd\` structured stderr | installed and called |"
  require_literal "$observability" '| OTLP logs | installed and called when configured |'
  require_literal "$observability" '| runner backend exporter | absent |'
  require_literal "$preflight" 'otel_logs.install'
  require_literal "$preflight" 'otel_traces.install'
  require_literal "$preflight" 'otel_metrics.install'
  require_literal "$daemon_main" 'log_sinks.registerSink(.{ .emit = stderrSinkEmit'
  require_literal "$daemon_main" 'log_sinks.registerSink(.{ .emit = otlpSinkEmit'
  require_literal "$http_server" 'otel_traces.enqueueSpan(span);'
  require_literal "$metering" 'otel_traces.enqueueSpan(span);'
  require_literal "$report_service" 'otel_metrics.recordRunSettlement'
  require_literal "$serve" 'capture(telemetry_mod.ServerStarted'
  require_literal "$workspace_lifecycle" 'capture(telemetry_mod.WorkspaceCreated'
  require_literal "$webhook_fleet" 'capture(telemetry_mod.FleetTriggered'
  require_literal "$webhook_github" 'capture(telemetry_mod.FleetTriggered'
  require_literal "$signup" 'capture(telemetry_mod.SignupBootstrapped'
  # --multiline: the shipped capture wraps its arguments across lines, so a
  # single-line pattern would silently report the event as declared-only.
  if rg --glob '*.zig' --glob '!**/*_test.zig' --multiline --quiet -- 'capture\(\s*telemetry(_mod)?\.FleetCompleted' "$repo_root/src/agentsfleetd"; then
    require_literal "$observability" "| PostHog \`FleetCompleted\` | installed and called when configured |"
  else
    require_literal "$observability" "| PostHog \`FleetCompleted\` | declared-only |"
  fi
  require_tree_absent "$repo_root/src/runner" 'otel_logs|otel_traces|otel_metrics|PostHog|posthog'
}

check_volume() {
  local row
  local -a rows=(
    $'| runner logs | steady | `L` records/s, each structured runner record at most 4096 bytes | local stderr only; repository network queue and remote bytes remain zero |'
    $'| runner logs | burst | arbitrary `L` until the host supervisor applies its byte/rate policy | no `agentsfleetd` load; local host retention is the sole current bound |'
    $'| runner logs | backend outage | unchanged local record rate | no application retry or queue; an enabled collector uses its declared disk ceiling then drops |'
    $'| runner logs | fleet growth | sum of host-local `L`; no central application aggregation | each host remains isolated; direct collection stays disabled until numeric host policy and redaction proof exist |'
    $'| control-plane logs | steady | `D` accepted structured records/s, each OTLP body truncated at 512 bytes | stderr remains authoritative; 2047 usable queue slots absorb the rate, and a cycle drains its whole starting backlog rather than one 50-record batch |'
    $'| control-plane logs | burst | arbitrary `D` from bounded control-plane events | non-blocking admission fills at 2047 records, then drops and exports overflow as `ring_full`; the consumer wakes at 50 and drains the cycle-start backlog |'
    $'| control-plane logs | backend outage | unchanged `D` while the endpoint is unavailable | the fixed ring fills, later entries drop, and product work continues; exporter warnings stay stderr-only and never re-enter the OTLP log ring |'
    $'| control-plane logs | fleet growth | `D` follows semantic control-plane events, never the raw runner byte stream | queue capacity remains 2047 process-wide; the logging allowlist excludes prompt, body, token, credential, environment, and arbitrary error fields |'
    $'| metrics | steady | idle heartbeats update memory but enqueue zero OTLP samples; each billed lease enqueues one credit sample and each accepted report enqueues at most five | 4096 runner slots; 1023 usable OTLP sample slots |'
    $'| metrics | burst | at most `B + 5C` OTLP samples/s before coalescing | non-blocking ring admission; overflow drops and is counted |'
    $'| metrics | backend outage | at most `B + 5C` attempted samples/s | ring holds 1023; later entries drop; request, debit, and settlement paths continue |'
    $'| metrics | fleet growth | `R` liveness keys plus debit and report samples | 4096 exact runner slots regardless of `R`; overflow counters aggregate into `_other`, while last-seen and active-lease gauges drop |'
    $'| traces | steady | `R/10` heartbeat requests/s before lease and run traffic | route policy admits at most 10 generic request spans/s plus one settled delivery span per accepted run |'
    $'| traces | burst | arbitrary matched requests plus `C` settled runs/s | generic spans keep the fixed 4 rejection, 4 server-error, and 2 sampled-success budget; the 1023-slot ring bounds all selected spans |'
    $'| traces | backend outage | selected generic spans plus one delivery span per accepted run | ring fills to 1023, then selected spans drop without blocking product work |'
    $'| traces | fleet growth | heartbeat input grows linearly with `R`; generic output does not | generic request output remains 10 spans/s process-wide and exact suppressions aggregate in Prometheus |'
  )
  for row in "${rows[@]}"; do
    require_literal "$observability" "$row"
  done
}

check_limits() {
  require_literal "$log_exporter" 'const BUFFER_CAPACITY: usize = 2048;'
  require_literal "$log_exporter" 'const FLUSH_BATCH_SIZE: usize = 50;'
  require_literal "$log_exporter" 'const MAX_MSG_LEN: usize = 512;'
  require_literal "$trace_exporter" 'const BUFFER_CAPACITY: usize = 1024;'
  require_literal "$trace_exporter" 'const FLUSH_BATCH_SIZE: usize = 50;'
  require_literal "$metric_exporter" 'const BUFFER_CAPACITY: usize = 1024;'
  require_literal "$metric_aggregate" 'pub const MAX_SERIES: usize = 256;'
  require_literal "$runner_metrics" 'pub const MAX_SLOTS: usize = 4096;'
  require_literal "$shared_exporter" 'flush_interval_ms: u64 = 5_000,'
  require_literal "$preflight" '.flush_interval_ms = 10_000,'
  require_literal "$preflight" '.flush_at = 20,'
  require_literal "$preflight" '.max_retries = 3,'
  require_literal "$build_zon" 'github.com/agentsfleet/posthog-zig.git#v0.2.0'
  require_literal "$observability" '| logs | 2047 records; body truncated at 512 bytes | wakes at 50 accepted records or 5 seconds, then drains the cycle-start backlog in 50-record batches |'
  require_literal "$observability" '| traces | 1023 spans; 12 attributes per span | wakes at 50 accepted spans or 5 seconds, then drains the cycle-start backlog in 50-span batches |'
  require_literal "$observability" '| OTLP metrics | 1023 samples; at most 256 coalesced series | wakes at 768 accepted samples or 5 seconds, then drains the cycle-start backlog and coalesces label sets |'
  require_literal "$observability" '| PostHog | 1000 events per double-buffer side; up to 2000 resident | 20 events or 10 seconds; three retries |'
  require_literal "$observability" $'4096 exact runner slots regardless of `R`'
}

check_log_route() {
  require_literal "$observability" 'agentsfleetd` never accepts, stores, or relays raw runner'
  require_literal "$runner_fleet" "the path bypasses \`agentsfleetd\`"
  require_literal "$runner_fleet" 'Activity frames remain user-visible run output and are never reused as a log stream.'
  require_absent "$observability" 'runner log ingestion endpoint'
  require_tree_absent "$repo_root/src/agentsfleetd/http" 'runner_log|/v1/runners/me/logs'
  require_absent "$runner_protocol" 'RunnerLog'
  require_absent "$runner_protocol" 'runner_log'
  require_absent "$runner_protocol" 'log_lines'
  require_absent "$runner_activity" 'RunnerLog'
  require_absent "$runner_activity" 'runner_log'
  require_absent "$runner_activity" 'log_lines'
}

check_collector() {
  require_literal "$observability" 'The optional host collector owns its backend credential and disk queue.'
  require_literal "$observability" 'single-line logfmt records after applying an allowlist'
  require_literal "$observability" 'application-owned collector queue is therefore zero bytes.'
  require_literal "$observability" 'drops prompts, response bodies, tokens, credentials, environment values,'
  require_literal "$observability" 'fixed level- or rate-based admission applied after redaction'
}

check_metric_source() {
  require_literal "$observability" 'fixed in-memory state after it accepts heartbeat, lease, and report operations.'
  require_literal "$observability" 'One terminal report can also enqueue at most five OTLP samples'
  require_literal "$report_service" 'otel_metrics.recordRunSettlement'
  require_literal "$report_service" 'metrics_runner.decRunnerActiveLeases(runner_id);'
  require_literal "$billing_service" 'otel_metrics.recordCreditDrain'
  require_literal "$lease_service" 'metrics_runner.incRunnerActiveLeases(runner_id);'
  require_literal "$heartbeat_handler" 'metrics_runner.touchRunnerSeen(runner_id);'
  require_literal "$runner_fleet" 'gauge     +1 on grant, −1 on terminal report'
}

check_metric_cardinality() {
  require_literal "$runner_metrics" 'pub const MAX_SLOTS: usize = 4096;'
  require_literal "$runner_metrics" 'const ID_OTHER = "_other";'
  require_literal "$observability" "4096 exact runner slots regardless of \`R\`"
  require_literal "$observability" "overflow counters aggregate into \`_other\`, while last-seen and active-lease gauges drop"
  require_literal "$observability" $'| `runner_id` | at most 4096 exact values | counters merge into `_other`; gauges drop |'
  require_literal "$observability" $'| `workspace` | retained; first 100 distinct values per process | later workspaces emit the sample without the label (`otel_metrics_cardinality.zig`) |'
  require_literal "$observability" $'| `direction` | `input`, `cached`, `output` | closed set; no overflow value |'
  require_literal "$observability" $'| `posture` | `platform`, `self_managed` | closed set; no overflow value |'
  require_literal "$observability" $'| `model` | retained; value truncated at 64 bytes | no per-catalog cap; bounded only by the 256-series flush ceiling |'
  # The guard must still exist in source; the correction preserved both labels.
  require_literal "$repo_root/src/agentsfleetd/observability/otel_metrics_cardinality.zig" 'pub const WORKSPACE_CARDINALITY_CAP: usize = 100;'
}

check_trace_map() {
  require_literal "$observability" 'adds no trace field to lease, renew, activity, or'
  require_literal "$observability" "trace getter returns null and its setter is a no-op"
  require_literal "$observability" 'lease rule intentionally covers both empty'
  require_literal "$observability" "useful run work retains one settled \`fleet.delivery\` span"
  require_literal "$runner_fleet" 'A future runner span producer must define sampling and a fixed span budget'
}

check_trace_budget() {
  require_literal "$observability" 'admits at most 10 generic request spans per monotonic second'
  require_literal "$observability" 'four runner'
  require_literal "$observability" 'four server errors, and two sampled successes'
  require_literal "$observability" 'server-generated span identifier, not caller-controlled trace input'
  require_literal "$observability" "\`agentsfleet_http_trace_suppressed_total\`"
  require_literal "$handler_common" 'return TraceContext.generate();'
  require_literal "$observability" $'A missing or malformed `traceparent` starts a new local root'
  require_literal "$observability" '| traces | backend outage |'
}

check_architecture() {
  require_literal "$observability" "raw runner logs bypass \`agentsfleetd\`"
  require_literal "$runner_fleet" 'Raw runner logs do not ride those verbs.'
  require_literal "$observability" 'the fixed ring fills, later entries drop, and product work continues'
  require_literal "$runner_fleet" "PostHog remains \`agentsfleetd\` product analytics."
}

# Pins the SHIPPED bounding policy to source. This case used to assert prose in
# the workstream spec; a spec is archived once it lands, so the durable audit
# asserts the code that actually enforces the policy.
check_shipped_export() {
  # Coalesced wake: producers count accepted pushes and arm one standard event.
  require_literal "$shared_exporter" 'pub fn notifyAccepted() void {'
  require_literal "$shared_exporter" 'var g_event: std.Io.Event = .unset;'
  require_literal "$shared_exporter" 'wake_threshold: u32 = 50,'
  require_literal "$metric_exporter" '.wake_threshold = 768,'

  # One attempt per destructively collected batch, raced against a boot-clock
  # deadline — never a retry, which would double-count delta metrics.
  require_literal "$otlp_client" 'std.Io.Select(Selected).init(self.io, &result_buf)'

  # Fixed exporter-health surface: two families, three signals, six reasons.
  require_literal "$otlp_health" 'pub const QUEUE_DEPTH_NAME = "agentsfleet_otlp_queue_depth";'
  require_literal "$otlp_health" 'pub const DISCARDED_NAME = "agentsfleet_otlp_entries_discarded_total";'
  local reason
  for reason in ring_full aggregate_cap serialize_failed partial_rejected export_rejected export_uncertain; do
    require_literal "$otlp_health" "    $reason,"
  done

  # Bounded trace admission and its exported suppression counter.
  require_literal "$trace_policy" 'const RUNNER_REJECTION_LIMIT: u32 = 4;'
  require_literal "$trace_policy" 'const SERVER_ERROR_LIMIT: u32 = 4;'
  require_literal "$trace_policy" 'const SAMPLED_SUCCESS_LIMIT: u32 = 2;'
  require_literal "$trace_policy" 'const SAMPLE_DENOMINATOR: u64 = 100;'
  require_literal "$trace_metrics" 'pub const SUPPRESSED_NAME = "agentsfleet_http_trace_suppressed_total";'

  # Completion capture is deterministic and sits behind the fenced claim.
  require_literal "$report_service" 'captureCompletion(hx, lease, body);'
  # shellcheck disable=SC2016  # $insert_id is PostHog's literal key, not a shell var.
  require_literal "$telemetry_events" 'const S_INSERT_ID = "$insert_id";'
  require_literal "$telemetry_events" 'std.crypto.hash.sha2.Sha256'

  require_literal "$quality_make" 'check-signal-routing:'
  require_literal "$quality_make" 'check-deploy-safety check-signal-routing'
}

run_case() {
  case "$1" in
    inventory) check_inventory ;;
    volume) check_volume ;;
    limits) check_limits ;;
    log-route) check_log_route ;;
    collector) check_collector ;;
    metric-source) check_metric_source ;;
    metric-cardinality) check_metric_cardinality ;;
    trace-map) check_trace_map ;;
    trace-budget) check_trace_budget ;;
    architecture) check_architecture ;;
    shipped-export) check_shipped_export ;;
    *) fail "unknown case '$1'" ;;
  esac
}

if [[ "${1:-all}" == 'all' ]]; then
  for audit_case in inventory volume limits log-route collector metric-source metric-cardinality trace-map trace-budget architecture shipped-export; do
    run_case "$audit_case"
  done
else
  run_case "$1"
fi

printf 'signal-routing audit passed\n'
