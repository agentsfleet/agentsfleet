<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M152_001: Ten-folder audit remediation — leaks closed, outbound reads capped, duplicates folded

**Prototype:** v2.0.0
**Milestone:** M152
**Workstream:** 001
**Date:** Jul 31, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the daemon leaks heap on every Clerk signup webhook and during every credential-vendor outage; operator-facing memory growth
**Categories:** API
**Batch:** B1 — single workstream
**Branch:** feat/m152-audit-remediation
**Test Baseline:** unit=3276 integration=501
**Depends on:** none
**Provenance:** LLM-drafted (claude-fable-5, Jul 31, 2026) — sourced from the Jul 30, 2026 four-agent audit of `src/agentsfleetd/{auth,cli,cmd,config,credentials,cron,crypto,db,errors,events}`; every finding was grep-verified against `main`
**Canonical architecture:** `docs/architecture/memory.md` (allocator ownership) · `docs/architecture/concurrency.md` (stop paths)

---

## Overview

**Goal (testable):** every finding from the Jul 30 ten-folder audit is closed by a test-proven fix — the three `std.Io.Writer.Allocating.fromArrayList` sites free their accumulator on all paths under allocation-failure injection, the five errdefer-ladder sites survive `checkAllAllocationFailures`, outbound reads reject at named byte caps, hostile `expires_in` yields `.mint_failed` instead of a panic, doctor and the runtime loader share one secret predicate, dead `pub` surface is gone, and duplicated helpers are single-sourced — with `make lint`, `make test`, and both Linux cross-compiles green.
**Problem:** the daemon leaks the full Clerk response body on every signup webhook, leaks partial response bytes on every failed credential mint (exactly during vendor outages), can be panicked by a hostile token-endpoint response, spawns an unbounded detached thread per webhook, and carries grep-confirmed dead `pub` surface plus duplicated validation logic that has already drifted (doctor green-lights secrets the daemon rejects at boot).
**Solution summary:** fix-in-kind across the ten audited folders plus the root-cause files the findings land in: adjacent `deinit`/`errdefer` for every `Writer.Allocating`, per-acquisition errdefer ladders, named byte caps on outbound reads, range-clamped provider input, a bounded Clerk worker with a stop path, zeroized secret frees, one shared 64-hex predicate, a dead-`pub` sweep, and duplication folds — plus one new `lint-zig.py` check that mechanizes the leak class that produced both high-severity findings.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(agentsfleetd): close audit leak class, cap outbound reads, fold duplicates
- **Intent (one sentence):** the daemon stops leaking under webhook traffic and vendor outages, cannot be panicked by hostile provider responses, and sheds the dead surface and duplicated validation the audit exposed.
- **Handshake** (filled at PLAN, Jul 31 2026): every audited finding is closed in kind at its site, mirroring a proven in-repo pattern, with a test in the same commit; the daemon must stop leaking under webhook traffic and vendor outages and must survive hostile provider input. `ASSUMPTIONS I'M MAKING:` (1) all 46 findings including lows are in scope; (2) root-cause files outside the ten folders are in scope where a finding lands there; (3) the four extra `Writer.Allocating` sites the new lint exposed (`http/test_harness_server`, `http/test_http_message`, `http/handlers/cross_workspace_idor_test`, `runner/cmd/help`) fold in — the lint invariant is incoherent with known-red sites; (4) the Clerk worker bound stays inside the auth portability wall (no `call_deadline` import) — bounded in-flight count + bounded shutdown drain instead of a joinable deadline-armed worker; the alternative is recorded in Discovery for Indy.

## Implementing agent — read these first

1. `~/Projects/dotfiles/dispatch/write_zig.md` — errdefer ladder (A2), Allocator Choice, safe-because comment rule, buffer selection; the rulebook every fix must satisfy.
2. `src/agentsfleetd/observability/metrics_render.zig` — the in-repo correct `Writer.Allocating` shape (`errdefer aw.deinit()`); mirror it at all three leak sites.
3. `src/agentsfleetd/cron/QStashClient.zig` — the in-repo correct capped-read pattern (named max-bytes constant + capped drain) to mirror for the JSON Web Key Set (JWKS) and Clerk fetches.
4. `src/agentsfleetd/secrets/secure_memory.zig` — the zeroizing free surface credential teardown must route through.
5. `src/agentsfleetd/cmd/serve_shutdown.zig` — the house stop-path/atomics shape to mirror for the bounded Clerk worker.

