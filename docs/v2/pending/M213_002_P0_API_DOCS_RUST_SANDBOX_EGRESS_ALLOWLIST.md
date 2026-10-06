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

# M213_002: The Rust sandbox reaches exactly what the API assigned it — all egress, none, or the allowlist, enforced by nftables on a veth pair the sandbox cannot touch

**Prototype:** v2.0.0
**Milestone:** M213
**Workstream:** 002
**Date:** Oct 06, 2026
**Status:** PENDING
**Priority:** P0 — M213_001 cannot cut over without it: on `allow_all` the Zig child shares the host network today and the Rust sandbox reaches nothing, so every package install inside a sandbox breaks on the day of the cutover
**Categories:** API, DOCS
**Batch:** B1 — ships in M213_001's Pull Request (Indy, Oct 06, 2026)
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M213_001 — the same Pull Request; this spec's §1 lands before M213_001 deletes `src/runner/`, because the Zig egress code is the behaviour reference read from `origin/main` · M211_001 — reshapes `afr_sandbox` (`bubblewrap.rs`, `warm_slots`, `probe.rs`); this spec's paths are re-confirmed against its merged tree at PLAN
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 06, 2026) from `origin/main` at `8cb2318ec` and `feat/m211-sandbox-tools-and-nested-loops` at `74956c0f7`; every cite below was read from source
**Canonical architecture:** `docs/architecture/runner_fleet.md` §"Egress model — outbound is the only network surface"; `docs/architecture/runner_execution.md` (the per-lease network allowlist bullet and the "No runner crate builds the per-lease network allowlist" non-goal this spec retires)

---

## Overview

**Goal (testable):** `test_sandbox_egress_follows_the_assigned_policy` — on a real kernel, a sandbox under `allow_list_egress` connects to an allow-listed host and fails to reach any other address, port 53 included; under `deny_all_egress` it reaches nothing; under `allow_all` it reaches the host's network; and a runner whose probe passes reports `egress_enforcement: true`, so the daemon stops reading it degraded.
**Problem:** Operators already set egress through the API, in two places: the runner's `assigned_policy` (`PATCH /v1/fleets/runners/{runner_id}`, `network_policy` + `registry_allowlist`) and the fleet's `network.allow` (`PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}`, `trigger_markdown`/`config_json`). The Rust sandbox reads neither. `bubblewrap.rs` passes `--unshare-net` on every launch (M211 branch, `afr_sandbox/src/bubblewrap.rs:65`), so `allow_all` behaves as `deny_all_egress` and an `npm install` inside a sandbox fails. `allow_list_egress` has never been enforced by either runner: the Zig probe pins `EGRESS_ENFORCEMENT_BUILT = false` (`src/runner/engine/capability_probe.zig:20`), the Rust probe pins `egress_enforcement: false` (`afr_supervisor/src/capability.rs:51`), and the published runner page says so (`runners.mdx:101`).
**Solution summary:** The supervisor passes the runner's assigned `network_policy` to the sandbox. `allow_all` shares the host network namespace. `deny_all_egress` keeps today's empty namespace. `allow_list_egress` gives each sandbox its own namespace joined to the host by one veth pair, with default-deny nftables rules in the **host** namespace on the host-side link, admitting only the IPv4 set the supervisor resolved at lease bind. That set comes from the runner's `registry_allowlist` plus the fleet's `network.allow`. The sandbox gets a rendered `/etc/hosts` and a resolver-less `/etc/resolv.conf`; port 53 is dropped. The probe reports `egress_enforcement: true` only once it has built and torn down a scope. No API shape changes: the daemon already demands egress control for `allow_list_egress` (`afd_runner/src/reconcile.rs:225-245`).

## PR Intent & comprehension handshake

