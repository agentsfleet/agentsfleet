<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M205_001: No constant is declared in two languages, and no test reads another language's source to decide

**Prototype:** v2.0.0
**Milestone:** M205
**Workstream:** 001
**Date:** Sep 21, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: one of the three ties computes a live Grafana alert threshold by running `sed` over Rust source, and another governs lease renewal on the shipping runner.
**Categories:** API, CLI, DOCS, OBS, UI
**Batch:** B1 — single stream; §1 is independent, §2 precedes nothing, §3 depends on §2 having settled which crate owns the lease clock.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none — M203_001 is merged and its Discovery is this spec's source record.
**Provenance:** LLM-drafted (claude-opus-5[1m], Sep 21, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §The control protocol — `/v1/runners`

---

## Overview

**Goal (testable):** Every number both runtimes need is declared once and travels to the other over the wire or over a function call; no test, script, or gate reads a second language's source text to decide anything, and the three files that did are deleted.

**Problem:** Three constants families are written down twice, in two languages, and kept equal by a reader rather than by a mechanism. `afd_billing/tests/cross_runtime_rates.rs` opens two TypeScript files and parses `export const` lines. `afd_core/tests/cross_runtime_timing.rs` opens `src/lib/common/constants.zig` and parses `pub const` lines. `playbooks/operations/observability/lib.sh:144` runs four `sed` expressions over two Rust files to compute the runner-offline alert threshold and the admission replay floor that Grafana actually alerts on. A parser that stops matching — a reformat, a renamed file, a changed type annotation — silently stops guarding, and every one of these still exits zero when it sees nothing. The observability script has the sharpest edge: it derives a production threshold from a text scan, so a Rust refactor that touches whitespace can change what pages an operator.

**Solution summary:** Delete the duplication instead of policing it, and in the one case where both runtimes genuinely need the same number, put it on the wire. The three rate constants in the two TypeScript mirrors have zero importers anywhere in `cli/src` or `ui/packages` — they exist only to be read by the pin — so they go, and `NANOS_PER_USD` (the wire unit, not a rate) collapses to one declaration per runtime. The Zig runner is already handed its lease deadline by the server, so its `LEASE_TTL_MS` copy goes; its renewal window and tick become runner-local policy guarded by a Zig `@compileError` inequality, which is the shape `src/lib/common/constants.zig:99` already uses; the heartbeat cadence, the one value whose safety depends on a daemon-owned threshold, joins `assigned_policy` on the heartbeat reply. The observability playbook stops parsing source and asks the daemon: `agentsfleetd thresholds --json` prints the numbers the binary itself enforces, exactly as the existing `agentsfleetd openapi` subcommand prints the document its own routes generate. All three reading files are deleted; nothing replaces them, because after the change there is no second copy to compare.

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(api,cli,app,obs): every shared constant is declared once and served, not mirrored
- **Intent (one sentence):** An operator's alert threshold, a runner's renewal clock, and a dashboard's price all come from the one place that enforces them, so none of the three can quietly disagree with the daemon.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/done/M203_001_P1_API_CLI_CONTINUATION_LINEAGE_AND_LEDGER_KEY.md` §Discovery — the measurement this spec acts on, and the owner decisions behind it. Do not edit that file.
2. `rustd/crates/agentsfleetd/src/cli.rs` — the `Openapi` subcommand is the pattern §3 mirrors: a verb that prints what the binary's own code generates, opens no datastore, and needs no runtime.
3. `src/lib/common/constants.zig` — the in-repo shape for §2's guards: a `@compileError` on an inequality between two runner-local constants. `src/lib/call_deadline/call_deadline.zig` is the same family with `std.debug.assert`.
4. `src/lib/contract/protocol.zig` and `rustd/crates/afd_wire/src/runner.rs` — the two declarations of the heartbeat reply that §2 extends. Reading both is how the agent sees that `assigned_policy` and `selftest_requested` already ride the beat for this exact reason.
5. `docs/RUST_ERROR_STANDARD.md` — read before any fallible signature under `rustd/`; §3 adds one.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_billing/tests/cross_runtime_rates.rs` | DELETE | The TypeScript parser. Nothing replaces it; after §1 there is no second copy of a rate. |
| `rustd/crates/afd_core/tests/cross_runtime_timing.rs` | DELETE | The Zig parser. Same reason. |
| `cli/src/constants/billing.ts` | EDIT | Drops the three unimported rate constants and the mirror preamble; keeps `NANOS_PER_USD` and `formatDollars`. |
| `ui/packages/app/lib/types.ts` | EDIT | Same three constants dropped; `NANOS_PER_USD` stays as the package's one declaration. |
| `cli/src/commands/models.ts` | EDIT | Stops redeclaring `NANOS_PER_USD` locally; imports the package one. |
| `ui/packages/app/components/layout/BalanceLink.tsx` | EDIT | Stops deriving `NANOS_PER_USD` from `NANOS_PER_CENT`; imports it. |
| `rustd/crates/afd_core/src/money.rs` | CREATE | The wire unit's one Rust home, beside `timing`. Every crate that reads a nanos column already depends on `afd_core`, so this adds no dependency edge. |
| `rustd/crates/afd_core/src/lib.rs` | EDIT | Registers the new module. |
| `rustd/crates/afd_billing/src/nanos.rs` | EDIT | Takes `NANOS_PER_USD` from `afd_core`; keeps the rates; the doc comments stop citing the deleted `tenant_billing.zig`. |
| `rustd/crates/afd_tenant/src/signup.rs` | EDIT | Drops its own `NANOS_PER_USD`; keeps `STARTER_CREDIT_NANOS`, which is this crate's to own. |
| `src/lib/common/constants.zig` | EDIT | `LEASE_TTL_MS` and `RUNNER_OFFLINE_AFTER_MS` deleted; `HEARTBEAT_INTERVAL_MS` demoted to a first-beat default; the window/tick inequality becomes a `@compileError`. |
| `src/lib/contract/protocol.zig` | EDIT | `HeartbeatResponse` gains the cadence field. |
| `rustd/crates/afd_wire/src/runner.rs` | EDIT | The same field on the Rust side of the reply. |
| `rustd/crates/afd_api_runner/src/handler/runner/heartbeat.rs` | EDIT | Serves the cadence from `afd_core::timing`. |
| `src/runner/daemon/loop.zig` | EDIT | Applies the served cadence to `heartbeat_interval_ms`, which is already a `pub var`. |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | Adds the compile-time inequality between the served cadence and the offline threshold; doc comments stop citing `constants.zig`. |
| `rustd/crates/agentsfleetd/src/thresholds.rs` | CREATE | Assembles the operational thresholds and renders them as JavaScript Object Notation (JSON). |
| `rustd/crates/agentsfleetd/src/cli.rs` | EDIT | The `Thresholds` subcommand. |
| `rustd/crates/agentsfleetd/src/lib.rs` | EDIT | Registers the new module. |
| `rustd/crates/afd_runner/src/sweep/replay.rs` | EDIT | `MIN_AGE` and `INTERVAL` become `pub` so the binary that prints them can see them. |
| `playbooks/operations/observability/lib.sh` | EDIT | `obs_runner_offline_seconds` and `obs_admission_replay_floor_seconds` invoke the subcommand instead of running `sed` over Rust. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (the deleted constants and their doc preambles leave nothing behind), **NLR** (the `nanos.rs` and `timing.rs` doc comments citing deleted `.zig` files are touched here and so are fixed here), **NLG** (the heartbeat field is required, not an optional compat shim for an older daemon — the two ship together), **UFS** (the served cadence and the printed thresholds are named constants, never literals), **ORP** (every deleted symbol gets a repository grep in the Dead Code Sweep), **TCF** (each Dimension's test is made red first; a test that passes against the unmodified tree does not count), **TST-NAM** (no milestone identifier in a test name), **ERR-RS** (§3's fallible path uses the crate's generated error type, never a hand-written one).
- **`dispatch/write_rust.md`** — fires on every `.rs` edit here; §3 adds a fallible signature and the error standard governs it.
- **`dispatch/write_zig.md`** — fires on `src/lib/common/constants.zig`, `src/lib/contract/protocol.zig` and `src/runner/daemon/loop.zig`; PUB GATE decides the visibility of what remains after the deletions.
- **`dispatch/write_ts_adhere_bun.md`** — fires on the four TypeScript and TypeScript-with-JavaScript-XML edits in §1.
- **`dispatch/write_shell.md`** — fires on `playbooks/operations/observability/lib.sh`; quoted expansions and a loud failure when the subcommand is unavailable.
- **`dispatch/write_http.md`** and `docs/REST_API_DESIGN_GUIDELINES.md` — the heartbeat reply is a public response shape.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — three `.zig` files | The deletions shrink `constants.zig`; the new guards are comptime, so a violation is a build failure, not a runtime one. Cross-compile both linux targets before the Pull Request. |
| PUB GATE | yes — `constants.zig` loses two `pub const` and `replay.rs` gains two | Each survivor's visibility is justified by a named reader; `MIN_AGE` and `INTERVAL` become `pub` only because the threshold binary reads them. |
| UI GATE / DESIGN TOKEN GATE | yes — `BalanceLink.tsx` | The edit is an import swap; no markup and no colour or spacing value changes. |
| UFS GATE | yes | The served cadence, the JSON keys, and the subcommand name are named constants. |
| LENGTH GATE (≤350 file / ≤50 function) | yes | `thresholds.rs` is a new file and stays small; `cli.rs` and `lib.sh` are checked against the cap before commit and split if either approaches it. |
| MILESTONE-ID GATE | yes | No `M205`, `§x.y` or `T{N}` token in any source file or test name. |
| LOGGING GATE | yes — `loop.zig` | The applied cadence rides the existing heartbeat debug event; no new event name. |
| ERROR REGISTRY | yes — §3 | The subcommand's failure uses the crate's declared scheme; no new `UZ-XXX-NNN` unless the registry requires one. |
| SCHEMA GUARD | no — no `schema/` file is touched | N/A. |
| LIFECYCLE GATE | no — no `init`/`deinit` pair is added or removed | N/A. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/agentsfleetd/src/cli.rs:97` (`Openapi`) — §3 mirrors it exactly: a verb whose output is a pure function of the binary's own code, opening no datastore and constructing no runtime. The divergence is that `Openapi` is compiled only under a feature flag because it is a build's verb; `Thresholds` is an operator's verb and ships in the release build.
- **Reference:** `src/lib/contract/protocol.zig:282` (`retry_after_ms` on the lease reply) — §2's cadence field is the same move: the server already tells the runner how long to wait before asking again, and `loop.zig:328` already prefers the served value over the local default.
- **Reference:** `src/lib/common/constants.zig:99` — the `@compileError` guard §2 adds for the renewal window and tick is this line's shape, applied to the two constants that remain.

## Sections (implementation slices)

### §1 — The rates are declared once, in Rust

The three rate constants in the two TypeScript mirrors have no importer: a repository-wide search of `cli/src` and `ui/packages` finds `STARTER_CREDIT_NANOS`, `EVENT_NANOS` and `RUN_NANOS_PER_SEC` only in the mirror files themselves and in the Rust parser that reads them. They are deleted. `NANOS_PER_USD` is a different thing — it is the denominator of the wire format, needed to interpret any `balance_nanos` at all, and it has roughly thirty real callers — so it stays client-side and collapses to one declaration per package; the two ad-hoc respellings go with the rest. **Implementation default:** `afd_core::money::NANOS_PER_USD` is the Rust home, because every crate that reads a nanos column already depends on `afd_core` and moving it there adds no dependency edge, where making `afd_tenant` depend on `afd_billing` would.

- **Dimension 1.1** — The dashboard renders a tenant balance from a served `balance_nanos` with no rate constant in the path → Test `test_balance_card_renders_served_nanos`
- **Dimension 1.2** — `agentsfleet billing` prints a balance and per-event charges from the served response alone → Test `test_billing_output_uses_served_charges_only`
- **Dimension 1.3** — A new tenant opens with the starter grant, with `STARTER_CREDIT_NANOS` declared once → Test `test_signup_grants_starter_credit_from_single_declaration`
- **Dimension 1.4** — Both client packages resolve `NANOS_PER_USD` from their one declaration → Test `test_dollar_scale_has_one_declaration_per_package`

### §2 — The runner is told its clock; what is its own, its own compiler guards

`src/runner/daemon/renew_driver.zig:120` renews against `self.deadline_ms`, which the server set — so the runner's `LEASE_TTL_MS` is read by nothing but the definition of `RUNNER_OFFLINE_AFTER_MS`, and deriving an offline threshold is the daemon's job, not the runner's. Both go. `RENEWAL_WINDOW_MS` and `RENEWAL_TICK_MS` stay as runner-local policy, which only ever had to satisfy `tick < window`, and a `@compileError` now says so. The heartbeat cadence is the one number whose safety genuinely depends on a threshold the daemon owns, so the daemon sends it. **Implementation default:** the field is required on the reply rather than optional, because RULE NLG forbids a compat shim before `0.30.0` and the daemon and runner ship together; a reply missing it fails the same closed path `assigned_policy: null` already takes.

- **Dimension 2.1** — A runner renews inside the window against a server-sent deadline, holding no lease-duration constant of its own → Test `test_renews_against_served_deadline_without_local_ttl`
- **Dimension 2.2** — A runner whose reply carries a cadence beats at that cadence, not at its first-beat default → Test `test_heartbeat_adopts_served_cadence`
- **Dimension 2.3** — A reply with no cadence field is refused and the runner reads degraded rather than guessing → Test `test_heartbeat_without_cadence_fails_closed`
- **Dimension 2.4** — The daemon serves a cadence strictly below its own offline threshold → Test `test_served_cadence_stays_below_offline_threshold`
- **Dimension 2.5** — A renewal tick not strictly below the renewal window fails the build → Test `test_tick_window_inequality_is_comptime`

### §3 — The alert threshold comes from the binary that enforces it

`obs_runner_offline_seconds` and `obs_admission_replay_floor_seconds` compute what Grafana alerts on by running four `sed` expressions over `afd_core/src/timing.rs` and `afd_runner/src/sweep/replay.rs`, including one that matches a specific `Duration::from_secs` spelling. A reformat changes an operator's pager. `agentsfleetd thresholds --json` prints the same numbers as a function of the compiled constants, and the playbook reads that. **Implementation default:** a subcommand on the existing daemon binary rather than a generated file checked into the repository, because a checked-in artifact needs a freshness gate and the freshness gate is another scan.

- **Dimension 3.1** — The subcommand prints both thresholds as JSON, opening no datastore → Test `test_thresholds_renders_without_a_datastore`
- **Dimension 3.2** — The printed runner-offline value equals what the liveness sweep enforces → Test `test_printed_offline_threshold_matches_sweep`
- **Dimension 3.3** — The printed replay floor equals the sweep's minimum age plus its interval → Test `test_printed_replay_floor_matches_sweep`
- **Dimension 3.4** — The playbook derives both thresholds from the subcommand and exits non-zero when it is unavailable → Test `test_playbook_fails_loud_without_the_threshold_verb`

## Interfaces

```
POST /v1/runners/me/heartbeats  →  HeartbeatResponse
  status                 : HeartbeatStatus
  assigned_policy        : AssignedPolicy | null
  degraded               : bool
  degraded_reason        : string | null
  selftest_requested     : bool
  heartbeat_interval_ms  : u32          // ADDED — required; the cadence the
                                        // daemon expects, always strictly below
                                        // its own runner-offline threshold

agentsfleetd thresholds --json  →  stdout, exit 0
  {"runner_offline_seconds": 90, "admission_replay_floor_seconds": <u64>}
  // Values are illustrative of shape, not pinned here: the constants are.

afd_core::money::NANOS_PER_USD : i64    // the wire unit, one Rust declaration
afd_billing::{RUN_NANOS_PER_SEC, RECEIVE_NANOS}   // the rates, unchanged
afd_tenant::signup::STARTER_CREDIT_NANOS          // the only credit inflow
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Cadence absent from reply | A daemon that does not serve the field | The runner refuses the reply, reads degraded with a reason, and does not lease — the path `assigned_policy: null` already takes. Operator sees a degraded row, not a silent wrong cadence. |
| Cadence not below the offline threshold | A future edit to `afd_core::timing` | The daemon does not compile. The inequality is a `const` assertion in the crate that declares both numbers. |
| Renewal tick not below the renewal window | A future edit to `constants.zig` | The runner does not compile. `@compileError` names which pair is wrong. |
| Threshold verb unavailable | The playbook runs where the binary is not built | `obs_runner_offline_seconds` writes an error naming the missing verb to standard error and exits 1, the same loud failure the unparseable-`sed` path takes today. No threshold is guessed. |
| Threshold output unparseable | A malformed or truncated JSON document | The playbook exits 1 before any Grafana write; a partially applied alert rule is never produced. |
| Balance rendered with no rate served | A billing response missing a field a client displays | The client renders the field's absence explicitly rather than substituting a constant; no client-side arithmetic reconstructs a charge. |

## Invariants

1. **No constant is declared in two languages.** Enforced by construction: after §1 and §2 the second declarations do not exist, so there is nothing to hold equal. This replaces a guard with an absence, and the absence is what the Dead Code Sweep's greps confirm at merge.
2. **No test, script, or gate parses another language's source to decide.** Enforced by construction: the three readers are deleted and none is replaced. A reintroduction is visible in review as a new file-reading test, and this spec deliberately adds no scanner to detect one — see Discovery.
3. **The renewal tick is strictly below the renewal window.** Enforced by `@compileError` in `src/lib/common/constants.zig`, in the file that declares both.
4. **The served heartbeat cadence is strictly below the daemon's runner-offline threshold.** Enforced by a compile-time assertion in `afd_core::timing`, in the crate that declares both.
5. **The runner holds no number the daemon separately enforces.** Enforced by the wire: the deadline and the cadence arrive on replies the runner already reads.
6. **The alert threshold equals the enforced threshold.** Enforced by shared code: the subcommand prints the same constants the liveness sweep consumes, and Dimensions 3.2 and 3.3 assert the equality in-process.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product signal changes | not applicable | — | — | — | — |
| `heartbeat` (existing debug event, `src/runner/daemon/loop.zig`) | ops | Each beat, unchanged in cadence semantics | Adds the applied interval in milliseconds; no new event name | No credential, token or host secret | `test_heartbeat_adopts_served_cadence` |

No analytics or funnel playbook update is required: no user-visible product event is added, renamed or removed. The operator-facing change is that two Grafana thresholds are now derived from the daemon rather than from source text, which the observability playbook's own output records.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `test_balance_card_renders_served_nanos` | A billing response of 5_000_000_000 nanos renders `$5.00` in the rendered dashboard with no rate constant read. |
| 1.2 | e2e | `test_billing_output_uses_served_charges_only` | A subprocess `agentsfleet billing` against a stubbed response prints the served per-event totals verbatim; no value is recomputed client-side. |
| 1.3 | integration | `test_signup_grants_starter_credit_from_single_declaration` | A fresh signup opens with a balance equal to `afd_tenant::signup::STARTER_CREDIT_NANOS`, against a real datastore. |
| 1.4 | unit | `test_dollar_scale_has_one_declaration_per_package` | Both client packages import `NANOS_PER_USD` from their package module; formatting 1 nano and 9_999_999_999_999 nanos is exact at the boundary. |
| 1.4 | unit | `test_formatter_rejects_a_negative_and_a_null_balance` | Negative and null inputs render explicitly rather than as `$0.00`. |
| 2.1 | unit | `test_renews_against_served_deadline_without_local_ttl` | A driver holding only `deadline_ms` renews inside the window and keeps outside it; no lease-duration constant is referenced. |
| 2.1 | unit | `test_renewal_survives_an_extreme_served_deadline` | A deadline at the integer maximum does not overflow the tick decision; the driver keeps rather than panicking. |
| 2.2 | integration | `test_heartbeat_adopts_served_cadence` | A reply carrying a cadence different from the first-beat default changes the loop's interval within one beat. |
| 2.3 | integration | `test_heartbeat_without_cadence_fails_closed` | A reply omitting the field yields a degraded row with a reason, and the runner does not lease. |
| 2.4 | unit | `test_served_cadence_stays_below_offline_threshold` | The handler's served value is strictly less than the offline threshold for the declared constants. |
| 2.5 | manual | `test_tick_window_inequality_is_comptime` | A local edit making the tick equal the window fails `zig build` with the named message; evidence is the build output pasted into Session Notes. Procedure and required person recorded there. |
| 3.1 | unit | `test_thresholds_renders_without_a_datastore` | The subcommand's render function returns both keys with no datastore handle constructed. |
| 3.2 | unit | `test_printed_offline_threshold_matches_sweep` | The printed seconds equal the liveness sweep's threshold converted from milliseconds. |
| 3.3 | unit | `test_printed_replay_floor_matches_sweep` | The printed seconds equal the sweep's minimum age plus its interval. |
| 3.4 | integration | `test_playbook_fails_loud_without_the_threshold_verb` | With the verb absent from `PATH`, the playbook function exits 1 and writes a message naming it; no Grafana request is issued. |
| 2.1 | integration | `test_lease_renew_and_settle_unchanged` | The existing renew and settle paths produce the same ledger rows and balances as before the change. |
| 2.2 | e2e | `test_runner_registers_leases_and_completes` | A runner registers, beats, leases, executes and settles end to end on the shipping binary. |
| 2.2 | integration | `test_repeated_heartbeats_do_not_drift_the_cadence` | Ten consecutive beats carrying the same cadence leave the applied interval unchanged and write one liveness update each. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No Rust test reads a TypeScript or Zig source file (§1, §2) | `test ! -f rustd/crates/afd_billing/tests/cross_runtime_rates.rs && test ! -f rustd/crates/afd_core/tests/cross_runtime_timing.rs` | exit 0 | P0 | |
| R2 | The observability playbook no longer parses Rust source (§3) | `grep -c 'timing.rs\|replay.rs' playbooks/operations/observability/lib.sh` | `0` | P0 | |
| R3 | The threshold verb prints both keys (§3) | `cargo run -q -p agentsfleetd -- thresholds --json \| jq -e 'has("runner_offline_seconds") and has("admission_replay_floor_seconds")'` | exit 0 | P0 | |
| R4 | The runner holds no lease-duration constant (§2) | `grep -c 'LEASE_TTL_MS\|RUNNER_OFFLINE_AFTER_MS' src/lib/common/constants.zig` | `0` | P0 | |
| R5 | The unimported rate constants are gone from both clients (§1) | `grep -rn 'STARTER_CREDIT_NANOS\|EVENT_NANOS\|RUN_NANOS_PER_SEC' cli/src ui/packages \| wc -l` | `0` | P0 | |
| R6 | The runner cross-compiles for both linux targets (§2) | `make dry` | exit 0 | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{N}_{NNN} R{n}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_billing/tests/cross_runtime_rates.rs` | `test ! -f rustd/crates/afd_billing/tests/cross_runtime_rates.rs` |
| `rustd/crates/afd_core/tests/cross_runtime_timing.rs` | `test ! -f rustd/crates/afd_core/tests/cross_runtime_timing.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `STARTER_CREDIT_NANOS` (TypeScript) | `grep -rn -w "STARTER_CREDIT_NANOS" cli/src ui/packages \| head` | 0 matches |
| `EVENT_NANOS` (TypeScript) | `grep -rn -w "EVENT_NANOS" cli/src ui/packages \| head` | 0 matches |
| `RUN_NANOS_PER_SEC` (TypeScript) | `grep -rn -w "RUN_NANOS_PER_SEC" cli/src ui/packages \| head` | 0 matches |
| `LEASE_TTL_MS` (Zig) | `grep -rn -w "LEASE_TTL_MS" src/ \| head` | 0 matches |
| `RUNNER_OFFLINE_AFTER_MS` (Zig) | `grep -rn -w "RUNNER_OFFLINE_AFTER_MS" src/ \| head` | 0 matches |
| `MIRRORED_NAMES` | `grep -rn -w "MIRRORED_NAMES" rustd/ \| head` | 0 matches |
| `ZIG_MIRROR` | `grep -rn -w "ZIG_MIRROR" rustd/ \| head` | 0 matches |
| `afd_tenant::signup::NANOS_PER_USD` | `grep -rn -w "NANOS_PER_USD" rustd/crates/afd_tenant/ \| head` | 0 matches |

## Out of Scope

- **The dead `.zig` citations in Rust doc comments** — 860 citation lines across 358 Rust files name 315 distinct `.zig` paths, 296 of them deleted with the Zig daemon, and 97 assert an active constraint in words against a file that does not exist. The sweep is mechanical but per-line and would bury this spec's change at 358 files. It is its own milestone, per the owner decision quoted in Discovery. This spec fixes only the citations in the files it already edits, under RULE NLR.
- **The Rust tests that parse Rust source** — `names.rs`, `openapi_artifact.rs`, `workspace.rs` and `scope_catalogue.rs`, kept at M203 close with recorded reasons. They are same-language, so no cross-runtime tie is at stake, but the owner's direction in this milestone's authoring covers the shape; they belong with the sweep milestone above. Named here so they are not mistaken for settled.
- **The published pricing strings** — `~/Projects/docs/snippets/rates.mdx` carries `"$5"` and `"free"` by hand, cites three paths that no longer resolve, and is guarded by nothing. Retiring that copy means a public rates endpoint, which this spec deliberately does not add. See Discovery.
- **Generating TypeScript from Rust** — rejected, not deferred: it would generate constants nothing imports.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator's pager fires at the threshold the daemon actually enforces, because the alert rule was built from the daemon's own output rather than from a `sed` expression over a source file somebody reformatted.
2. **Preserved user behaviour** — Every billing display, every lease renewal, every heartbeat and every existing alert keeps working unchanged. A runner in the field keeps leasing and settling; a dashboard keeps rendering the same balance.
3. **Optimal-way check** — This is the direct route for §2 and §3. For §1 the unconstrained-optimal shape is a public rates endpoint that the docs site and the dashboard both read, which would retire the last hand-maintained copy of the price as well. That is a new public surface for a display nothing currently renders, so the gap is accepted and named in Out of Scope.
4. **Rebuild-vs-iterate** — Iterate. Nothing here trades away run-to-run determinism; §2 and §3 add it, by replacing text parsing with a compiler and a served value.
5. **What we build** — Three deletions, one wire field, one daemon subcommand, and two compile-time inequalities.
6. **What we do NOT build** — No Rust-to-Zig code generation, no shared constants format, no public rates endpoint, no gate that fails on a Rust comment citing an unresolvable `.zig` path. The last one is refused on its own terms: it would be another source scan, which is the shape this milestone exists to retire.
7. **Fit with existing features** — Compounds with the heartbeat's existing role as the policy channel: `assigned_policy` and `selftest_requested` already ride the beat so an operator change reaches a host with no host visit, and the cadence now does too. The one thing it must not destabilize is lease renewal on the shipping runner, which the §2 regression rows cover.
8. **Surface order** — Neither. This is an internal correctness change with one operator-facing surface, the threshold verb, which ships with the daemon that owns it.
9. **Dashboard restraint** — Nothing is added to the dashboard. The three constants removed from `ui/packages/app` were never rendered.
10. **Confused-user next step** — An operator whose playbook fails gets a message naming the missing subcommand and the binary that provides it, which is a command they can run.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Three Sections split by the mechanism that replaces each tie, not by language: §1 replaces a tie with deletion, §2 replaces one with the wire plus a compiler, §3 replaces one with a served value. Each is independently shippable and independently provable, and the split keeps the Zig work in one Section so the eventual Rust port of the runner touches one slice rather than three.
- **Alternatives considered:** (a) Generate the TypeScript constants from the Rust at build time — rejected: it makes the duplication unfalsifiable rather than absent, and generates constants that no client imports. (b) Keep `cross_runtime_timing.rs` until the runner is ported to Rust, on the grounds that the Zig side is temporary — rejected: the pin enforces an agreement that was never needed, and carrying it means carrying a text parser over a file the port deletes. (c) Have the observability playbook read a generated JSON artifact checked into the repository — rejected: a checked-in artifact needs a freshness gate, and a freshness gate is another scan.
- **Patch-vs-refactor verdict:** this is a **refactor** because the problem is a shape, not a defect. Each individual constant is correct today; what is wrong is that three of them are held correct by a reader that fails open. Patching would mean fixing the parsers, which is the thing being retired.

## Discovery (consult log)

- **Consults** — Source: `docs/v2/done/M203_001_P1_API_CLI_CONTINUATION_LINEAGE_AND_LEDGER_KEY.md` §Discovery, which records the measurement this spec acts on: two cross-runtime pins holding nine constants, 860 citation lines across 358 Rust files naming 315 distinct `.zig` paths of which 296 are deleted, and the owner's acceptance of the pins at that time — > Indy (Sep 21, 2026): "I am okay to have diagreement via the unit test failing" — context: the ask named the drift each pin catches and offered removing the duplication instead. That acceptance is superseded here by direction given at this spec's authoring: > Indy (Sep 21, 2026): "I dont want to have a tie or read of rs/ts, rs/zig and so on. and also the rs -> files doing a scan on some files in rus to take a decision" — context: the ask had put three options for the rates side; the answer widened the mandate from the rates to every cross-language tie and to source-scanning generally. **Required human decision, GRANTED:** the rates mechanism, asked as a three-way choice and answered "Delete rates, unify the unit" — context: the ask recorded that the three rate constants have zero importers in `cli/src` or `ui/packages`, that `NANOS_PER_USD` is the wire unit rather than a rate and has roughly thirty callers, and that the alternatives were a public rates endpoint or build-time generation. Architecture consult: `docs/architecture/runner_fleet.md` §The control protocol — `/v1/runners` describes the heartbeat as the channel that carries assignment to a host within one beat; §2 extends that channel rather than adding one, so no divergence to reconcile. `docs/architecture/observability.md` §Service Level Objectives is the reader of §3's thresholds and is revised in the same Pull Request.
- **Third tie found while authoring, not in the original brief.** `playbooks/operations/observability/lib.sh:144-173` derives two live Grafana thresholds by running four `sed` expressions over `rustd/crates/afd_core/src/timing.rs` and `rustd/crates/afd_runner/src/sweep/replay.rs`, one of which matches a literal `Duration::from_secs` spelling. It is in scope under the direction quoted above and is §3. Its own comment already predicted the failure: it notes the playbook "would have derived its alert threshold from a deleted file" the day `constants.zig` goes — which §2 does.
- **Stated rather than implied: what ends up unguarded.** Invariants 1 and 2 are enforced by absence, not by a check. Nothing in the repository will fail if a future change reintroduces a constant in two languages or adds a test that parses another language's source; the detector is review. This is deliberate and is the direction quoted above — a gate that failed on a Rust comment citing an unresolvable `.zig` path was considered and refused in Product Clarity item 6, because it is another source scan. Likewise `~/Projects/docs/snippets/rates.mdx` remains a hand-maintained copy of the price, guarded by nothing, and citing three paths that do not resolve. No test in this spec covers it.
- **Public surface, cross-repository.** The heartbeat reply gains a required field and the daemon gains a subcommand, so a matching branch in `~/Projects/docs` is required at CHORE(close) per the repository rules. It is authored on its own branch off `main` there, never through this worktree.
- **Metrics review** — no analytics or funnel playbook update required: no product event is added, renamed or removed. The operator-facing change is the derivation of two existing Grafana thresholds.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` once per Section over that Section's diff and again at the boundary; `/orly-write-integration-test` at the boundary, which applies here because §2 and §3 cross a module boundary with real input and output; `/review`; `orly-babysit-prs` after push.
- **Deferrals** — none. The two Out of Scope items that carry work are named as their own milestone with the owner quote recorded in the M203 Discovery cited above; neither is a deferral of this spec's scope.