## Files Changed (blast radius)

All paths below are under `src/agentsfleetd/` unless rooted otherwise.

| File | Action | Why |
|------|--------|-----|
| `auth/clerk_backend.zig` | EDIT | free accumulator all paths; bounded worker + timeout + stop path; capped read |
| `auth/jwks.zig` | EDIT | error-path free; capped read; fallible init (no panic); drop dead re-export |
| `auth/{oidc,claims,jwks_standard_claims}.zig` | EDIT | errdefer ladders; namespace-constant collapse; fallible-init cascade |
| `auth/{api_key,audit,audit_events,bearer_or_api_key,jwks_test}.zig` | EDIT | duplicate compare deleted; de-pub; preamble extraction; redundant re-parse dropped; fixture consumption; safe-because |
| `auth/jwks_test_fixtures.zig` | CREATE | single-source the triplicated JWKS test fixture |
| `auth/jwks_fetch.zig` | CREATE | bounded JWKS transport split from jwks.zig by concern (file cap) |
| `auth/clerk_fetch_worker.zig` | CREATE | bounded in-flight accounting + shutdown drain for the Clerk workers |
| `auth/clerk_metadata_payload.zig` | CREATE | pure payload rendering split from clerk_backend (file cap) |
| `auth/tests.zig` | EDIT | portability aggregate discovers the new modules |
| `http/test_harness.zig` + `http/runner_enrollment_integration_test.zig` | EDIT | fallible verifier-init cascade |
| `http/handlers/integration_grants/handler.zig` | EDIT | migrate straggler caller to canonical compare |
| `credentials/{serve_broker,broker}.zig` | EDIT | free accumulator on error exits; zeroized frees; truthful telemetry; de-pub |
| `credentials/{broker_flight,integration_oauth_refresh}.zig` | EDIT | bounded waiter wait; range-clamp `expires_in`; token path per handle |
| `crypto/hmac_sig.zig` | EDIT | zero keyed state after final |
| `cmd/preflight.zig` + `observability/otlp/{exporter,config}.zig` | EDIT | parse config once; ownership on every install outcome; de-pub per-signal fns; name literals |
| `cmd/{doctor,serve,migrate}.zig` | EDIT | shared hex predicate; delegate Redis username parse; run splits |
| `cmd/{common,serve_args}.zig` | EDIT | shared env-bool parser; de-pub iterator |
| `config/{load,env_vars,runtime_loader,runtime_loader_test}.zig` | EDIT | shared trim-set + env-bool + hex predicate; named size defaults; dotenv error names the line |
| `queue/redis_config.zig` | EDIT | expose username parse for doctor parity |
| `db/{pool,pool_migration_lock,pool_migration_state,sql_splitter,pg_query}.zig` | EDIT | URL-parse ladder; dead role/alias/wrapper removal; fn split; relocated pin test |
| `db/{test_fixtures,test_fixtures_uc1,pool_test}.zig` + the four index/liveness `db/*_integration_test.zig` suites | EDIT | shared EXPLAIN scaffolding; alias + dead ids dropped; conn-helper fold |
| `events/{fleet_set_cache,subscription,subscription_hub_reader,bus}.zig` | EDIT | enumerate ladder; lock defer; safe-because; fn split; post-stop drops counted (the hub itself needed no edit — its audited items landed in the reader) |
| `events/fleet_set_cache_test.zig` | EDIT | schema-qualified fixtures; complete cleanup |
| `errors/{error_registry,error_entries,error_entries_runtime,error_registry_test}.zig` | EDIT | dead constant dropped; stale comment fixed; helpers single-sourced; foreign pin tests relocated |
| `state/tenant_billing_rates.zig` + `fleet/service.zig` | EDIT | receive relocated pin tests |
| `cron/{Service,QStashClient,Credentials,FireQueue}.zig` | EDIT | de-pub constants; finalize fallback logs + preserves original error; buffer sized to data; oversized-token distinct outcome; reply-free allocator alignment |
| `lint-zig.py` (repo root) | EDIT | new check: `fromArrayList` without adjacent deinit/errdefer fails lint |
| `scripts/check_allocating_writer_test.py` | CREATE | fixture proof the new check bites (SCRIPT_SELF_TESTS discovery) |
| `make/quality.mk` | EDIT | pg-drain recipe names the allocating-writer check |
| `http/{test_harness_server,test_http_message}.zig` + `http/handlers/cross_workspace_idor_test.zig` + `src/runner/cmd/help.zig` | EDIT | same leak class exposed by the new lint — folded to keep the invariant coherent |
| `main.zig` | EDIT | consumes the dotenv diagnostic — boot error names the line |
| `cmd/serve_boot.zig` | CREATE | process-exiting boot prologue split out of serve.run (fn cap) |
| `credentials/testing.zig` | EDIT | RecordingMetrics captures latency for the telemetry proof |
| Sibling `*_test.zig` of the rows above | EDIT | per-Dimension tests; allocation-failure sweeps |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (no dead code at write time: every removal grep-proven), ORP (orphan sweep on deletions), UFS (repeated/semantic literals become named constants), FLL (file ≤350 / fn ≤50 / method ≤70 on every touched file), NLR (touch-it-fix-it applies only inside Files Changed — no opportunistic reach beyond this table).
- `~/Projects/dotfiles/dispatch/write_zig.md` — A2 errdefer ladder, A5 ownership phrases + deinit poisoning on touched types, Memory Safety Rules, Concurrency (safe-because comments, lock/defer), Buffer Type Selection, Panic policy (no `@panic` for recoverable failure), Progressive Cleanup (de-pub on touch).
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — the new dual-finalize and dotenv log lines conform.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every fix is Zig | cross-compile both Linux targets; drain rules untouched; safe-because on every weak ordering |
| PUB / Struct-Shape | yes — de-pub sweep + new shared helpers | each de-pub verified by zlint unused-decls; each new helper has ≥1 external consumer at introduction |
| File & Function Length (≤350/≤50/≤70) | yes — five over-cap fns split | split via existing seams; no touched file crosses 350 |
| UFS (repeated/semantic literals) | yes — caps, trim-set, size defaults, endpoint literals | named constants at single owners; tests reference the names |
| UI Substitution / DESIGN TOKEN | no — no UI surface | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes (two new log lines); ERROR REGISTRY no (no new codes); SCHEMA no | new logs follow the standard; reuse existing error codes |