- **PR title (eventual):** rides M213_001's: `feat(runner)!: deploy the Rust runner; retire the Zig runner and NullClaw`
- **Intent (one sentence):** The network a fleet's sandbox can reach is the one the operator and the fleet author set through the API, enforced by the kernel, on the Rust runner.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/network/EgressScope.zig`, `Plan.zig`, `AllowList.zig`, `nfnetlink_rule.zig`, `rtnetlink.zig` on `origin/main` — the behaviour reference: per-worker table, `/30` per worker, rules in the host namespace, IPv4-only refusal, an allowlist cap that fails closed, idempotent teardown. Read it for behaviour; no Rust line names it (M213_001 Dimension 3.1).
2. `rustd/crates/afd_runner/src/reconcile.rs` — what the daemon already demands: `enforces_egress` and `shares_host_net` define each policy's meaning; `EgressControl` is proven by `report.egress_enforcement`.
3. `rustd/crates/afd_gate/src/policy/shape.rs` — the fleet half: an absent `network` block is an empty allow list, never "reach anything"; `read_only` and `read_post_paths` are Layer 7 (L7) rules only the supervisor's `afr_egress` can enforce.
4. `docs/architecture/runner_fleet.md` §Egress model — resolver-less naming, port 53 drop, rules owned by root outside the child; its Zig names (`uzveth`, `network/AllowList.build`) are restated for Rust in §3.
5. `rustd/crates/afr_sandbox/src/bubblewrap.rs`, `warm_slots.rs`, `probe.rs` on the merged M211 tree — where the namespace flag is set, where a slot outlives a lease, and where host capabilities are probed.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/bubblewrap.rs` | EDIT | The network flag follows the policy: `--share-net` for `allow_all`, the unshared namespace otherwise |
| `rustd/crates/afr_sandbox/src/egress.rs`, `egress/{plan,link,rules,names}.rs` + their `tests.rs` | CREATE | The scope: allowlist merge and resolution, veth pair and addresses over rtnetlink, the nftables table over nfnetlink, the rendered `/etc/hosts` and `/etc/resolv.conf`, teardown |
| `rustd/crates/afr_sandbox/src/warm_slots.rs` | EDIT | A slot's veth lives with the slot; its rule set is filled at lease bind and emptied at release |
| `rustd/crates/afr_sandbox/src/probe.rs` | EDIT | The egress probe builds and tears down one scope; its verdict becomes `egress_enforcement` |
| `rustd/crates/afr_sandbox/src/mounts.rs` | EDIT | The two rendered resolver files are bound read-only under `allow_list_egress` |
| `rustd/crates/afr_sandbox/src/error.rs` | EDIT | Egress setup kinds, through `error_shell!` (`docs/RUST_ERROR_STANDARD.md`) |
| `rustd/crates/afr_sandbox/examples/kernel_lane/egress.rs` | CREATE | The real-kernel proof for all three policies, in `make test-runner-kernel` |
| `rustd/crates/afr_sandbox/Cargo.toml`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | The netlink dependency chosen at PLAN |
| `rustd/crates/afr_supervisor/src/capability.rs` | EDIT | `egress_enforcement` comes from the probe, not a constant |
| `rustd/crates/afr_supervisor/src/heartbeat.rs` | EDIT | The assigned `network_policy` and `registry_allowlist` reach the sandbox engine |
| `rustd/crates/afr_supervisor/src/lease_loop.rs`, `lease_loop/refusal.rs` | EDIT | Lease bind passes the fleet's allowlist; a scope that cannot be built refuses the lease |
| `rustd/crates/afr_supervisor/src/lib.rs` | EDIT | The boot sweep removes egress tables and links a crashed runner left behind |
| `docs/architecture/runner_fleet.md`, `docs/architecture/runner_execution.md` | EDIT | §Egress model describes the Rust scope; the non-goal at `runner_execution.md:202` leaves |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (table, chain, set and link name prefixes, the `/30` base, the allowlist cap, as single named constants shared by builder, sweep and tests); NDC and NLR (the constant `egress_enforcement: false` and the unconditional `--unshare-net` leave, not sit beside the new path); NLG (no "legacy network mode"); OBS (each refusal names its reason); TST-NAM; TCF.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — every new fallible signature; no hand-written error type.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runner page's egress lines on M213_001's docs branch.
- `AGENTS.orly.md` §Hard Safety — "Install-process launches in core paths": netlink from Rust, never the `nft` or `ip` binaries.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — new module | `egress` split by concern (plan, link, rules, names) from the start |
| UFS | yes | Names and limits declared once in `egress.rs` |
| LOGGING | yes | One structured line per scope build, refusal and sweep, no allowlist contents beyond host count |
| Architecture consult | yes | `runner_fleet.md` §Egress model rewritten in the same Pull Request |
| Schema guard | no | No schema change |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/network/` on `origin/main` — the same design (own namespace, veth, host-side nftables, resolve at setup, resolver-less) built for Zig and never switched on. Mirrored by behaviour; one divergence: the inference endpoint leaves the merged set, because the Rust agent loop calls providers from the supervisor, outside the sandbox (`docs/architecture/runner_execution.md` crate graph).

## Sections (implementation slices)

### §1 — The sandbox honours the assigned policy

