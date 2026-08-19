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

# M170_001: Zero-trust lease sandbox

**Prototype:** v2.0.0
**Milestone:** M170
**Workstream:** 001
**Date:** Aug 19, 2026
**Status:** PENDING
**Priority:** P0 — a Fleet that declares no tools currently receives a shell on the runner host, and that host holds a public address and a private-network interface.
**Categories:** INFRA, ZIG
**Batch:** B1 — no dependency on another unstarted workstream
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M136_001 (the child's `HOME` must resolve inside its own sandbox; until that lands every lease fails on `HOME` regardless of this spec)
**Provenance:** agent-generated during M136_001's live-proof investigation, Aug 19, 2026
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Process boundary

---

## Overview

**Goal (testable):** A Fleet declaring `tools: []` executes with zero tools, no hosted Fleet can obtain a process-spawning tool even by naming one, and a lease sandbox contains no credential file and no executable but the runner's own statically linked binary.

**Problem:** An operator who writes `tools: []` believing it locks a Fleet down instead gets the most capable Fleet the platform can produce — one that can run arbitrary commands on the runner host. That host carries a public address and an interface onto the operator's private network, so a Fleet reached by prompt injection can read host files and reach other machines. The operator's declaration currently widens access instead of narrowing it, and nothing in the product tells them.

**Solution summary:** Three fail-closed changes at the runner boundary. A tools declaration becomes subtractive: a present-but-empty list and an absent list both resolve to zero tools instead of the full registry. The hosted tool set becomes an allowlist this repository owns, so a process-spawning or host-reaching tool cannot be granted even when a bundle names it, and a tool added upstream does not enrol itself. The baseline bind set narrows to the four paths a lease actually needs — trust store, resolver, hosts file, resolver state — which removes every credential file and every executable from the sandbox. The runner binary is statically linked, so nothing is lost by removing the library and executable trees.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(runner): fail-closed tool resolution and a minimal lease bind set
- **Intent (one sentence):** An operator can trust that a Fleet receives only the tools it names, and that a lease sandbox holds no credential and no executable it does not need.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/engine/tool_bridge.zig` — the bridge registry and the denylist this spec inverts; the registry names are the vocabulary the allowlist is written against.
2. `src/lib/contract/protocol_bind.zig` — the bind lists, the two-sided validation principle, and the recorded lesson that a denylist "fails open on everything unlisted and goes stale". That reasoning is what §2 carries across to tools.
3. `src/runner/selftest.zig` and `src/runner/selftest_probe.zig` — the instrument that proves a narrowed bind set on a real host; §3 is validated by tightening and reading the probe's verdict, not by assertion.
4. `src/runner/child_exec_input.zig` — where the lease payload becomes the child's arguments, and where an empty tools array is currently lost.
5. `docs/architecture/runner_fleet.md` §Process boundary — the documented claims about what the child inherits, which this spec must leave true.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/child_exec_input.zig` | EDIT | Preserve a present-but-empty tools array instead of collapsing it to absent. |
| `src/runner/engine/runner_helpers.zig` | EDIT | An absent tools spec resolves to zero tools rather than the whole registry. |
| `src/runner/engine/tool_bridge.zig` | EDIT | Replace the unsupported-tool denylist with the hosted allowlist, and refuse a declared name outside it. |
| `src/runner/engine/hosted_tools.zig` | EDIT | The default builder consumes the allowlist rather than the full registry. |
| `src/runner/engine/{tool_bridge,hosted_tools}_test.zig` | EDIT | Prove refusal by name, allowlist membership, and that every shipped bundle still resolves. |
| `src/runner/child_exec_input_test.zig` | EDIT | Pin empty-array and absent-key resolution separately. |
| `src/lib/contract/protocol_bind.zig` | EDIT | Narrow `BASELINE_RO_PATHS` to the four paths a lease needs; comptime-prove the allowlist is a registry subset. |
| `src/runner/sandbox_args_bind_test.zig` | EDIT | Pin the narrowed argv and the absence of credential-bearing trees. |
| `src/runner/engine/landlock.zig` | EDIT | The read set derives from the narrowed list; confirm no floor entry re-widens it. |
| `src/runner/selftest_integration_test.zig` | EDIT | Prove the probe still passes under the narrowed bind set on a real sandbox. |
| `docs/architecture/runner_fleet.md` | EDIT | Record the tool allowlist and the narrowed bind set as the documented boundary. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (the allowlist and the bind list are each single-sourced and shared by both enforcement layers), NDC (the denylist is deleted, not left beside its replacement), NLR (the files touched get their stale comments corrected), ORP (no orphaned reference to the removed denylist survives), FLL (touched runner files stay under the file cap).
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — pub-surface discipline for the new allowlist constant, comptime assertion style, and the cross-compile check both Linux targets need.
- **`~/Projects/dotfiles/dispatch/write_documentation.md`** — the architecture page states the boundary in operator terms, not in constant names.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — runner and shared-protocol modules change | cross-compile both Linux targets; the runner's own build graph runs green |
| PUB / Struct-Shape | yes — one new pub allowlist constant | justified against two consumers (`tool_bridge`, `hosted_tools`); no other new pub surface |
| File & Function Length (≤350/≤50/≤70) | yes — `tool_bridge.zig` grows an allowlist and a refusal path | the allowlist is data, not branches; split the registry table out if the file approaches the cap |
| UFS (repeated/semantic literals) | yes — tool names and bind paths repeat across two layers | one named list per concern, consumed by both layers, as `BASELINE_RW_TMPFS` already is |
| LOGGING | yes — a refused tool is an operator-visible event | reuse the existing `tool_skipped` vocabulary with a distinct refusal reason |
| UI / DESIGN TOKEN / SCHEMA / ERROR REGISTRY | no — no dashboard, schema, or new error code | N/A |

## Prior-Art / Reference Implementations

- **Reference:** `src/lib/contract/protocol_bind.zig` — the shared bind list already solves this exact shape: one list, consumed by both the mount layer and the policy layer, with a comptime assertion tying them together and a recorded rationale for why a denylist was rejected. The tool allowlist mirrors that structure rather than inventing a second idiom.

## Sections (implementation slices)

### §1 — A tools declaration can only narrow

Today an empty declaration is indistinguishable from an absent one, and an absent one means "give it everything". Both must mean zero. **Implementation default:** carry the distinction in the type that crosses into the child rather than re-deriving it, because the loss happens at exactly one conversion point.

- **Dimension 1.1** — a present-but-empty tools array resolves to zero tools → Test `an empty tools array grants nothing`
- **Dimension 1.2** — an absent tools spec resolves to zero tools rather than the full registry → Test `an absent tools spec is not a licence for every tool`
- **Dimension 1.3** — a Fleet left with zero tools records why, so an operator can see the refusal rather than infer it from behaviour → Test `a fleet with no granted tools logs the reason once`

### §2 — The hosted tool set is an allowlist this repository owns

The registry carries far more tools than a hosted multi-tenant runner should offer, and the current denylist names only the ones already known to be wrong. Invert it: name what is safe, refuse everything else, and prove the allowlist is a subset of the registry so a rename upstream fails the build instead of silently granting nothing.

- **Dimension 2.1** — only allowlisted tools resolve; a process-spawning or host-reaching tool is refused even when a bundle names it explicitly → Test `a declared shell is refused, not granted`
- **Dimension 2.2** — the allowlist is a subset of the bridge registry, enforced at compile time → Test `every allowlisted tool name exists in the registry`
- **Dimension 2.3** — every Fleet bundle shipped in this repository resolves its declared tools unchanged → Test `every shipped bundle's declared tools survive the allowlist`

### §3 — The lease sandbox carries no credential and no executable

The baseline binds six broad trees into every lease. The runner binary is statically linked and is bound separately as a single file, so the library and executable trees serve nothing once §2 removes the process-spawning tools. Narrowing removes the daemon's own installation directory — which holds its control-plane token — and the host account database, from every sandbox.

- **Dimension 3.1** — the baseline read set is the trust store, the resolver file, the hosts file, and the resolver state directory, and nothing else → Test `the baseline binds only what a lease needs to dial`
- **Dimension 3.2** — no baseline path contains or is an ancestor of a credential-bearing file → Test `no baseline bind reaches the daemon's own state`
- **Dimension 3.3** — the self-test probe passes on a real sandbox under the narrowed set, proving the narrowing did not remove something the engine opens → Test `the probe's checks pass under the narrowed bind set`

## Interfaces

```
Lease wire (unchanged shape, tightened meaning):
  policy.tools : []string
    []          -> zero tools            (was: every tool in the registry)
    absent      -> zero tools            (was: every tool in the registry)
    ["name"]    -> name, iff allowlisted (was: name, iff in the registry)

Sandbox baseline read set (narrowed):
  TLS trust store · resolver file · hosts file · resolver state directory
  plus the runner's own binary, bound as a single file, and the per-lease
  writable tmpfs floor. No library, executable, or installation tree.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Declared tool not allowlisted | a bundle names a process-spawning or host-reaching tool | The tool is refused and recorded by name; the Fleet runs with its remaining allowlisted tools rather than failing the lease. |
| Every declared tool refused | a bundle names only non-allowlisted tools | The Fleet runs with zero tools and the refusal is recorded, so the cause is readable without host access. |
| Unknown tool name | a bundle names a tool absent from the registry | Existing skip-and-record behaviour is preserved; an unknown name is not silently equivalent to an allowlisted one. |
| Allowlist drifts from the registry | an upstream rename removes a name the allowlist carries | The build fails at the comptime subset assertion rather than granting an empty set at runtime. |
| Engine opens a path the narrowed set removed | a runtime dependency nobody enumerated | The self-test probe's check for that surface fails on the host and the runner reports degraded, rather than every lease failing opaquely. |
| Bundle depends on the removed default | an existing Fleet relied on the full-registry fallback | No shipped bundle does; the case is proven absent by Dimension 2.3 before the fallback is removed. |

## Invariants

1. The hosted allowlist is a subset of the bridge registry — comptime assertion in the same file as the allowlist, so a drift fails the build.
2. No tool reachable by a hosted Fleet spawns a process or reaches the host outside its declared network allowance — enforced by the allowlist being explicit and by a test that names every entry.
3. A tools declaration can only subtract from the hosted set — enforced by resolution taking the intersection, never a fallback union, with both the empty and absent cases pinned by test.
4. No baseline bind path is, or contains, a credential-bearing file — enforced by a test asserting the daemon's own state directory and the host account database are unreachable from a composed sandbox argv.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_refused` | ops | a declared tool is not on the hosted allowlist | tool name, fleet id, refusal reason | no lease payload, prompt text, secret value, or resolved credential | `a declared shell is refused, not granted` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `an empty tools array grants nothing` | a lease carrying `tools: []` yields a zero-length tool set, not the registry default |
| 1.2 | unit | `an absent tools spec is not a licence for every tool` | a lease with no tools key yields a zero-length tool set |
| 1.3 | unit | `a fleet with no granted tools logs the reason once` | zero granted tools emits one refusal record naming the cause, not one per candidate |
| 2.1 | unit | `a declared shell is refused, not granted` | a bundle declaring a process-spawning tool resolves without it and records the refusal by name |
| 2.2 | unit | `every allowlisted tool name exists in the registry` | each allowlist entry matches a registry name; a fabricated entry fails the build |
| 2.3 | unit | `every shipped bundle's declared tools survive the allowlist` | each bundle under `tests/fixtures/fleetbundle/` resolves the same tool set it declares |
| 3.1 | unit | `the baseline binds only what a lease needs to dial` | the composed argv carries the four baseline paths and none of the library, executable, or installation trees |
| 3.2 | unit | `no baseline bind reaches the daemon's own state` | no baseline entry equals or contains the daemon state directory or the host account database |
| 3.3 | integration | `the probe's checks pass under the narrowed bind set` | on a real bubblewrap sandbox the resolver, scratch, home, and egress checks all pass with the narrowed baseline |
| regression | unit | `a declared allowlisted tool still resolves` | a bundle declaring an allowlisted tool receives exactly it — the narrowing removes nothing a Fleet legitimately asked for |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A no-tools declaration grants nothing (§1) | `make test-unit-agentsfleet-runner` | exit 0 | P0 | |
| R2 | A process-spawning tool cannot be granted (§2) | `rg -n 'shell' src/runner/engine/tool_bridge.zig \| rg -c 'HOSTED_TOOL_ALLOWLIST'` | 0 matches | P0 | |
| R3 | No shipped bundle loses a tool it declares (§2) | `make test-unit-agentsfleet-runner` | exit 0 | P0 | |
| R4 | The sandbox reaches no credential file (§3) | `rg -n 'BASELINE_RO_PATHS' -A3 src/lib/contract/protocol_bind.zig` | no `/opt`, `/etc`, `/usr`, `/bin`, `/sbin`, `/lib` entry | P0 | |
| R5 | The probe still passes on a real host (§3) | `make test-integration` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl && zig build --build-file build_runner.zig -Dtarget=aarch64-linux-musl` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `UNSUPPORTED_HOSTED_TOOLS` | `grep -rn "UNSUPPORTED_HOSTED_TOOLS" src/ \| head` | 0 matches |
| `isUnsupportedHostedToolName` | `grep -rn "isUnsupportedHostedToolName" src/ \| head` | 0 matches |

## Out of Scope

- Dropping the child's capabilities — `child_exec.zig` records this as future work, and it is the durable fix for the residual memory-inspection path. Its own spec.
- Building `allow_list_egress` — the interim `allow_all` posture is what turns any escape into host-network reach. Designed, unbuilt, far larger than this change.
- Control-plane validation of a bundle's tools list at install — the bind list's two-sided principle applied to tools. Worth doing, separate surface and test matrix.
- Changing which tools exist in the vendored engine — this spec governs what a hosted Fleet may reach, not what the engine ships.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator writes `tools: []` on a Fleet, installs it, and it can do nothing at all. What they wrote is what they got.
2. **Preserved user behaviour** — every Fleet that names its tools keeps exactly those tools; installs, grants, and lease execution are unchanged. All six bundles in this repository are unaffected.
3. **Optimal-way check** — the direct path is refusing at resolution, where the declaration is already being read. The unconstrained-optimal shape validates on both sides, control plane and runner, as the bind lists already do; only the runner side lands here because it is the side that can be bypassed by a hand-built bundle.
4. **Rebuild-vs-iterate** — iterate. The resolution path and the bind lists are sound structures with wrong defaults; the fix is inverting two of them and shortening a third.
5. **What we build** — subtractive tool resolution, an owned hosted allowlist with a comptime subset proof, and a four-path baseline bind set.
6. **What we do NOT build** — capability dropping, kernel-enforced egress, control-plane tool validation, or any change to the engine's own tool implementations.
7. **Fit with existing features** — compounds with the network allowance and the grant resolver, which already bound what a Fleet may reach. It must not destabilise lease execution for the six shipped bundles, which Dimension 2.3 proves before the fallback is removed.
8. **Surface order** — no user surface; the change is entirely at the runner boundary and is observed through the self-test probe and the refusal record.
9. **Dashboard restraint** — N/A — no user surface. The refusal record is operator-facing through existing logging, and no dashboard claim is added.
10. **Confused-user next step** — an operator whose Fleet has no tools reads the refusal record naming the tool and the reason, rather than inferring it from a Fleet that answers but never acts.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections in one workstream because they are one boundary. Fixing resolution without the allowlist still lets an explicit declaration reach a shell; fixing the allowlist without the bind set still leaves the daemon's token one deploy-user change away from readable; narrowing binds without the tool set breaks nothing but protects nothing either.
- **Alternatives considered:** shipping only the resolution fix was rejected because it leaves the widest path open for the smallest saving. Folding this into M136_001 was rejected because that workstream is a live-proof spec whose Files Changed table never anticipated a security boundary change, and mixing them makes both harder to review.
- **Patch-vs-refactor verdict:** this is a **patch** because every structure it touches is already correct in shape — a list consumed by two layers, a resolution function, a registry. Only the defaults and the direction of the lists are wrong.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