## Prior-Art / Reference Implementations

- **Reference:** in-repo correct shapes — `observability/metrics_render.zig` (accumulator cleanup), `cron/QStashClient.zig` (capped reads), `db/reclaim`/`approval_gate` sweepers (A2 ladder), `cmd/serve_shutdown.zig` (stop paths + annotated atomics). Every fix mirrors an existing proven pattern; nothing greenfield.

## Sections (implementation slices)

### §1 — Leak class

Close every audited leak: the three `Writer.Allocating.fromArrayList` sites (ownership transfer makes the original list's deinit a no-op), the five multi-dupe errdefer-ladder gaps, and the OpenTelemetry (OTel) exporter install seam that drops its config on failure outcomes and re-parses the environment three times. Then mechanize the class. **Implementation default:** fix shape is the `metrics_render.zig` pattern — adjacent `defer`/`errdefer aw.deinit()` — because it is the proven in-repo idiom.

- **Dimension 1.1** — Clerk fetch frees its accumulator on success, fetch-error, and Out of Memory (OOM) paths (implemented by deleting the accumulator: the body is never read, so fetch stream-discards) → Test `test_clerk_fetch_frees_body_all_paths` — **DONE**
- **Dimension 1.2** — mint post frees its accumulator on every early exit (deadline fire, reset, OOM) → Test `test_mint_post_frees_body_on_error` — **DONE**
- **Dimension 1.3** — JWKS fetch frees the partial body when the fetch errors mid-stream → Test `test_jwks_fetch_frees_partial_on_error` — **DONE**
- **Dimension 1.4** — auth claim materialization (claims, standard-claims, oidc verified-set) survives `checkAllAllocationFailures` with zero leaks; OOM now propagates as OOM instead of collapsing into token-malformed (error-class fix the sweep forced) → Test `test_auth_claim_ladders_alloc_failure` — **DONE**
- **Dimension 1.5** — database URL parse survives `checkAllAllocationFailures` → Test `test_pool_parse_url_alloc_failure` — **DONE**
- **Dimension 1.6** — fleet-set enumerate survives an injected-allocation-failure sweep with zero leaks (integration tier; the cache swallows refresh errors by design, so the sweep drives the backing leak detector, not error propagation) → Test `test_fleet_set_enumerate_alloc_failure` — **DONE**
- **Dimension 1.7** — exporter config is parsed once per boot and owned (freed) on every install outcome including already-running and spawn-failed → Test `test_otlp_install_failure_frees_config` — **DONE**
- **Dimension 1.8** — `lint-zig.py` fails a fixture containing `fromArrayList` with no adjacent deinit/errdefer and passes the fixed sites → Test `test_lint_fromarraylist_check` — **DONE**

### §2 — Outbound hardening

Outbound HTTP reads reject at named byte caps (mirror the QStash client's capped-read shape); provider-controlled numbers are range-clamped; the per-webhook detached thread becomes a bounded worker with a timeout and a named stop path joined at shutdown; single-flight waiters cannot park forever; boot-path OOM returns an error instead of aborting.

- **Dimension 2.1** — JWKS responses larger than the named cap are rejected (bounded chunked read in the new `jwks_fetch.zig`); the Clerk side retains zero bytes by construction (stream-discard, §1.1) → Test `test_outbound_reads_reject_over_cap` — **DONE**
- **Dimension 2.2** — `expires_in` of non-finite, negative, or over-range value returns `.mint_failed = .permanent`, never a panic; in-range floats still accepted → Test `test_expires_in_hostile_values_permanent` — **DONE**
- **Dimension 2.3** — Clerk metadata fetches are bounded by a named in-flight cap (burst beyond it is rejected and logged) and shutdown performs a bounded drain; stragglers own only self-lifetime memory. The deadline-armed joinable-worker alternative needs a `call_deadline` import through the auth portability wall — recorded in Discovery for Indy's call. → Test `test_clerk_worker_bounded_drains_at_shutdown` — **DONE**
- **Dimension 2.4** — a single-flight waiter whose winner never finishes wakes at a named deadline and returns transient failure (deadline-tracked poll; the condition variable it replaced was removed as dead concurrent code) → Test `test_flight_waiter_bounded_wait` — **DONE**
- **Dimension 2.5** — JWKS verifier init propagates allocation failure as an error (no `@panic`), cascading through the oidc verifier init and every caller → Test `test_verifier_init_oom_errors` — **DONE**

### §3 — Secret hygiene

Secret material is zeroed before release everywhere it is owned: credential teardown (private-key material, client secrets, cached/minted tokens) routes through the existing zeroizing free, and the keyed Hash-based Message Authentication Code (HMAC) state is zeroed after finalization — matching what the vault load path already does.

- **Dimension 3.1** — credential secret frees route through `secure_memory` and remain leak-free under the testing allocator → Test `test_credential_frees_zeroize_leak_free` — **DONE**
- **Dimension 3.2** — MAC computation still matches known vectors after state zeroing (regression) → Test `test_hmac_vectors_unchanged` — **DONE**

### §4 — Validation drift

One predicate, one parser, one grammar: doctor and the runtime loader validate 64-hex secrets with the same shared predicate; doctor's Redis username extraction delegates to the queue module's URL parser; environment booleans parse with a single shared grammar.

- **Dimension 4.1** — doctor rejects a 64-char non-hex key exactly as the loader does, via one shared predicate → Test `test_doctor_rejects_non_hex_key` — **DONE**
- **Dimension 4.2** — doctor's Redis username matches the queue parser's for every URL shape including colonless userinfo → Test `test_doctor_redis_username_parity` — **DONE**
- **Dimension 4.3** — the two env-boolean call sites accept the identical (trimmed) grammar → Test `test_env_bool_single_grammar` — **DONE**

### §5 — Dead surface sweep

Remove or de-pub every grep-confirmed dead symbol from the audit: the uncalled migration-lock wrapper, the unused role variant and its env mapping, the unused row alias and test-only counter, the duplicate namespace constant, dead re-exports, per-signal exporter functions, iterator, lease/response/vault-key constants, the dead bearer-prefix constant, and the compat-alias fixture ids. De-pub items are proven by zlint unused-decls; deletions by the Dead Code Sweep greps.

- **Dimension 5.1** — every listed symbol is removed or de-pub'd; sweep greps return zero; lint stays green → Test Dead Code Sweep greps + `make lint` — **DONE** (per-signal exporter fns were already deleted by §1.7's preflight rework; `constantTimeEql` copy lands with §6.1)

### §6 — Duplication folds

Single-source every duplicated helper the audit found, keeping behavior pinned by the existing suites.

- **Dimension 6.1** — constant-time compare exists only in `crypto/hmac_sig.zig`; the straggler handler caller migrates; the auth copy is deleted → Test `test_constant_time_eql_single_source` — **DONE**
- **Dimension 6.2** — trim-set and loader size defaults have one named owner each; the test file's re-declared copies are gone → Test `test_config_constants_single_owner` — **DONE**
- **Dimension 6.3** — EXPLAIN scaffolding lives in `db/test_fixtures.zig`; the four index suites and the pool-test conn helper consume shared fixtures; fleet-set-cache fixtures are schema-qualified and clean up fully → Test the four suites + fleet-set suite green with zero leftover rows — **DONE** (suite-green proof lands with the VERIFY-stage `make test-integration` run)
- **Dimension 6.4** — the JWKS test fixture is single-sourced in the new fixtures module; three consumers → Test `test_jwks_fixture_single_source` — **DONE**
- **Dimension 6.5** — error-entry constructor helpers exist once; both entry files consume them → Test existing registry suite green — **DONE**

### §7 — Remaining hygiene

The audited low-severity drift, fixed in kind.

- **Dimension 7.1** — events hygiene: drop-note lock uses defer; three weak-ordering loads carry safe-because comments; post-stop publish increments the drop counter instead of enqueueing → Test `test_bus_post_stop_counts_drop` — **DONE**
- **Dimension 7.2** — structure-only refactors (serve/doctor/migrate run splits, inspect split, resubscribe split, audit-event preamble extraction, pin-test relocations) change no behavior: full suite green, no fn over cap in touched files → Test `make test` + length check rubric row — **DONE** (residual: `serve.run` is down from 290 to ~180 lines via the new `serve_boot.zig` prologue split; the remainder is the defer-ordered resource graph whose full split would restructure shutdown choreography — flagged in Discovery for Indy)
- **Dimension 7.3** — mint telemetry is truthful: real latency from the injected clock; cache-dupe OOM emits a mint-failed event; an oversized vault token logs a config-shaped error distinct from a provider reject → Test `test_broker_telemetry_truthful` — **DONE** (the oversized QStash token gets a distinct `credential_invalid` outcome + config-shaped persisted detail — the client is deliberately log-free)
- **Dimension 7.4** — a malformed dotenv line fails boot naming the line number → Test `test_dotenv_error_names_line` — **DONE**
- **Dimension 7.5** — a lost lease during finalize fallback is logged and the original provider/store error is preserved → Test `test_finalize_fallback_preserves_error` — **DONE** (void-by-type: the caller's `return err` cannot be masked; review-hardened test proves the live-lease fallback WRITES failed+detail and the lost-lease path touches nothing)
- **Dimension 7.6** — the OAuth token path is per-handle configuration with the current value as default; existing providers unchanged → Test `test_token_path_per_handle_default` — **DONE**
- **Dimension 7.7** — fire-queue reply frees use the connection's allocator by construction, not by caller convention → Test existing fire-queue suite green under testing allocator — **DONE**

## Interfaces

```
Changed public surfaces (everything else stays shape-identical):
- auth/jwks Verifier.init and auth/oidc Verifier.init become fallible (error on OOM).
- auth/api_key loses its pub constant-time compare; consumers use crypto/hmac_sig.
- queue/redis_config exposes its username-extraction for doctor's parity check.
- config gains one shared 64-hex predicate + one shared env-bool parser (single owners).
- observability/otlp exposes a parse-once config surface; install outcomes own or free it.
- credentials mint events carry real latency_ms; wire shape of the event is unchanged.
HTTP endpoints, OpenAPI, CLI, schema: no changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Vendor stalls mid-body | Clerk/JWKS/mint endpoint hangs or resets | deadline fires; accumulator freed; caller sees existing fetch-failed error; no heap growth |
| Oversized response | endpoint returns more than the named cap | read rejected at cap; connection cleaned; fetch-failed error |
| Hostile `expires_in` | token endpoint returns non-finite/over-range number | `.mint_failed = .permanent`; no panic |
| OOM mid-materialization | allocation fails between dupes | errdefer ladder frees earlier fields; error propagates |
| Webhook burst | many signups at once | bounded worker; beyond the bound rejected/queued at a named limit; no unbounded threads |
| Winner never finishes | single-flight owner hangs past its deadline | waiter wakes at named deadline; transient failure |
| Post-stop publish | emitter races bus stop | drop counted; nothing enqueued |
| Lost lease in finalize | lease expired during error recovery | logged with original error preserved |
| Malformed dotenv line | bad `.env.local` | boot fails naming the line number |

## Invariants

1. Every `Writer.Allocating` construction has an adjacent deinit/errdefer — enforced by the new `lint-zig.py` check in `make lint`.
2. Outbound response reads are bounded by named cap constants — enforced by the cap tests (2.1/2.2) and UFS naming.
3. No `pub` without an external consumer — enforced by zlint unused-decls (already `error`).
4. No function over the length caps in touched files — enforced by the LENGTH gate in `make harness-verify`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| mint event `latency_ms` (existing event, now truthful) | ops | every credential mint outcome | duration, outcome, provider kind | no token/secret material | `test_broker_telemetry_truthful` |
| bus drop counter (existing, new increment path) | ops | publish after stop | count only | none needed | `test_bus_post_stop_counts_drop` |

No other product/operator signal changes; no analytics/funnel playbook update required (internal daemon hygiene).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_clerk_fetch_frees_body_all_paths` | success/error/OOM fetch paths leak nothing under testing allocator |
| 1.2 | unit | `test_mint_post_frees_body_on_error` | injected fetch failure mid-body leaks nothing |
| 1.3 | unit | `test_jwks_fetch_frees_partial_on_error` | mid-stream error leaks nothing |
| 1.4 | unit | `test_auth_claim_ladders_alloc_failure` | `checkAllAllocationFailures` over the three claim fns: zero leaks, state rolled back |
| 1.5 | unit | `test_pool_parse_url_alloc_failure` | every allocation-failure point frees earlier dupes |
| 1.6 | unit | `test_fleet_set_enumerate_alloc_failure` | append-failure frees the orphan dupe |
| 1.7 | unit | `test_otlp_install_failure_frees_config` | already-running outcome freed under the leak detector; spawn-failed + one-parse-per-boot hold structurally (the returned handle owns cfg on every outcome; one configFromEnv call site) — noted per review |
| 1.8 | unit | `test_lint_fromarraylist_check` | leaking fixture fails, fixed sites pass |
| 2.1 | unit | `test_outbound_reads_reject_over_cap` | JWKS and Clerk bodies > cap → error, cleanup, no unbounded growth |
| 2.2 | unit | `test_expires_in_hostile_values_permanent` | 1e300 / NaN / negative → `.mint_failed = .permanent` |
| 2.3 | unit | `test_clerk_worker_bounded_drains_at_shutdown` | burst beyond the in-flight cap rejected; shutdown drain returns once in-flight work completes |
| 2.4 | unit | `test_flight_waiter_bounded_wait` | stuck winner → waiter returns transient at deadline |
| 2.5 | unit | `test_verifier_init_oom_errors` | failing allocator → error, not abort |
| 3.1 | unit | `test_credential_frees_zeroize_leak_free` | teardown through zeroizing free; testing allocator reports no leak |
| 3.2 | unit | `test_hmac_vectors_unchanged` | known MAC vectors still match |
| 4.1 | unit | `test_doctor_rejects_non_hex_key` | 64-char non-hex → doctor red, loader red, same predicate |
| 4.2 | unit | `test_doctor_redis_username_parity` | URL shapes incl. colonless userinfo → identical extraction |
| 4.3 | unit | `test_env_bool_single_grammar` | `" true"` accepted identically at both call sites |
| 5.1 | unit | Dead Code Sweep greps + `make lint` | zero matches; lint green |
| 6.1 | unit | `test_constant_time_eql_single_source` | one definition; all callers resolve to it; timing property test still green |
| 6.2 | unit | `test_config_constants_single_owner` | one declaration site per constant |
| 6.3 | integration | four index suites + fleet-set suite | green; fleet-set fixtures leave zero rows behind |
| 6.4 | unit | `test_jwks_fixture_single_source` | three consumers import the fixtures module |
| 6.5 | unit | existing registry suite | green after helper fold |
| 7.1 | unit | `test_bus_post_stop_counts_drop` | publish after stop → drop counter +1, queue unchanged |
| 7.2 | unit | `make test` + length check | full suite green; no fn over cap in touched files |
| 7.3 | unit | `test_broker_telemetry_truthful` | injected clock → nonzero latency; OOM path emits; oversized token logs config error |
| 7.4 | unit | `test_dotenv_error_names_line` | line 3 malformed → error mentions line 3 |
| 7.5 | unit | `test_finalize_fallback_preserves_error` | lease lost during recovery → original error returned, loss logged |
| 7.6 | unit | `test_token_path_per_handle_default` | absent config → current default path; custom path honored |
| 7.7 | unit | existing fire-queue suite | green under testing allocator |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Leak class closed under injection (§1) | `make test-unit-all` | exit 0 incl. the eight §1 tests | P0 | ✅ exit 0 — `All unit lanes passed` (first run tripped one flaky UI vitest, `fleets-install-entry-gate.test.ts`, 22/22 green in isolation and on rerun; no TypeScript in this diff) |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 non-`*_test.zig` paths missing from the table | P0 | ✅ every diff path has a table row (three rows added during EXECUTE: `main.zig`, `cmd/serve_boot.zig`, `credentials/testing.zig`) |
| R3 | Secret zeroization present at teardown (§3) | `grep -rn "secure_memory" src/agentsfleetd/credentials/ \| wc -l` | ≥ 2 matches | P1 | ✅ 11 matches |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — `All package coverage gates passed` / `All unit lanes passed` |
| S2 | Lint clean (incl. new check) | `make lint-all` | exit 0 | P0 | ✅ exit 0 — `4 passed, 0 failed, 2 skipped` / `All lint checks passed` |
| S3 | Integration passes (db/events/cron touched) | `make test-integration` | exit 0 | P0 | ✅ exit 0 — `✓ [agentsfleetd] Full integration suite passed` (also the 6.3 suite-green proof) |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | ✅ exit 0 — `✓ memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)` |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ exit 0 both targets, re-verified after every Section |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (4032 commits scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| grep -vE '(^\|/)(tests?)/\|_test\.zig$\|tests\.zig$\|.*test.*\.zig$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output (mirrors the FLL gate's test-pattern exemption) | P0 | ✅ no output |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ all eight greps 0 matches |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted (the new fixtures module is a CREATE).

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| migration-lock `acquire` wrapper | `grep -rn "lock\.acquire(" src/ \| head` | 0 matches |
| `DATABASE_URL_CALLBACK` | `grep -rnw "DATABASE_URL_CALLBACK" src/ \| head` | 0 matches |
| `DbRole.callback` variant | `grep -rn "\.callback" src/agentsfleetd/db/ \| head` | 0 matches |
| jwks `getInt` re-export | `grep -rn "jwks\.getInt" src/ \| head` | 0 matches |
| `BEARER_PREFIX` (errors) | `grep -rnw "BEARER_PREFIX" src/agentsfleetd/errors/ \| head` | 0 matches |
| duplicate namespace constant | `grep -rnw "NAMESPACE_DEV\|NAMESPACE_PROD" src/ \| head` | 0 matches (both collapsed into the single `CLAIM_NAMESPACE` owner — the pair were identical, so neither env-suffixed name was honest) |
| auth `constantTimeEql` copy | `grep -rn "constantTimeEql" src/agentsfleetd/auth/api_key.zig \| head` | 0 matches |
| `TEST_TENANT_ID` alias (uc1) | `grep -rn "^pub const TEST_TENANT_ID" src/agentsfleetd/db/test_fixtures_uc1.zig \| head` | 0 matches (the surviving `TENANT_ID` legitimately re-exports `base.TEST_TENANT_ID`) |

De-pub-only items (per-signal OTel fns, `ArgvIter`, `SYNC_LEASE_MS`, `MAX_RESPONSE_BYTES`, `DEADLINE_MS`, `VAULT_KEY`, `sessionIdHash`, splitter `count`, pool `Row`) are proven by zlint unused-decls staying green with `pub` removed.

## Out of Scope

- Unifying the four deadline-armed outbound-HTTP exchanges into one shared helper — deferred; the third-copy extraction trigger is recorded in the audit and in Decomposition below.
- Audit of the remaining 13 `src/agentsfleetd` folders (`fleet`, `fleet_library`, `fleet_runtime`, `http`, `memory`, `observability`, `queue`, `secrets`, `session`, `state`, `types`, `util`, plus top-level files) — follow-up milestone.
- Any behavior change to drain discipline, migration locking, or the events hub design — audited healthy; only the named hygiene items are touched.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator watches daemon memory stay flat through a signup burst and a credential-vendor outage that previously grew heap on every webhook and failed mint.
2. **Preserved user behaviour** — every endpoint, CLI verb, schema, and wire shape is unchanged; mint events keep their shape (latency becomes truthful).
3. **Optimal-way check** — fix-in-kind at each audited site is the most direct path; the unconstrained-optimal (one shared outbound-exchange helper) is deferred deliberately (Decomposition).
4. **Rebuild-vs-iterate** — iterate: every fix mirrors an existing in-repo proven pattern; no redesign is warranted by the findings.
5. **What we build** — the fixes in §1–§7, one new lint check, one new test-fixtures module.
6. **What we do NOT build** — outbound-exchange unification (review-risk on four security paths in one PR); remaining-folder remediation (needs its own audit).
7. **Fit with existing features** — compounds with the events/cron proof culture the audit praised; must not destabilize the auth middleware chain — its adversarial suite is the guard.
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; operator-facing errors gained specificity (dotenv line numbers, config-shaped token errors).

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections ordered by severity (leak class → hardening → hygiene) so review reads highest-risk first; one workstream because every slice shares the same verification gates.
- **Alternatives considered:** unify the four outbound-HTTP exchange sites into one deadline-armed helper — rejected for now: it rewrites four security-sensitive paths in one PR and the audit rated the duplication a pattern echo, not a foldable duplicate.
- **Patch-vs-refactor verdict:** this is a **patch** milestone because every finding is a localized divergence from an already-proven in-repo pattern; the follow-up refactor (outbound-exchange unification) is named and deferred, not silently mud-patched.

## Discovery (consult log)

- **Consults** — Clerk worker bound (Dimension 2.3): the deadline-armed joinable worker (mirroring the credentials/cron exchanges) requires importing `call_deadline` + `http_pin` into `src/agentsfleetd/auth/`, which the `auth-only-tests` portability aggregate deliberately excludes. Chosen: bounded in-flight cap + bounded shutdown drain, all std+common — stragglers touch only self-owned memory. Flag for Indy: if widening the auth dependency surface is acceptable, the deadline-armed worker is the stronger long-term shape. · Flight waiter (Dimension 2.4): `std.Io.Condition` (0.16) has no timed wait; losers now poll on a short cadence with a deadline, and the no-longer-consumed `inflight_cond` was removed rather than kept as dead concurrent code.
- **Metrics review** —
- **Review consults (adversarial pass, for Indy)** — (1) Clerk worker slots (Dimension 2.3): the portability-wall design has no per-fetch deadline, so a blackholed Clerk endpoint can wedge all 8 slots and silently disable signup-metadata writes for the process lifetime (the pre-M152 code leaked threads but kept serving). Recommended follow-up: a std-only slot LEASE (per-slot claim timestamp, expired slots reclaimable) restores availability without breaching the wall — Indy picks fix-now vs follow-up. (2) The mint token-exchange (`serve_broker.postImpl`) remains the one outbound read without a byte cap (deadline-bounded only, allowlisted endpoints); capping it adds a fourth copy of the capped-read pattern, so it belongs to the already-deferred outbound-exchange unification rather than this diff.
- **Skill-chain outcomes** — `/write-unit-test`: diff ledger fully resolved; two gaps it exposed were closed in-tree (oversized-QStash-credential test, finalize-fallback observable-state test). `/write-integration-test`: three `make test-integration` passes green (one clean-state, two shared) incl. the review-hardened suites. gstack `/review`: four-agent army (adversarial + testing + maintainability + security) + Codex cross-model (timed out at its 5-minute cap, non-blocking) — 24 findings; 12 fixed in-tree (double boot log, assertion-free tests, JWKS success-path coverage, string expires_in compat, token_path shape guard, lint comment-strip hole + pinning fixture, emit-on-OOM observability, noteDrop lock scope, oversized-token constant coupling, otlp deinit doc, spec table drift), 2 recorded as Discovery consults for Indy (Clerk slot lease, mint-exchange byte cap), the rest documented in PR Session Notes with reasons.
- **Deferrals** — none acked. FLAG for Indy (Dimension 7.2 residual): 🎯 `cmd/serve.zig run()` still ~180 lines after the `serve_boot.zig` prologue extraction · 🔧 a full ≤50 split requires moving the boot resource graph (pools, hub, streams, registry, server) into an owner struct whose deinit re-encodes today's defer order — multi-file, touches shutdown choreography · 🏆 fn-cap compliance and a testable boot seam · ⚠️ if not fixed: one function stays over the cap; the ordering-sensitive teardown remains proven by the existing lifecycle integration tests either way.