The heartbeat's `assigned_policy` already reaches `afr_supervisor` and today only labels the self-test (`heartbeat.rs:89-91`). It now reaches the sandbox engine. `allow_all` passes `--share-net`, `deny_all_egress` keeps the empty namespace, and `allow_list_egress` keeps the empty namespace until §2 attaches its link. A change of assignment applies to sandboxes built after it; a warm slot built under the old policy is retired, never reused. **Implementation default:** an absent assignment is `FAIL_CLOSED_DEFAULT` (`afd_wire/src/runner.rs:70`) because a runner must never open egress on missing input.

- **Dimension 1.1** — Each policy produces its namespace flag, and no assignment produces the fail-closed one → Test `test_bubblewrap_network_flag_per_policy`
- **Dimension 1.2** — A reassignment retires warm slots built under the previous policy → Test `test_reassignment_retires_warm_slots`
- **Dimension 1.3** — On a real kernel, `allow_all` reaches a host-network listener and `deny_all_egress` reaches nothing → Test `test_kernel_allow_all_and_deny_all`
- **Dimension 1.4** — The runner binary still settles a lease under each of the three policies (regression on M213_001 Dimension 1.4) → Test `test_runner_binary_runs_a_lease_per_policy`

### §2 — `allow_list_egress` is enforced by the kernel

At lease bind the supervisor merges the runner's `registry_allowlist` with the fleet's `network_policy.allow`, and resolves each name to IPv4 addresses with the host resolver. When the fleet sets `read_only`, its hosts stay out of the merged set and are reached only through `http_request`, because nftables cannot enforce a method. A slot's veth pair gets a point-to-point `/30` from its slot index. The nftables table for that slot lives in the host namespace, is owned by root, and holds a default-deny chain on the host-side link, plus a set the bind fills and the release empties. The sandbox gets `/etc/hosts` with each name and its resolved address, and a `/etc/resolv.conf` naming no resolver. Port 53 and every address outside the set drop. An IPv6-only name, a set over the cap or any netlink failure refuses the lease, and the refusal names its reason. The boot sweep deletes every table and link carrying the runner's prefix before the first lease. The probe builds one scope in a scratch namespace and tears it down; only success reports `egress_enforcement: true`. **Implementation default:** a pure-Rust netlink crate that links into the static musl binary (PLAN names it with its licence and maintenance record), because `libnftnl` breaks the zero-`NEEDED` release check (M213_001 Dimension 1.1).

- **Dimension 2.1** — Merge and resolve: registry hosts plus fleet hosts, deduplicated in first-seen order, `read_only` fleet hosts excluded, an empty result admitting nothing → Test `test_egress_plan_merges_and_excludes_read_only`
- **Dimension 2.2** — The rendered `/etc/hosts` names every merged host and `/etc/resolv.conf` names no server → Test `test_egress_resolver_files_render`
- **Dimension 2.3** — On a real kernel, the sandbox connects to an allow-listed address and fails on a non-listed address, a link-local address and port 53 → Test `test_kernel_allow_list_admits_only_the_set`
- **Dimension 2.4** — The sandbox cannot widen its own rules: `nft flush ruleset` inside it changes nothing on the host → Test `test_kernel_sandbox_cannot_flush_host_rules`
- **Dimension 2.5** — IPv6-only name, set over the cap and netlink failure each refuse the lease with a named reason → Test `test_egress_setup_failures_refuse_the_lease`
- **Dimension 2.6** — Release empties the set before the slot is reused, and the boot sweep removes a crashed runner's tables and links → Test `test_egress_release_and_boot_sweep`
- **Dimension 2.7** — The probe reports `egress_enforcement: true` only after a scope builds and tears down, and the daemon then reads an `allow_list_egress` runner healthy → Test `test_egress_probe_reports_enforcement`

### §3 — The pages say what the runner does

`runner_fleet.md` §Egress model describes the Rust scope with its Rust names, keeping the dated decisions. `runner_execution.md` drops the non-goal "No runner crate builds the per-lease network allowlist". On M213_001's docs branch, `runners.mdx` stops saying allowlist egress is not enforced, and names the host requirement (nftables in the kernel) and what a degraded runner shows when the probe fails.

- **Dimension 3.1** — The architecture check passes and no page says allowlist egress is unenforced → Test `test_egress_pages_describe_enforcement`
- **Dimension 3.2** — The docs branch carries the runner page change, and its checks pass → Test `test_docs_branch_carries_egress_page`

## Interfaces

