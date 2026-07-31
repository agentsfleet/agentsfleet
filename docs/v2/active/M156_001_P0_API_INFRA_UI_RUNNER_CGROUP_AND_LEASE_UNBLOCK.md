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

# M156_001: A deployed runner enables its cgroup controllers, runs a lease, and reclaims the scope

**Prototype:** v2.0.0
**Milestone:** M156
**Workstream:** 001
**Date:** Jul 31, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the dev fleet leases nothing; three deploy jobs are red and every lease dies at init.
**Categories:** API, INFRA, UI
**Batch:** B1 — single stream; the runner change gates the gate change, which gates the acceptance jobs.
**Branch:** feat/m156-runner-cgroup-lease-unblock
**Test Baseline:** unit=3372 integration=522
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5, Jul 31, 2026) from a live read-only diagnosis of `zombie-dev-worker-ant`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Capability flows up

---

## Overview

**Goal (testable):** A freshly deployed `agentsfleet-runner` has `cpu memory pids` in its delegated `cgroup.subtree_control` before its first heartbeat, completes a lease without a `config_load_failed`, and leaves no `exec-*` cgroup behind.

**Problem:** The dev fleet accepts no work. An operator assigning a runner from the dashboard sees it go `ACTIVE · ONLINE`, then every lease fails instantly and silently — the runner's own log names no cause. A user who types a message is told *"This fleet needs instructions before it can respond. — FleetInitFailed"*: the failure is blamed on their fleet configuration, which is intact, and a raw internal error identifier is shown to them. The deploy job that is supposed to catch this fails on an unrelated-looking `cpu` message, and a host that genuinely cannot enforce limits still gets a runner deployed. Assigning the policy at all requires a fullscreen window, because the dialog cannot scroll to its Save button.

