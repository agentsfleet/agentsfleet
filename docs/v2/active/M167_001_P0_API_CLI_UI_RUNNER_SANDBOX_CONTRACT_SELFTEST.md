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

# M167_001: An operator proves a runner can actually run work, from the runner's own page

**Prototype:** v2.0.0
**Milestone:** M167
**Workstream:** 001
**Date:** Aug 16, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — a runner whose sandbox cannot resolve a hostname reports itself healthy and fails every lease; today that gap is only visible by reading the daemon journal over Secure Shell (SSH)
**Categories:** API, CLI, UI
**Batch:** B1 — §3 declares the contract §2 probes and §1 renders; §4 extends the contract with an operator-editable additive layer
**Branch:** feat/m167-runner-sandbox-selftest
**Test Baseline:** unit=3924 integration=649
**Depends on:** none — the fix that motivated it (`/run/systemd/resolve` added to the sandbox bind set) landed separately in `ba7c990a7` and is not a prerequisite
**Provenance:** LLM-drafted (Claude Sonnet 5, Aug 16, 2026), from a live production-blocking incident reproduced on `zombie-dev-worker-ant` and confirmed against `main`
**Canonical architecture:** `docs/architecture/runner_fleet.md` · `docs/architecture/capabilities.md`

---

## Overview

**Goal (testable):** An operator triggers a connectivity self-test from a runner's page and, without leaving the dashboard, sees a per-check pass/fail verdict produced by a probe that ran inside a real sandbox under that runner's assigned policy — so a runner that cannot resolve a hostname reads as broken before a tenant's work does.

**Problem:** A sandboxed lease runs inside an unshared mount namespace. On a systemd-managed host `/etc/resolv.conf` is a symlink into `/run/systemd/resolve`, and the sandbox bound the symlink without its target — so the link dangled, every outbound name lookup failed, and every chat event on the affected runner terminated `runner_crash` / `HostResolutionFailed` after a five-second stall. The assigned network policy was irrelevant: `allow_all` shares the network namespace, not the mount namespace. Nothing surfaced it. The runner's own `doctor` answered `ok: true` throughout, because every check it runs — control-plane reachability included — runs on the **host**, outside the sandbox the work actually executes in. The dashboard showed `ACTIVE · ONLINE`. The runner detail page rendered a wall of `FAILED · The runner crashed` leases with no way to ask "can this host actually reach anything?", and the only diagnosis path was an operator with tailnet access reading `journalctl`. A host that cannot run work must say so itself.

**Solution summary:** Give the runner a self-test it executes **through the same argv builder a real lease uses**, inside a real sandbox, under the assigned policy — and surface it as one operator control on the runner's page. The dashboard records a request; the daemon picks it up on its next heartbeat, runs the probe, and reports a per-check result back over the same channel that already carries the capability report. Alongside it, the set of host paths the sandbox binds — and the mode each is bound at — stops being an unnamed array and becomes a declared contract with a regression test per required path, so a missing bind fails a test rather than a tenant's run. Whether an operator may edit that path set from the dashboard is a decision this spec gates rather than assumes (§4); the spec's default is that they may not.

## PR Intent & comprehension handshake