```
Unchanged API:   PATCH /v1/fleets/runners/{runner_id}  assigned_policy.{network_policy, registry_allowlist}
                 PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}  network.allow / read_only (TRIGGER.md)
Unchanged wire:  CapabilityReport.egress_enforcement (now measured) · ExecutionPolicy.network_policy
Sandbox engine:  launch takes the assigned NetworkPolicy; lease bind takes the merged, resolved allowlist
Host objects:    one nftables table and one veth pair per slot, both named with the runner's prefix and slot index
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Host lacks nftables or netlink permission | Kernel without `nf_tables`, or the unit drops the capability | Probe fails; `egress_enforcement: false`; the daemon reads an `allow_list_egress` runner degraded with `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE`; no lease runs open |
| Name does not resolve or resolves IPv6 only | Typo in `network.allow`, or a v6-only host | Lease refused with a reason naming the host; the event settles failed with that reason |
| Allowlist over the cap | Large `registry_allowlist` plus fleet hosts | Lease refused before any rule is installed |
| Netlink call fails mid-build | Kernel error or a race with the sweep | Partial objects torn down; lease refused; the sandbox never starts with a half-built scope |
| Runner crashes holding scopes | Kill or power loss | Boot sweep deletes the runner's tables and links before the first lease |
| Host IP of an allowed name rotates during a lease | CDN churn | The connection fails; the next lease resolves again (the name layer is out of scope) |
| Sandbox tries to edit rules | Hostile code inside | Its namespace holds no rules; the host table is unreachable from it (Dimension 2.4) |
| Assignment missing | Heartbeat reply without a policy | Fail-closed default; no egress opens |

## Invariants

1. A sandbox under `allow_list_egress` never runs before its scope is built; a build error refuses the lease — enforced by the lease bind returning the scope a launch requires as an argument.
2. Egress rules live only in the host namespace — enforced by the scope taking the host namespace handle and the kernel-lane test that flushes from inside.
3. A missing or unparsable assignment never opens egress — enforced by `FAIL_CLOSED_DEFAULT` and a unit test on the absent case.
4. `egress_enforcement` is `true` only after a measured probe — the constant leaves; the field is set from the probe result alone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `egress_scope_refused` log line | ops | A scope cannot be built and the lease is refused | reason, slot index, host count | no host names beyond the one that failed to resolve; no addresses | `test_egress_setup_failures_refuse_the_lease` |
| `egress_enforcement` in the capability report | ops | Every report | boolean | none needed | `test_egress_probe_reports_enforcement` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_bubblewrap_network_flag_per_policy` | `allow_all` → `--share-net` present; `deny_all_egress`, `allow_list_egress`, `None` → `--unshare-net`, no `--share-net` |
| 1.2 | unit | `test_reassignment_retires_warm_slots` | slots built under `allow_all`, assignment becomes `deny_all_egress` → no old slot is handed to a lease |
| 1.3 | integration (kernel lane) | `test_kernel_allow_all_and_deny_all` | host listener on a veth-less address: `allow_all` connects, `deny_all_egress` gets `ENETUNREACH` |
| 2.1 | unit | `test_egress_plan_merges_and_excludes_read_only` | `["a","b"]` + `["b","c"]` → `a,b,c`; same with `read_only` → `a,b`; empty both → empty set |
| 2.2 | unit | `test_egress_resolver_files_render` | `a→10.0.0.1` → hosts line `10.0.0.1 a`; `resolv.conf` has no `nameserver` |
| 2.3 | integration (kernel lane) | `test_kernel_allow_list_admits_only_the_set` | listener on an allowed address connects; non-listed, `169.254.169.254` and `:53` time out or are refused |
| 2.4 | integration (kernel lane) | `test_kernel_sandbox_cannot_flush_host_rules` | flush from inside → host table unchanged; the non-listed address still fails |
| 2.5 | unit | `test_egress_setup_failures_refuse_the_lease` | v6-only, cap + 1 and an injected netlink error → refusal with three distinct reasons, no objects left |
| 2.6 | integration (kernel lane) | `test_egress_release_and_boot_sweep` | after release the set is empty; prefixed table and link left by a killed run → gone after sweep |
| 2.7 | unit + integration | `test_egress_probe_reports_enforcement` | probe success → `true`; injected failure → `false`; daemon reconcile over the report → healthy vs degraded |
| 3.1 | unit | `test_egress_pages_describe_enforcement` | `scripts/check_architecture_doc.sh` exit 0; grep for "not enforced" over `docs/architecture` → 0 |
| 3.2 | manual | `test_docs_branch_carries_egress_page` | docs Pull Request link with checks green, pasted in Session Notes |
| 1.4 | integration | `test_runner_binary_runs_a_lease_per_policy` | M213_001's fake-daemon lease, once per policy → one settled report each, no `run_refused` line |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | All three policies hold on a real kernel (§1, §2) | `make test-runner-kernel` | exit 0, `egress` cases listed as passed | P0 | |
| R2 | The probe is measured, not pinned (§2) | `grep -rn 'egress_enforcement: false' rustd/crates/afr_supervisor/src --include='*.rs' \| grep -v tests \| wc -l` | `0` | P0 | |
| R3 | No shell-out to `nft` or `ip` (§2) | `grep -rnE 'Command::new\("(nft\|ip)"\)' rustd/crates \| wc -l` | `0` | P0 | |
| R4 | Pages describe enforcement (§3) | `bash scripts/check_architecture_doc.sh && ! grep -rqi 'allowlist egress is not enforced' docs/architecture` | exit 0 | P0 | |
| R5 | Dev runner assigned `allow_list_egress` reads healthy and runs a lease that installs a package from the registry baseline (§2) | manual — `agentsfleet runners list` output and the thread link in Session Notes | healthy row, settled thread | P0 | |
| R6 | Diff stays inside Files Changed (this spec and M213_001 together) | `git diff --name-only origin/main...HEAD` | 0 paths missing from the two tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted by this spec; `src/runner/network/` leaves with M213_001's §2.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| Pinned enforcement | `grep -rn 'egress_enforcement: false' rustd/crates/afr_supervisor/src --include='*.rs' \| grep -v tests` | 0 matches |
| The non-goal | `grep -n 'No runner crate builds the per-lease network allowlist' docs/architecture/runner_execution.md` | 0 matches |