**Solution summary:** Move delegated-controller enablement from the first policy-carrying heartbeat to daemon startup, where host capability is already known — making `subtree_control` a deterministic post-condition of the unit being active rather than a race against the control loop. Give the systemd unit a `HOME`, without which the sandboxed child's config load fails closed and kills every lease. Stop swallowing the cause of that failure. Reclaim execution cgroups with the one call the kernel permits. Harden the readiness gate to prove the host *before* deploying and to report every missing controller instead of the first. Make the policy dialog reachable and its isolation options land on one row.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m156): enable runner cgroups at startup and unblock leasing
- **Intent (one sentence):** A runner an operator assigns from the dashboard actually runs work, and a host that cannot enforce isolation is refused before it receives a deployment.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/engine/CgroupScope.zig` — carries `enableDelegatedControllers` and its doc comment explaining why the delegatee must write `subtree_control`; also the `destroy()` reclaim path this spec corrects. Read both before moving either.
2. `src/runner/daemon/policy_apply.zig` — the current lazy call site and the `Gates.controllers_enabled` one-shot flag. The startup move must not leave a second, contradictory enablement path (RULE NDC).
3. `src/runner/sandbox_args.zig` — `ENV_PASSTHROUGH_ALLOWLIST` and the deny-prefix assertion. `HOME` is already allowlisted and load-bearing; the fix belongs in the unit, not the allowlist.
4. `playbooks/founding/06_runner_bootstrap_dev/03_deploy_readiness.sh` + its sibling `deploy_readiness_test.sh` — the gate and its existing stub-driven test harness to extend.
5. `docs/architecture/runner_fleet.md` §Capability flows up — states the daemon probes `subtree_control` **at startup**; today's lazy enablement contradicts it. This spec closes that divergence.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/main.zig` | EDIT | Startup gains the controller-enablement step, before the control loop hands off. |
| `src/runner/engine/CgroupScope.zig` | EDIT | `destroy()` reclaims a cgroup with a directory removal the kernel permits; enablement stays here and gains a not-delegated classification. |
| `src/runner/daemon/policy_apply.zig` | EDIT | Lazy enablement and its one-shot gate are removed now that startup owns it. |
| `src/runner/engine/runner.zig` | EDIT | The `Config.load` failure logs its cause instead of discarding it. |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | Unit supplies `HOME` so the sandboxed child can resolve a config directory. |
| `playbooks/founding/06_runner_bootstrap_dev/03_deploy_readiness.sh` | EDIT | Reports every missing controller; gains a host-capability probe usable before deploy. |
| `playbooks/founding/06_runner_bootstrap_dev/deploy_readiness_test.sh` | EDIT | Covers the all-controllers report and the pre-deploy probe. |
| `.github/workflows/deploy-dev.yml` | EDIT | Host capability is proven before the runner is deployed, not only after. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` | EDIT | Dialog body scrolls so the footer stays reachable on a short viewport. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | Same scroll treatment — it renders the same policy fields. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` | EDIT | The three isolation options stop wrapping two-then-one. |
| `ui/packages/app/components/domain/fleetFailureCopy.tsx` | EDIT | An unclassifiable cause stops being reported as the user's missing instructions, and no raw error identifier is shown. |
| `ui/packages/app/components/domain/fleetFailureCopy.test.ts` | EDIT | Covers the inverted default and the preserved missing-instructions case. |
| `src/runner/engine/cgroup_scope_test.zig` | CREATE | Unit coverage for reclaim classification and the not-delegated error. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx` | EDIT | Asserts the scroll affordance on the dialog body. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.test.tsx` | EDIT | Asserts the isolation options share one row at the breakpoint. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the lazy enablement path is deleted, not left beside the new one), **NLR** (touch-it-fix-it on the reclaim path), **UFS** (`HOME`, the controller set, and the gate's controller names are named constants, shared verbatim across Zig and shell), **ECL** (a not-delegated cgroup is a distinct error class from a write failure — the daemon must tell them apart), **ORP** (removing `Gates.controllers_enabled` sweeps its references), **DFS** (no dead struct field left on `Gates`), **TST-NAM** (test identifiers carry no milestone id).
- `dispatch/write_zig.md` — `*.zig` surface: `errdefer` placement on the startup path, tagged-union/error-set results, file ≤350 and function ≤50, cross-compile both linux targets.
- `dispatch/write_shell.md` — the readiness gate: quoted expansions, no untrusted `eval`, repository shell compatibility.
- `dispatch/write_ts_adhere_bun.md` — the dialog and policy fields: design-system primitive over raw HTML, token utility over arbitrary values.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — the new startup event and the now-populated failure cause.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — four `*.zig` files | Cross-compile `x86_64-linux` and `aarch64-linux`; `errdefer` on the startup enablement; no leak on the failure arm. |
| PUB / Struct-Shape | yes — `CgroupScope` gains a reclaim classification | Shape verdict recorded per new pub surface; keep the error set closed rather than widening to `anyerror`. |
| File & Function Length (≤350/≤50/≤70) | yes — `CgroupScope.zig` is already near the file cap | Measure before editing; if the reclaim change pushes it past 350, split the scope-lifecycle half into its own file rather than trimming comments. |
| UFS (repeated/semantic literals) | yes | `HOME`, the `cpu memory pids` set, and the gate's controller names become named constants; the Zig set and the shell set stay identical strings. |
| UI Substitution / DESIGN TOKEN | yes — two dialogs and the policy fields | Scroll affordance and grid columns via design-system primitives and token utilities; no arbitrary `*-[…]` values. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING + LIFECYCLE yes; ERROR REGISTRY reuse; SCHEMA no | New startup event follows the logging standard; startup enablement respects init/deinit ordering; reuse `UZ-EXEC-012` and `UZ-RUN-*` — no new code minted; no schema touched. |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/cgroup-delegate.sh` — the repository's own working precedent. It already writes `subtree_control`, already handles the no-internal-process constraint, and already verifies each controller landed. The daemon's startup path mirrors its sequence and its verify-after-write discipline rather than inventing one.
- **Reference:** `src/runner/engine/capability_probe.zig` — reads `subtree_control` and already tolerates a missing/unreadable subtree. The startup enablement must leave the probe's contract intact so the reported capability stays truthful.
- **Reference:** `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerDialogs.tsx` — the sibling dialog whose overflow handling the two policy dialogs should match.

## Sections (implementation slices)

### §1 — Controllers are enabled at startup, not on first policy

Host capability is knowable the moment the daemon starts; nothing about it depends on which policy the control plane later assigns. Moving enablement to startup makes `subtree_control` a post-condition of an active unit, which is what the readiness gate and the capability probe both already assume. **Implementation default:** enablement runs unconditionally on Linux at startup and a failure is logged and non-fatal, because a `dev_none` host builds no cage and must still start; the leasing refusal stays where it is, driven by the reconciliation.

- **Dimension 1.1** — On Linux, startup writes the controller set to the delegated base before the control loop begins → Test `test_startup_enables_delegated_controllers`
- **Dimension 1.2** — **DONE** — A cgroup base that is not delegated is classified distinctly from a write failure → Test `test_not_delegated_is_distinct_from_write_failure`
- **Dimension 1.3** — Enablement is idempotent across a restart → Test `test_enable_controllers_is_idempotent`
- **Dimension 1.4** — **DONE** — The lazy call site and its one-shot gate field no longer exist → Test `test_policy_apply_has_no_controller_gate`

### §2 — A lease can load its config under systemd

The daemon runs with the environment systemd gives it, which contains no `HOME`. The passthrough allowlist forwards `HOME` only when the daemon has it set, so the sandboxed child inherits nothing and its config load fails closed — killing every lease at init. The unit is the right place to fix it: the allowlist is already correct. **Implementation default:** the unit sets `HOME` to a root-owned directory that exists on a bare-metal host, because the daemon runs without a `User=` and systemd supplies no home for it.

- **Dimension 2.1** — **DONE** — The unit defines `HOME` → Test `test_unit_defines_home`
- **Dimension 2.2** — **DONE** — The passthrough allowlist forwards `HOME` when the daemon has it → Test `test_home_reaches_sandboxed_child`
- **Dimension 2.3** — A lease completes without a config-load failure on a host with the unit installed → Test `test_lease_runs_with_unit_environment`

### §3 — A failed config load names its cause, to the operator and to the user

The failure that took the dev fleet down logged an error code and nothing else, which is why diagnosis needed a host login rather than a journal read. The same failure reaches the user as *"This fleet needs instructions before it can respond."* — the chat surface recognises five exact runner cause lines and blames the fleet for everything else, so a runner-side fault is reported as the user's misconfiguration, with a raw internal error identifier appended. An error that is caught must be named, and a cause the surface cannot classify must not be attributed to the user. **Implementation default:** log the error name alongside the existing code, keep the returned error unchanged because callers already branch on it, and invert the chat surface's default so an unrecognised cause reads as a runner-side failure rather than a missing-instructions one.

- **Dimension 3.1** — The config-load failure log carries the underlying error name → Test `test_config_load_failure_names_error`
- **Dimension 3.2** — A startup-posture failure whose cause is unrecognised reads as a runner-side failure, not a missing-instructions one → Test `test_unrecognised_cause_is_not_blamed_on_the_fleet`
- **Dimension 3.3** — A fleet genuinely lacking instructions still gets the instructions sentence → Test `test_missing_instructions_keeps_its_sentence`
- **Dimension 3.4** — No raw internal error identifier reaches the chat surface → Test `test_raw_error_identifier_never_shown`

### §4 — Execution cgroups are reclaimed

Teardown removes an execution scope with a recursive tree delete, but a cgroup's control files cannot be unlinked, so every reclaim fails and the directories accumulate — 25 were resident on the dev host at diagnosis. A cgroup is emptied of processes and then removed as a directory. **Implementation default:** remove the scope directory directly and treat a still-populated scope as a distinct, retryable outcome, because a scope with live processes is a supervision bug, not a filesystem one.

- **Dimension 4.1** — A process-empty scope is removed at teardown → Test `test_destroy_removes_empty_scope`
- **Dimension 4.2** — Teardown leaves no `exec-*` directory behind after a completed lease → Test `test_no_orphan_scope_after_lease`
- **Dimension 4.3** — A reclaim failure is logged with the reason and does not mask the lease's own outcome → Test `test_reclaim_failure_preserves_lease_result`

### §5 — The readiness gate proves the host, before and after

Two defects: the gate returns on the first missing controller, so a host missing all three reports only `cpu`; and it runs only after the runner is deployed, so a host that cannot enforce limits still receives one. **Implementation default:** the pre-deploy probe reads the root controller set and the parent slice's `subtree_control` — the two things that are true independent of the daemon — while the post-deploy check keeps asserting the delegated subtree.

- **Dimension 5.1** — Every missing controller is reported, not the first → Test `test_gate_reports_all_missing_controllers`
- **Dimension 5.2** — A host missing a controller at the root fails before deployment → Test `test_pre_deploy_probe_rejects_incapable_host`
- **Dimension 5.3** — A capable host passes the pre-deploy probe → Test `test_pre_deploy_probe_accepts_capable_host`
- **Dimension 5.4** — The post-deploy check still fails a runner whose delegated subtree is empty → Test `test_post_deploy_check_requires_delegated_subtree`

### §6 — The policy dialog is reachable and its options are legible

An operator cannot assign a policy without resizing the window, because the dialog body does not scroll and the footer falls below the fold. The three isolation options wrap two-then-one, leaving an orphan card. Both are on the only path that makes a runner useful. **Implementation default:** constrain the dialog body's height and scroll it, matching the sibling dialog, rather than shortening the copy — the copy is load-bearing operator guidance.

- **Dimension 6.1** — The dialog body scrolls and the footer stays reachable on a short viewport → Test `test_policy_dialog_body_scrolls`
- **Dimension 6.2** — The three isolation options occupy one row at the breakpoint → Test `test_isolation_options_share_one_row`
- **Dimension 6.3** — The add-runner dialog carries the same scroll affordance → Test `test_add_runner_dialog_body_scrolls`

## Interfaces

```
CgroupScope.enableDelegatedControllers(io, alloc) — unchanged signature.
  Error set gains a not-delegated classification, distinct from a write failure.
  Callers: runner startup only. The policy-apply call site is removed.

CgroupScope.destroy(limits) -> CgroupMetrics — unchanged signature and return.
  Reclaim outcome is logged, never returned; the lease result is not affected.

03_deploy_readiness.sh
  env REQUIRE_RUNNER_CGROUP_DELEGATION=1  -> post-deploy delegated-subtree check
  env REQUIRE_HOST_CGROUP_CAPABILITY=1    -> pre-deploy host probe (new)
  Both may be set independently. Output: one line per controller checked;
  a failing run names every missing controller. Exit non-zero on any miss.

deploy/baremetal/agentsfleet-runner.service
  Gains HOME. Delegate, DelegateSubgroup, and the accounting flags are unchanged.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Controllers unavailable at startup | Host kernel lacks a controller, or the unit is not delegated | Startup logs the missing set and continues; the capability probe reports what is actually enabled and the reconciliation degrades the row with `cgroup controllers not delegated`. The operator sees a degraded runner naming the mechanism. |
| Base not delegated | The daemon is not running under the delegated subgroup | Classified distinctly from a write failure so the log names the real cause; startup continues, the row degrades. |
| `HOME` absent from the unit | Unit edited or an older unit still installed | The child's config load fails; the log now names the underlying error, and the lease fails closed and is redeliverable. |
| Scope still populated at teardown | A child survived the kill | Reclaim is skipped and logged with the reason; the lease's own result is reported unchanged. |
| Pre-deploy probe cannot reach the host | Secure Shell (SSH) or tailnet failure | The gate exits non-zero with the connection error; no deployment proceeds. |
| Gate runs against a host with a partial controller set | Kernel or boot configuration drift | Every missing controller is named in one run; the job fails before the runner is deployed. |
| Dialog opened on a short viewport | Small window or low-height display | Body scrolls; footer actions stay reachable without resizing. |
| Runner-side failure reaches the chat surface with an unclassifiable cause | A new or unmapped runner cause line | The surface reports a runner-side failure and withholds any raw internal identifier; the user is never told their intact fleet lacks instructions. |

## Invariants

1. A Linux daemon that reaches its control loop has attempted controller enablement — enforced by the call sitting on the startup path ahead of the loop handoff, asserted by a startup test.
2. There is exactly one controller-enablement call site — enforced by a repository grep in the Dead Code Sweep returning a single match.
3. The Zig controller set and the readiness gate's required set are the same strings — enforced by named constants and a test asserting the shell gate's set matches the Zig set verbatim.
4. A sandboxed child never receives a variable outside the passthrough allowlist — already enforced by the `forkExec` deny-prefix assertion; unchanged by this spec and re-asserted by the existing test.
5. Teardown never reports a lease result altered by a reclaim failure — enforced by the reclaim outcome being logged rather than returned, asserted by a negative test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cgroup_controllers_enabled` | ops | Daemon startup completes controller enablement | Base path, controller set | No credential or tenant data — paths and controller names only | `test_startup_enables_delegated_controllers` |
| `cgroup_controllers_unavailable` | ops | Startup enablement fails or the base is not delegated | Error name, classification, controller set | Same — no credential or tenant data | `test_not_delegated_is_distinct_from_write_failure` |
| `config_load_failed` | ops | A lease's config load fails | Existing error code plus the underlying error name | Error name only — never config values or key material | `test_config_load_failure_names_error` |
| `cleanup_failed` | ops | An execution scope cannot be reclaimed | Scope path, reason | Path and reason only | `test_reclaim_failure_preserves_lease_result` |

No product analytics event is added, renamed, or removed; the dashboard change is a layout fix on an existing surface. No analytics or funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_startup_enables_delegated_controllers` | The startup path invokes enablement before the loop handoff on a Linux target. |
| 1.2 | unit | `test_not_delegated_is_distinct_from_write_failure` | A base resolving outside the delegated subgroup yields the not-delegated classification, not a write failure. |
| 1.3 | integration | `test_enable_controllers_is_idempotent` | Enabling twice against a real delegated cgroup leaves the same controller set and returns success both times. |
| 1.4 | unit | `test_policy_apply_has_no_controller_gate` | Applying a cage-tier policy performs no cgroup work and the gate struct carries no enablement field. |
| 2.1 | unit | `test_unit_defines_home` | The shipped unit file defines `HOME`. |
| 2.2 | unit | `test_home_reaches_sandboxed_child` | With `HOME` set in the parent environment, the child's environ contains it; with it unset, the child's environ omits it and no other variable is substituted. |
| 2.3 | integration | `test_lease_runs_with_unit_environment` | A lease executed with the unit's environment reaches completion with no `config_load_failed`. |
| 3.1 | unit | `test_config_load_failure_names_error` | An injected config-load failure produces a log record carrying the underlying error name alongside the existing code. |
| 3.2 | unit | `test_unrecognised_cause_is_not_blamed_on_the_fleet` | A startup-posture failure carrying a cause outside the known set renders the runner-side sentence, not the missing-instructions one. |
| 3.3 | unit | `test_missing_instructions_keeps_its_sentence` | A startup-posture failure with no cause still renders the missing-instructions sentence. |
| 3.4 | unit | `test_raw_error_identifier_never_shown` | A cause matching an internal error-identifier shape is not appended to the rendered sentence. |
| 4.1 | integration | `test_destroy_removes_empty_scope` | Teardown of a process-empty scope removes the directory; a subsequent stat finds nothing. |
| 4.2 | integration | `test_no_orphan_scope_after_lease` | After a completed lease, the delegated base contains zero `exec-*` directories. |
| 4.3 | unit | `test_reclaim_failure_preserves_lease_result` | An injected reclaim failure logs the reason and returns metrics identical to the success path's shape. |
| 5.1 | unit | `test_gate_reports_all_missing_controllers` | A stubbed subtree missing all three controllers produces three named misses in one run, not one. |
| 5.2 | unit | `test_pre_deploy_probe_rejects_incapable_host` | A stubbed root controller set without `cpu` exits non-zero and names `cpu`. |
| 5.3 | unit | `test_pre_deploy_probe_accepts_capable_host` | A stubbed root set containing all three exits zero. |
| 5.4 | unit | `test_post_deploy_check_requires_delegated_subtree` | An empty delegated subtree still fails the post-deploy check. |
| 6.1 | unit | `test_policy_dialog_body_scrolls` | The dialog body carries a bounded height and a vertical scroll affordance. |
| 6.2 | unit | `test_isolation_options_share_one_row` | The isolation group resolves to three columns at the breakpoint. |
| 6.3 | unit | `test_add_runner_dialog_body_scrolls` | The add-runner dialog body carries the same bounded height and scroll affordance. |
| regression | integration | `test_capability_probe_reports_enabled_set` | The probe continues to report exactly the controllers present in `subtree_control` after startup enablement. |
| regression | unit | `test_dev_none_still_starts_without_delegation` | A `dev_none` host with no delegated subtree still starts and refuses leases through the reconciliation, not a crash. |
| idempotency | integration | `test_restart_reenables_controllers` | Restarting the unit re-enables the controller set without operator action. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A restarted runner has the controller set enabled before its first beat (§1) | `systemctl restart agentsfleet-runner && sleep 2 && cat /sys/fs/cgroup/system.slice/agentsfleet-runner.service/cgroup.subtree_control` | `cpu memory pids` | P0 | |
| R2 | A lease completes with no config-load failure (§2, §3) | `journalctl -u agentsfleet-runner --since -10min \| grep -c config_load_failed` | `0` | P0 | |
| R3 | No execution cgroup is left behind (§4) | `find /sys/fs/cgroup/system.slice/agentsfleet-runner.service -maxdepth 1 -name 'exec-*' \| wc -l` | `0` | P0 | |
| R4 | The gate names every missing controller and rejects an incapable host (§5) | `bash playbooks/founding/06_runner_bootstrap_dev/deploy_readiness_test.sh` | exit 0 | P0 | |
| R5 | The policy dialog is usable on a short viewport (§6) | `make test-unit-app` | exit 0 | P1 | |
| R6 | Exactly one controller-enablement call site remains | `grep -rn "enableDelegatedControllers" src/ \| grep -v "CgroupScope.zig" \| wc -l` | `1` | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `controllers_enabled` | `grep -rn "controllers_enabled" src/ \| head` | 0 matches |
| lazy enablement call site | `grep -rn "enableDelegatedControllers" src/runner/daemon/ \| head` | 0 matches |

## Out of Scope

- **The production worker host.** This spec proves the fix on dev; rolling it to `zombie-prod-worker-ant` follows once dev is green.
- **Assigning a policy automatically at enrollment.** The dashboard remains the assignment surface by design (`06_runner_bootstrap_dev/001_playbook.md`); the operator gap this spec closes is reachability, not automation.
- **The remaining acceptance failures** — `workspace-create`, `secrets-lifecycle`, the two `dashboard-performance` cases, the Events heading strict-mode violation, and the `UZ-BUNDLE-001` frontmatter failure. They are independent of runner liveness and get their own spec once this one confirms which of the six were runner-dependent.
- **Egress enforcement.** `egress_enforcement` stays pinned false; no change here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator adds a runner, assigns Landlock (full) from a dialog that fits on their screen, and watches the runner pick up a lease and return a result. No terminal, no host login.
2. **Preserved user behaviour** — Policy assignment stays a dashboard action delivered on the heartbeat. Existing runners keep their assignments. `dev_none` local development keeps starting on hosts with no delegated subtree. Lease redelivery semantics are unchanged.
3. **Optimal-way check** — The direct route to moment #1 is §2 alone: `HOME` is what kills the lease. §1 and §5 exist because the gate that should have caught this could not — it asserted a state the daemon only reached by luck. The gap to the unconstrained-optimal shape is that host capability is still proven by a shell probe over SSH rather than reported by the runner's own preflight; acceptable now because the pre-deploy moment has no runner running to ask.
4. **Rebuild-vs-iterate** — Iterate. Every defect here is a wrong call site or a missing environment variable in an otherwise sound design; the delegation model, the capability probe, and the reconciliation are all correct and stay.
5. **What we build** — Startup enablement; `HOME` in the unit; an error name in one log line; a directory removal that works on cgroups; a gate that reports fully and runs early; a dialog that scrolls.
6. **What we do NOT build** — Automatic policy assignment (the dashboard is the intended surface). A new error code (existing codes cover these classes). Retention or partitioning of orphan scopes (removing them correctly is the fix). A production rollout (dev proves it first).
7. **Fit with existing features** — Compounds with the capability reconciliation, which becomes truthful once startup enablement makes the probe's reading stable. It must not destabilize the fail-closed leasing refusal: a host that genuinely cannot isolate must still refuse work, not start leasing because controllers were enabled optimistically.
8. **Surface order** — Both. The runner and gate changes are the substance; the dialog change ships alongside because it is on the only path that exercises them.
9. **Dashboard restraint** — The dialog gains no new control and no new claim. It gets a scroll affordance and one row of options; the degraded reason already displayed stays the single source of operator truth.
10. **Confused-user next step** — The degraded reason on the runner row names the missing mechanism in operator vocabulary and maps to a bootstrap playbook step; the journal now names the config-load cause. Both are self-serve.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections, one per defect, ordered by dependency: enablement (§1) makes the gate (§5) meaningful; `HOME` (§2) is what actually restores leasing; diagnosability (§3) and reclaim (§4) were found by the same trace and touch the same files. One Workstream, because splitting would put the gate change in a different Pull Request from the behaviour it gates.
- **Alternatives considered:** (a) *Gate-only fix* — leave lazy enablement and make the post-deploy check wait for the first heartbeat. Rejected: it encodes the race rather than removing it, and leaves the gate asserting control-plane state under a host-readiness name. (b) *Provisioning-only fix* — add a policy-assignment step to the bootstrap playbook. Rejected: it treats a missing assignment as the root cause when the actual root cause of the lease failures is `HOME`, and it would have shipped a fleet that still could not run work. (c) *Deferring the dialog fix* — rejected by Indy in-session; it is on the path that makes a runner useful.
- **Patch-vs-refactor verdict:** this is a **patch**, and deliberately so. The problem is a set of wrong call sites and a missing environment variable inside a design that is otherwise correct; solution-size matches problem-size. The one structural move — relocating enablement to startup — is a call-site change, not a redesign, and it deletes more code than it adds.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/runner_fleet.md` §Capability flows up states the daemon probes `subtree_control` at startup; today's lazy enablement contradicts the doc, and §1 closes the divergence in the code rather than amending the doc. Gate-flag triage: none yet.
  - Live diagnosis, Jul 31, 2026, read-only on `zombie-dev-worker-ant`: root `cgroup.controllers` carried `cpu`, `system.slice/cgroup.subtree_control` carried `cpu memory pids`, and the service's own `subtree_control` was empty — disproving the kernel/boot hypothesis the work was picked up under. The daemon environment contained no `HOME`. 25 orphan `exec-*` directories were resident.
  - > Indy (2026-07-31): "Startup enablement + gate hardening (Recommended)" — context: choosing P1 scope over a gate-only or provisioning-only fix.
  - > Indy (2026-07-31): "I have updated the policy and its online / busy" — context: assigning a Landlock (full) policy to the dev runner from the dashboard mid-diagnosis, which confirmed the enablement chain and surfaced the `HOME` lease failure underneath it.
  - > Indy (2026-07-31): "Fold into this spec" — context: the policy dialog scroll and isolation-grid defects, folded into §6 rather than tracked as a separate spec.
  - > Indy (2026-07-31): "Approve both (Recommended)" — context: acking the `deploy/baremetal/agentsfleet-runner.service` deploy-config edit (§2, `HOME`) and the `.github/workflows/deploy-dev.yml` CI/CD edit (§5, pre-deploy host probe), both otherwise blocked by Hard Safety.
  - > Indy (2026-07-31): "Same tree, no worktree" — context: CHORE(open) implements on a branch in the existing checkout rather than hydrating a fourth worktree.
  - > Indy (2026-08-01): "This fleet needs instructions before it can respond. — FleetInitFailed (That is the error i see now when i type Hello in a fleet)" — context: the user-facing arm of the same `HOME` defect. `fleetFailureCopy.tsx` classifies only five exact runner cause lines and defaults everything else to blaming the fleet, so a runner-side fault is reported as the user's misconfiguration with a raw error identifier appended. Folded into §3 as Dimensions 3.2–3.4 rather than tracked separately.
- **Metrics review** — pending implementation.
- **Skill-chain outcomes** — pending: `/write-unit-test`, `/write-integration-test`, gstack `/review`, `kishore-babysit-prs`.
- **Deferrals** — none.