- **PR title (eventual):** Prove a runner can run work before a tenant finds out it cannot
- **Intent (one sentence):** An operator can answer "is this runner actually able to do work?" from the runner's own page, and a host that silently cannot is caught at deploy instead of at first lease.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/sandbox_args.zig` — `RO_SYSTEM_PATHS` and `appendBwrap` are the bind contract §3 declares. The probe MUST build its argv through `buildArgv`, never a parallel copy, or it stops proving anything about real leases.
2. `src/runner/engine/capability_probe.zig` — the existing host-capability report: how a runner describes what it can enforce, and the shape the self-test result mirrors. Its cardinality discipline (no identity in the report) applies here too.
3. `src/agentsfleetd/http/handlers/runner/heartbeat.zig` — where a capability report arrives and how `degraded` / `degraded_reason` are reconciled. The self-test result lands on this same path.
4. `src/agentsfleetd/http/handlers/fleet/runner_patch.zig` — the operator-plane mutation surface (`cordon`/`drain`/`revoke` + `assigned_policy`), its scoping and its exactly-one-of body guard. The self-test request is a new arm on this shape.
5. `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` — the operator action row the control joins, including its confirm/refresh posture.
6. `src/runner/cmd/doctor.zig` — the `Check{name, ok, detail}` result vocabulary and its human/JSON rendering; the self-test reuses it rather than inventing a second verdict shape.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/660_*.sql` | CREATE | Self-test request + latest result on the runner row, in the runner layer |
| `schema/embed.zig` | EDIT | One migration entry |
| `src/runner/sandbox_args.zig` | EDIT | The bind set becomes a declared contract with mode per entry, composed with the assigned extra binds |
| `src/runner/bind_policy.zig` | CREATE | Validates an assigned extra-bind list and composes it onto the baseline, fail-closed |
| `src/runner/selftest.zig` | CREATE | Builds the probe argv via `buildArgv`, runs it, collects per-check results |
| `src/runner/cmd/selftest.zig` | CREATE | The operator-invocable `agentsfleet-runner selftest` surface |
| `src/runner/daemon/loop.zig` | EDIT | A requested self-test runs on the heartbeat path and reports its result |
| `src/agentsfleetd/http/handlers/fleet/runner_patch.zig` | EDIT | One new action arm requesting a self-test |
| `src/agentsfleetd/http/handlers/runner/heartbeat.zig` | EDIT | Accepts and stores the self-test result |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Registered codes for a refused and a failed self-test |
| `ui/packages/app/lib/api/runners.ts` | EDIT | Typed fetcher + result shape |
| `ui/packages/app/app/(dashboard)/admin/runners/**` | EDIT | The control, its states, and the per-check result rendering |
| `public/openapi/paths/*.yaml`, `public/openapi.json` | EDIT | The new action arm and result shape |
| `docs/architecture/runner_fleet.md` | EDIT | Records the bind contract and where a host proves it can run work |
| `~/Projects/docs/changelog.mdx` | EDIT | User-visible: operators can test a runner from its page |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (every bind path and its mode is a named constant, single-sourced between the builder, the contract test and the architecture doc), **NDC** (no editable-bind-path code exists unless §4's consult returns A), **FLL** (`sandbox_args.zig`, `loop.zig` and the billing-shaped handler all grow — split before the cap), **ERR** (a refused and a failed self-test each get a declared `UZ-` code), **LOG** (the self-test outcome is an emit surface), **ITF** (the probe's integration proof runs against a real sandbox on Linux), **PUB** (the new selftest module's surface), **UIS** and **DTK** (the control and its result list use design-system primitives and token utilities), **TSC** / **TSJ** (fetcher and component conventions).
- **`dispatch/write_zig.md`** — errdefer ladder on the probe's owned results, child-process reap discipline (the probe forks), cross-compile both linux targets.
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — a new `6xx` runner-layer slot; no `ALTER`, no static strings in the schema.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — the request is an action arm on the existing runner PATCH, not a new verb; the result rides the existing read.
- **`~/Projects/dotfiles/docs/LOGGING_STANDARD.md`** — scope, event naming and error-code embedding for the self-test outcome.
- **`dispatch/write_ts_adhere_bun.md`** — the control is a user-interface surface; primitive substitution and token discipline apply.
- **`docs/AUTH.md`** (product repo) — the request is a platform-operator mutation; its scope must match the sibling admin actions exactly.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new probe module, edited loop, edited handler | errdefer ladder on owned results; single reap of the probe child; cross-compile `x86_64-linux` and `aarch64-linux` |
| PUB / Struct-Shape | yes — the selftest module is a new surface | shape verdict per new `pub`; no `pub` without an in-tree consumer |
| File & Function Length (≤350/≤50/≤70) | yes — `sandbox_args.zig` and `loop.zig` both grow | extract the probe to its own module before the cap, mirroring the capability-probe split |
| UFS (repeated/semantic literals) | yes — bind paths, modes, check names | named constants shared verbatim across builder, contract test and architecture doc |
| SCHEMA GUARD | yes — new columns in the runner layer | new `6xx` slot, one `embed.zig` entry, no `ALTER`, app-enforced vocabularies |
| UI Substitution / DESIGN TOKEN | yes — a new operator control and result list | design-system primitives and token utilities only; no raw markup, no arbitrary values |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes — new emit surface and two new failure classes | declared `UZ-` codes; the outcome log follows the standard |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/engine/capability_probe.zig` with `handlers/runner/heartbeat.zig` — a host describing what it can do, reported upward on the heartbeat and reconciled into `degraded`. The self-test result is the same arc with an executed proof instead of a static probe.
- **Reference:** `src/agentsfleetd/http/handlers/fleet/runner_patch.zig` — the operator-plane action arm, its exactly-one-of body guard and its scoping. The request mirrors it.
- **Reference:** `src/runner/cmd/doctor.zig` — the `Check{name, ok, detail}` verdict vocabulary and its dual human/JSON rendering. Reused, not re-invented, so one runner never reports health two ways.
- **Reference:** `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` — the operator action row, its pending state and its post-action refresh.

## Sections (implementation slices)

### §1 — An operator tests a runner from its own page

The runner page today shows what a host *is* (active, online, its tier) and what it *did* (its leases), with no way to ask what it *can do right now*. This adds that question as one control beside the existing operator actions, and renders the answer as a per-check list — each check named in the operator's language with its own verdict, so "DNS resolution failed inside the sandbox" is readable without a journal. A stale result is labelled with when it was produced; a result is never presented as current when it is not. **Implementation default:** the control requests and the page reports — it does not block on the runner, because the daemon picks the request up on its own heartbeat and a synchronous wait would hang the dashboard on an offline host.

- **Dimension 1.1** — triggering the control records a request and the page reflects that a test is pending → Test `test_selftest_control_requests_and_reflects_pending`
- **Dimension 1.2** — a completed result renders per-check verdicts, each with its name and failure detail → Test `test_selftest_result_renders_per_check`
- **Dimension 1.3** — a result older than the current assignment is labelled stale rather than presented as current → Test `test_stale_selftest_result_is_labelled`
- **Dimension 1.4** — an operator without the runner-write scope sees no control → Test `test_selftest_control_requires_write_scope`

### §2 — The daemon proves egress from inside a real sandbox

A check that runs on the host proves nothing about a lease: the incident that motivated this spec had a green host check and a dead sandbox for a week. This runs the probe **inside** a sandbox built by the same `buildArgv` a lease uses, under the runner's assigned policy, and reports what it found. The checks are the ones that actually failed: the resolver file resolves to a readable target, a hostname resolves, and the inference endpoint is reachable. **Implementation default:** the probe runs on the heartbeat path rather than a dedicated worker, because it must not consume a lease slot or race a tenant's work for the same sandbox resources.

- **Dimension 2.1** — the probe's sandbox argv is produced by `buildArgv` under the assigned policy, not a parallel construction → Test `test_probe_uses_the_lease_argv_builder`
- **Dimension 2.2** — with the resolver bind absent the probe fails its resolver check and the runner reports it → Test `test_probe_detects_a_dangling_resolver`
- **Dimension 2.3** — under `deny_all_egress` the probe reports egress unavailable as an expected verdict, never as a fault → Test `test_probe_reports_deny_all_as_expected`
- **Dimension 2.4** — a probe that exceeds its bound is reaped and reports a timeout verdict, leaving no orphan → Test `test_probe_timeout_reaps_and_reports`
- **Dimension 2.5** — the result carries no token, credential, or environment value → Test `test_probe_result_carries_no_secrets`

### §3 — The sandbox filesystem contract is declared and tested

The bind set is currently an unnamed array whose completeness nothing asserts — which is exactly how a missing path shipped. This makes it a declared contract: each entry names the path, its mode (read-only for system paths, read-write only for the lease workspace), and why the sandbox needs it, with a test per required entry. A path added or dropped without a matching contract entry fails a test. **Implementation default:** system paths stay read-only and the workspace stays the only read-write bind — a mode change is a security-boundary change and belongs to a spec that argues for it, not to this one.

- **Dimension 3.1** — every path in the contract appears in the built argv at its declared mode → Test `test_every_contract_path_is_bound_at_its_mode`
- **Dimension 3.2** — the workspace is the only read-write bind in a sandboxed argv → Test `test_workspace_is_the_only_writable_bind`
- **Dimension 3.3** — a contract entry with no corresponding bind, or a bind with no entry, fails the contract test → Test `test_contract_and_argv_agree_exactly`
- **Dimension 3.4** — the architecture doc's path table matches the contract constant → Test `test_architecture_doc_matches_the_contract`

### §4 — An operator adds extra read-only binds, and proves they work

A future missing path should be repairable without a deploy, so the assigned policy gains an operator-editable list of **extra** read-only binds, delivered on the heartbeat exactly as `registry_allowlist` already is, and verified by §1's control rather than by a tenant's failing run. Editable does not mean unguarded: the operator's list is **additive to the daemon-owned baseline, never a replacement** — §3's contract paths cannot be removed or re-moded from the dashboard, so no edit can un-bind the resolver and re-create the incident this milestone came from. Every entry is validated daemon-side before it reaches an argv, and a rejected entry degrades the runner with a reason rather than silently binding something else. **Implementation default:** read-only only, and a refused entry fails the whole list closed rather than partially applying — a half-applied bind set is a sandbox nobody has reasoned about.

- **Dimension 4.1** — an operator-added path is delivered on the heartbeat and appears in the built argv as a read-only bind → Test `test_operator_bind_reaches_the_argv_read_only`
- **Dimension 4.2** — the daemon-owned baseline survives any operator list: a contract path cannot be dropped or re-moded from the dashboard → Test `test_operator_list_cannot_remove_a_contract_path`
- **Dimension 4.3** — a relative path, a traversal, a non-absolute entry, or a deny-listed sensitive path is refused and the runner reports the reason → Test `test_operator_bind_validation_refuses_unsafe_paths`
- **Dimension 4.4** — a refused entry fails the list closed: no partial application, and the runner degrades rather than leasing under an unreasoned bind set → Test `test_refused_bind_list_fails_closed`
- **Dimension 4.5** — the self-test reports each operator-added bind as its own named check, so an operator sees which entry did not land → Test `test_selftest_reports_operator_binds_individually`

## Interfaces

```
CHANGED PATCH /v1/fleets/runners/{runner_id}
         Gains one action arm requesting a self-test, alongside the existing
         cordon/drain/revoke. Same platform-operator scope, same exactly-one-of
         body guard. Requesting on a revoked runner is refused, matching the
         sibling actions. Returns the recorded request, not a result — the
         result arrives asynchronously over the heartbeat.
         `assigned_policy` additionally carries an extra-bind list: absolute
         host paths bound read-only IN ADDITION TO the daemon-owned baseline.
         No mode is accepted — read-only is the only option the surface offers.
         An invalid entry is refused at the API boundary AND re-validated by
         the daemon; neither side trusts the other's check.

CHANGED POST /v1/runners/me/heartbeats
         The runner-authored body may carry a self-test result: an ordered list
         of {name, ok, detail} checks plus the policy it ran under and the
         instant it ran. Absent on every heartbeat that ran no test. The
         existing capability report is unchanged.

CHANGED GET /v1/fleets/runners/{runner_id}
         Response gains the latest self-test result and its request state.
         Every existing field is untouched.

NEW CLI  agentsfleet-runner selftest [--json]
         Runs the same probe on demand from the host, rendering the same
         {name, ok, detail} checks as `doctor`. Exits non-zero if any check
         failed, so a deploy playbook can gate on it.

NEW ERROR Self-test refused (revoked runner) and self-test failed to execute —
         two registered UZ- codes. A failed CHECK is a result, not an error.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner never picks the request up | Host offline or not heartbeating | The request stays pending and is labelled with its age; the page never shows a stale result as current |
| Probe cannot start | `bwrap` absent or the sandbox cannot be established | The self-test reports a failed check naming the mechanism, with the registered code; the runner is not silently marked healthy |
| Probe exceeds its bound | A hung resolver or an unreachable endpoint | The child is reaped, a timeout verdict is reported, no orphan process or leaked sandbox remains |
| Result arrives after a policy change | Assignment re-assigned between request and report | The result records the policy it ran under; a mismatch renders as stale, never as a verdict on the current policy |
| Self-test requested on a revoked runner | Operator acts on a terminal runner | Refused with the registered code, matching the sibling admin actions |
| Non-operator triggers the request | Scope escalation attempt | Refused by the same scope guard as cordon/drain/revoke; the control does not render for them |
| Probe output carries sensitive text | A check's detail echoes an environment value | Details are drawn from a fixed vocabulary, never raw child output; asserted by test |
| Operator assigns an unsafe extra bind | A traversal, a relative path, or a deny-listed sensitive path | Refused at the API boundary and again by the daemon; the whole list fails closed and the runner degrades with the reason — never a partial bind set |
| Operator assigns a path absent on the host | A typo, or a path that exists on one runner and not another | The bind is skipped (`--ro-bind-try` semantics), the self-test reports that named check as failed, and the operator sees which entry did not land |
| Every check passes but leases still fail | The probe's checks do not cover the real fault | A gap in check coverage is a defect: the incident that exposes it becomes a new check, and the check set only grows |

## Invariants

1. **The probe runs through the lease argv builder.** Enforced by the probe calling `buildArgv` and a test asserting the probe's argv equals a lease's argv for the same policy — a parallel construction would prove nothing about real work.
2. **The contract and the built argv agree exactly.** Enforced by the bidirectional contract test: no bind without an entry, no entry without a bind.
3. **The workspace is the only writable bind.** Enforced by a test scanning the sandboxed argv for `--bind` occurrences.
4. **A self-test result never outlives the assignment it describes.** Enforced by recording the policy on the result and rendering a mismatch as stale.
5. **An operator-supplied bind is additive and read-only.** Enforced by composing the operator list onto the daemon-owned baseline rather than replacing it, and by admitting no mode parameter on the operator surface — asserted by 4.2's test.
6. **An unsafe or unvalidated bind never reaches an argv.** Enforced by daemon-side validation ahead of `buildArgv`, failing the list closed; a refused list degrades the runner instead of leasing.
7. **Self-test results carry no secrets.** Enforced by drawing every detail from a fixed vocabulary rather than child output, asserted by test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Self-test requested | ops | An operator records a self-test request | runner id, requesting scope, outcome | No token or credential material | `test_selftest_control_requests_and_reflects_pending` |
| Self-test completed | agentsfleetd | A runner reports a result | runner id, per-check names and verdicts, policy, duration | Details from a fixed vocabulary only; never raw child output | `test_probe_result_carries_no_secrets` |
| `runner_capability_reported` | agentsfleetd | Unchanged — the existing heartbeat report | Unchanged; no new dimension | Identity stays off the metric by design | existing capability-report coverage |

Product analytics: the runner page gains one operator interaction. It is an operator-plane signal, not a tenant funnel event; no analytics/funnel playbook update is required and no monetary value or tenant identifier enters an analytics event.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `test_selftest_control_requests_and_reflects_pending` | Triggering the control records a request and the page shows it pending |
| 1.2 | e2e | `test_selftest_result_renders_per_check` | A completed result renders each check's name and verdict |
| 1.3 | unit | `test_stale_selftest_result_is_labelled` | A result whose policy differs from the current assignment renders as stale |
| 1.4 | integration | `test_selftest_control_requires_write_scope` | A read-scoped operator gets no control and a refused request |
| 2.1 | unit | `test_probe_uses_the_lease_argv_builder` | The probe's argv equals a lease's argv for the same policy |
| 2.2 | integration | `test_probe_detects_a_dangling_resolver` | With the resolver bind absent the resolver check fails and is reported |
| 2.3 | integration | `test_probe_reports_deny_all_as_expected` | Under `deny_all_egress` egress-unavailable is an expected verdict, not a fault |
| 2.4 | integration | `test_probe_timeout_reaps_and_reports` | A hung probe is reaped, reports a timeout, and leaves no orphan |
| 2.5 | unit | `test_probe_result_carries_no_secrets` | No token, credential, or environment value appears in any check detail |
| 3.1 | unit | `test_every_contract_path_is_bound_at_its_mode` | Every contract path appears in the argv at its declared mode |
| 3.2 | unit | `test_workspace_is_the_only_writable_bind` | The sandboxed argv has exactly one `--bind`, the workspace |
| 3.3 | unit | `test_contract_and_argv_agree_exactly` | A bind with no entry, or an entry with no bind, fails |
| 3.4 | unit | `test_architecture_doc_matches_the_contract` | The doc's path table matches the contract constant |
| 4.1 | integration | `test_operator_bind_reaches_the_argv_read_only` | An assigned extra path appears in the argv as a read-only bind |
| 4.2 | unit | `test_operator_list_cannot_remove_a_contract_path` | No operator list drops or re-modes a contract path |
| 4.3 | unit | `test_operator_bind_validation_refuses_unsafe_paths` | Relative, traversal, and deny-listed entries are refused with a reason |
| 4.4 | unit | `test_refused_bind_list_fails_closed` | A refused entry applies none of the list and degrades the runner |
| 4.5 | integration | `test_selftest_reports_operator_binds_individually` | Each operator-added bind is its own named check in the result |
| regression | integration | `test_runner_read_shape_unchanged` | The runner read returns every pre-existing field in the same shape |
| regression | unit | `test_existing_admin_actions_unchanged` | cordon/drain/revoke keep their behaviour and scoping |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | An operator tests a runner from its page (§1) | `test_selftest_result_renders_per_check` | test passes | P0 | |
| R2 | The probe runs through the lease argv builder (§2) | `test_probe_uses_the_lease_argv_builder` | test passes | P0 | |
| R3 | A dangling resolver is detected (§2) | `test_probe_detects_a_dangling_resolver` | test passes | P0 | |
| R4 | Contract and argv agree exactly (§3) | `test_contract_and_argv_agree_exactly` | test passes | P0 | |
| R5 | The workspace is the only writable bind (§3) | `test_workspace_is_the_only_writable_bind` | test passes | P0 | |
| R6 | An operator bind is additive, read-only, and validated (§4) | `test_operator_list_cannot_remove_a_contract_path && test_operator_bind_validation_refuses_unsafe_paths` | tests pass | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned references — zero remaining live uses.** The grep drops comment lines, so prose recording a decision cannot fail the criterion asserting it.

| Deleted symbol/column | Grep | Expected |
|-----------------------|------|----------|
| host-only health claims | `grep -rn 'doctor.*proves\|host check.*sandbox' src/ \| grep -vE ':[0-9]+:[[:space:]]*(//\|--)'` | 0 matches after RULE NLR cleanup |

## Out of Scope

- **Operator-editable read-write binds** — the extra-bind surface is read-only by construction. A writable operator bind is a different security argument and is not in this spec.
- **Replacing or re-moding the daemon-owned baseline** — §3's contract is not operator-editable; only additions are.
- **Automatic remediation** — the self-test reports; it never edits a host, restarts a daemon, or re-assigns a policy.
- **Scheduled/periodic self-tests** — this ships the operator-triggered path. A cadence is a follow-up once the check set has proven its coverage in the wild.
- **Cordoning a runner on a failed self-test** — acting on the verdict is a policy decision with tenant-visible consequences; it needs its own spec.
- **`allow_list_egress` enforcement** — still unbuilt (2.0.1); the probe reports what the assigned policy actually does today, and does not implement the strict posture.
- **macOS/Seatbelt parity** — the sandbox contract and probe are the Linux/bubblewrap path; the in-process sandbox is a separate surface.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator seeing a wall of crashed leases clicks one control and reads "DNS resolution failed inside the sandbox" — the answer that previously required tailnet access and a journal.
2. **Preserved user behaviour** — the runner page renders exactly as it does today: same header actions, same lease table, same metrics strip. The control and its result are additive; tenants see nothing new.
3. **Optimal-way check** — the optimal shape proves capability by exercising the real path rather than describing it. A host-side check is cheaper and is what failed; running the probe through the lease's own argv builder is the shortest honest form.
4. **Rebuild-vs-iterate** — iterate. The heartbeat, the capability report and the operator action row all stay; this adds a request arm, a probe, and a control.
5. **What we build** — a declared bind contract with per-path tests, an operator-editable list of *extra* read-only binds composed onto it, an in-sandbox probe reusing the lease argv builder, a request/report round trip on the existing heartbeat, one operator control, and a host-invocable `selftest` command a deploy playbook can gate on.
6. **What we do NOT build** — writable operator binds, an editable baseline, automatic remediation, scheduled runs, auto-cordon on failure, or a second health vocabulary alongside `doctor`'s.
7. **Fit with existing features** — compounds with the capability report and the degraded-runner reconciliation already driving assignment. It must not destabilise the heartbeat: a self-test that delays or fails a heartbeat is worse than the bug it detects.
8. **Surface order** — UI-first, per Indy's direction on this milestone. The repo default is CLI-first; the divergence is deliberate because the operator moment being fixed happens on the runner page. The host-invocable command ships in the same workstream so the deploy playbook is not left behind.
9. **Dashboard restraint** — the control renders only where a real probe backs it. A button that reports a fabricated or host-only verdict is worse than no button, because it re-creates the exact false confidence this milestone exists to remove.
10. **Confused-user next step** — a failed check names the mechanism that failed and the host path or endpoint involved, so the operator's next move is a host fix, not a support ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** declare the contract, prove it from inside a real sandbox, and surface one operator control. Each Section stands alone — the contract tests catch the next missing path even if the probe never runs; the probe catches faults no static contract can predict.
- **Rejected — extend `doctor` in place.** `doctor` runs on the host and its verdict is consumed by the deploy gate; widening it to fork sandboxes conflates a fast preflight with an executed proof and would have kept reporting `ok: true` for exactly the fault that motivated this spec.
- **Rejected — assert the bind set statically and stop there.** A contract test would have caught this specific missing path, but not a host whose resolver is broken, whose egress is blocked upstream, or whose policy is misassigned. The class of bug is "the sandbox cannot do the work", not "one path is missing".
- **Chosen over "not editable at all" — additive operator binds plus the self-test that verifies them (Indy's A+B).** Editability earns its risk only because three things bound it: the operator list is additive so the baseline cannot be un-bound, read-only is the only mode the surface offers, and §1's control is how an operator confirms an edit actually landed instead of discovering it through a tenant's failed run. Without those, a free-text host-path field on a security boundary would not be worth the deploy it saves.
- **Rejected — run the probe on a dedicated worker.** Consumes a lease slot and races tenant work for sandbox resources; the heartbeat path already runs on the cadence the result needs.
- **Patch-vs-refactor verdict:** this is a **patch** in shape — one new module, one action arm, one control — but it hardens a security boundary that had no test, which is why the contract (§3) is scoped as its own Section rather than folded into the probe.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - **RESOLVED — §4, bind-path editability (A/B/C consult).** Options put: **A** — operator-editable bind paths from the dashboard. **B** — daemon-owned constant, self-test detects, deploy fixes. **C** — vetted named toggles, no free-text.
    > Indy (2026-08-16): "Actually its both A & B - bind paths editable by operator and operator when they edit it, the runner gets those path and operator self-tests the runner to see if they work okay" — context: §4, decided **A+B**.

    Read as: the daemon-owned baseline (**B**) stays and is not editable, and an operator may **add** to it (**A**), with §1's self-test as the verification step that an edit landed. §4 is rewritten to that shape. The agent's original recommendation was B-only; Indy's decision supersedes it, and the risk A introduces is bounded in-spec by three invariants rather than by review discipline — additive-only (Invariant 5), read-only-only (Invariant 5), and validated-fail-closed (Invariant 6).
  - **Open — §1 surface order vs dashboard restraint.** Indy directed UI control first. Product Clarity item 9 forbids a control whose backing signal is not real, so §1 and §2 must land in the same workstream; the control must not ship against a stubbed probe. Recorded so the sequencing is a decision, not a discovery mid-EXECUTE.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