## Out of Scope

- The name layer for rotating CDN addresses (DNS-answer snooping into the same set), per `runner_fleet.md` §Egress model — a later spec.
- IPv6 egress — refused, as the Zig design refused it.
- New API fields or a new allowlist surface — both knobs exist (Interfaces).
- Closing the allow-listed write-capable host as an exfiltration channel — a credential-model change.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator assigns `allow_list_egress` with `registry.npmjs.org` in the registry list; the runner shows healthy, a fleet's `npm install` works, and a `curl` to any other host inside the same sandbox fails.
2. **Preserved user behaviour** — Runners on `allow_all` keep reaching the network as they do on Zig; runners on `deny_all_egress` keep reaching nothing; both API calls keep their shapes.
3. **Optimal-way check** — Kernel rules at the host side of a veth are the narrowest enforcement that needs no proxy and no resolver; the gap to the optimum is the name layer, acceptable while allowlists are small and stable.
4. **Rebuild-vs-iterate** — Port of a finished design that was never switched on; no redesign.
5. **What we build** — The policy flag, the scope, the probe, the sweep, the two page changes.
6. **What we do NOT build** — See Out of Scope.
7. **Fit with existing features** — Composes with `afr_egress`, which still owns every credentialed call and every L7 rule; must not destabilise warm slots.
8. **Surface order** — API already exists; runner first, published page in the same window.
9. **Dashboard restraint** — N/A — no dashboard change; the runner list already shows degraded and its reason.
10. **Confused-user next step** — A degraded runner names `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE`; the runner page names the kernel requirement; a refused lease names the host that failed to resolve.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a separate spec in M213_001's Pull Request — the cutover would regress `allow_all` without §1, and the Zig reference leaves in the same diff.
- **Alternatives considered:** folding it into M213_001 (Indy's first answer, superseded the same day by "create a new spec"); a later Pull Request after the cutover (rejected: `allow_all` hosts would lose the network between the two).
- **Patch-vs-refactor verdict:** this is a **patch** because the API, wire and daemon already carry the policy; only the runner's enforcement is missing.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 06, 2026): "I think the allowlist also is managed via the api, just create a new spec and make it depend on M213 and they must ship in 1 PR". Source-verified: `PATCH /v1/fleets/runners/{runner_id}` takes `assigned_policy` (`public/openapi.json`); `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` takes `trigger_markdown` and `config_json`, whose `network` block `afd_gate/src/policy/shape.rs:18` reads. Agent choices recorded for review: `read_only` fleet hosts stay out of the kernel set; the inference endpoint leaves the merged set.
- **Metrics review** — No analytics or funnel playbook update required; the signals are an operator log line and an existing report field.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
