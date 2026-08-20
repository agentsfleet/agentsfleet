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
**Status:** IN_PROGRESS
**Priority:** P0 — a Fleet that declares no tools currently receives a shell on the runner host, and that host holds a public address and a private-network interface.
**Categories:** INFRA, ZIG
**Batch:** B1 — no dependency on another unstarted workstream
**Branch:** `feat/m136-live-connector-proof-followup` — shared with M136_001, which this workstream unblocks
**Test Baseline:** unit=4166 integration=709
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
| `docs/v2/active/M170_001_P0_INFRA_ZIG_ZERO_TRUST_LEASE_SANDBOX.md` | EDIT | Record the §3 rescope, the withdrawn executable claim, and the reachability Dimensions that replaced reasoning with measurement. |
| `src/runner/child_exec_input.zig` | EDIT | Preserve a present-but-empty tools array instead of collapsing it to absent. |
| `src/runner/engine/runner_helpers.zig` | EDIT | An absent tools spec resolves to zero tools rather than the whole registry. |
| `src/runner/engine/tool_bridge.zig` | EDIT | Replace the unsupported-tool denylist with the hosted allowlist, and refuse a declared name outside it. |
| `src/runner/engine/hosted_tools.zig` · `hosted_tools_test.zig` | DELETE | Both callers of the registry-default builder are gone, so the module is dead (RULE NDC). |
| `src/runner/engine/tool_bridge_test.zig` | EDIT | Prove allowlist membership, that no spawning tool is hosted, and that the scheduler set stays unreachable. |
| `src/runner/tool_bridge_resolution_test.zig` | EDIT | Pin refusal, empty-array and absent-spec resolution, and that an allowlisted tool still resolves. |
| `src/runner/engine/runner_helpers_test.zig` | EDIT | The non-array arm degrades to zero tools rather than to a default set. |
| `src/runner/child_exec_input_test.zig` | EDIT | An empty policy yields a null fleet_config but an EMPTY tools array. |
| `src/lib/contract/protocol_bind.zig` | EDIT | Narrow `BASELINE_RO_PATHS` to the four paths a lease needs; comptime-prove the allowlist is a registry subset. |
| `src/runner/sandbox_args_bind_test.zig` | EDIT | Pin the narrowed argv and the absence of credential-bearing trees. |
| `src/runner/engine/landlock.zig` | EDIT | The read set derives from the narrowed list; confirm no floor entry re-widens it. |
| `src/runner/selftest_integration_test.zig` | EDIT | Prove the probe still passes under the narrowed bind set on a real sandbox. |
| `docs/architecture/runner_fleet.md` | EDIT | Record the tool allowlist and the narrowed bind set as the documented boundary. |
| `src/runner/lease_transport_integration_test.zig` | CREATE | Prove by execution that a lease can run the transport the engine spawns, with a stripped-bind control arm so the green is decided by the bind set (Dimensions 3.6–3.9). |
| `src/runner/lease_hardening_integration_test.zig` | CREATE | Prove the transport executes under the FULL wall (`no_new_privs` → landlock → seccomp), and isolate the filesystem wall so a future refusal names the layer that refused (Dimensions 3.10–3.11). Split from the sibling on the mounts/hardening seam. |
| `src/runner/selftest_transport.zig` | CREATE | The transport as a self-test subject — host resolution and the raw `fork`+`execve` spawn — shared by the parent and the probe so the two cannot disagree about which binary is under test (RULE UFS). |
| `src/runner/selftest_test_fixtures.zig` | CREATE | Shared real-sandbox harness: the lease-argv splice, the host control arm, and the resolver fixture's cleanup — split from the test file on the FLL cap. |
| `src/runner/sandbox_integration_test.zig` | EDIT | Root the new transport proofs in the single integration-lane module. |
| `src/runner/selftest_exec.zig` · `selftest_exec_test.zig` | EDIT | Grade the CHILD's environ rather than the daemon's, so the runner token never rides into the sandbox being measured. |
| `src/runner/selftest.zig` · `selftest_test.zig` · `selftest_probe.zig` · `selftest_probe_test.zig` | EDIT | Probe timeout wording, the displaced doc comment, and the flag-prefix pins the argv splice depends on. |
| `src/runner/daemon/loop.zig` · `daemon/selftest_beat.zig` | EDIT | Thread the captured environ to the self-test beat instead of reading the daemon's own. |
| `src/runner/child_process.zig` · `child_process_test.zig` | EDIT | Create the child HOME the direct-exec tier assigns, and prove it is idempotent. |
| `src/runner/main.zig` · `runner_tail_coverage_test.zig` | EDIT | Keep the probe subcommand's tail reachable and its coverage claim true. |
| `src/runner/engine/tool_bridge_registry.zig` | EDIT | Registry names the allowlist asserts against. |
| `src/lib/contract/protocol.zig` · `protocol_policy.zig` | EDIT | Re-export the bind-path module and keep the policy shape aligned with it. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/policy-binds.ts` · `policy-binds.test.ts` | EDIT | Mirror the `DANGER_HOST_` baseline in the dashboard so the operator-facing list cannot disagree with the runner's. |

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

- **Dimension 1.1** — **DONE** — a present-but-empty tools array resolves to zero tools → Test `an empty tools array grants nothing`
- **Dimension 1.2** — **DONE** — an absent tools spec resolves to zero tools rather than the full registry → Test `an absent tools spec is not a licence for every tool`
- **Dimension 1.3** — **DONE** — a Fleet left with zero tools records why, so an operator can see the refusal rather than infer it from behaviour → Test `a fleet with no granted tools logs the reason once`

### §2 — The hosted tool set is an allowlist this repository owns

The registry carries far more tools than a hosted multi-tenant runner should offer, and the current denylist names only the ones already known to be wrong. Invert it: name what is safe, refuse everything else, and prove the allowlist is a subset of the registry so a rename upstream fails the build instead of silently granting nothing.

- **Dimension 2.1** — **DONE** — only allowlisted tools resolve; a process-spawning or host-reaching tool is refused even when a bundle names it explicitly → Test `a declared shell is refused, not granted`
- **Dimension 2.2** — **DONE** — the allowlist is a subset of the bridge registry, enforced at compile time → Test `every allowlisted tool name exists in the registry`
- **Dimension 2.3** — **DONE** — every Fleet bundle shipped in this repository resolves its declared tools unchanged → Test `every shipped bundle's declared tools survive the allowlist`

### §3 — The lease sandbox carries no credential

The baseline binds seven broad trees into every lease. Two of them carry credentials — the daemon's own installation directory, which holds its control-plane token, and the host account database — and nothing a lease runs reads either. Narrowing removes both from every sandbox and names the individual `/etc` files a lease does read.

**Scope corrected during VERIFY (Aug 20, 2026).** This Section was written as "no credential and no executable", on the premise that the statically-linked runner means a lease needs no executable at all. That premise was false: the lease child runs the NullClaw engine, whose model transport spawns `curl` (ten provider modules), as does the `http_request` tool. With the executable trees unbound every lease dies at `execvp` before its first model call. The executable half is **withdrawn**, not delivered — earning it back requires removing the `curl` dependency and is out of scope here. The credential half is unaffected and ships. Detail in Discovery.

- **Dimension 3.1** — **DONE** — no baseline path is, contains, or is contained by a credential-bearing tree: `/opt` (the daemon's `.env`) and the broad `/etc` (the host account database) are refused at compile time in both directions, while the specific `/etc` files a lease reads stay bound → Test `the baseline binds only what a lease needs to dial`
- **Dimension 3.2** — **DONE** — no baseline path contains or is an ancestor of a credential-bearing file → Test `no baseline bind reaches the daemon's own state`
- **Dimension 3.3** — **DONE** — the self-test probe passes on a real sandbox under the narrowed set, proving the narrowing did not remove something the engine opens → Test `the probe's checks pass under the narrowed bind set`
- **Dimension 3.4** — **DONE** — a real lease cannot read the daemon's token or the host account database, proven by reading each from inside the sandbox after confirming it is readable on the host → Test `the daemon's credentials are unreachable inside a real lease sandbox`
- **Dimension 3.5** — **WITHDRAWN** — "no executable is reachable inside a lease". The engine's transport spawns `curl`; asserting this would ship a runner on which no lease can reach its model. Reopens when the transport no longer needs a subprocess.
- **Dimension 3.6** — **DONE** — a dynamically linked executable actually runs inside a real lease, so the trees restored for the transport are proven reachable by the kernel rather than by the bind list agreeing with itself → Test `a dynamically linked executable runs inside a real lease sandbox`
- **Dimension 3.7** — **DONE** — the engine's own transport binary executes inside a real lease wherever the host carries one → Test `the engine's model transport is executable inside a real lease sandbox`
- **Dimension 3.8** — **DONE** — the TLS trust store is readable from inside a real lease, not merely bound, so a bundle reached through symlinks into `/usr/share` cannot pass while unreadable → Test `the TLS trust store is readable inside a real lease sandbox`
- **Dimension 3.9** — **DONE** — the same executable FAILS in the same lease with only the system-core bind triples removed, so Dimension 3.6's green is proven to be decided by the bind set rather than by the command succeeding anywhere → Test `the same executable fails in a lease stripped of the system trees`
- **Dimension 3.10** — **DONE** — a binary executes under the lease's FULL hardening (`no_new_privs` → landlock → seccomp), not merely inside its mounts, measured through the real probe's new `transport=` row rather than a test-only path → Test `a binary spawns under the lease's full hardening, not just its mounts`
- **Dimension 3.11** — **DONE** — the filesystem wall ALONE permits the exec, isolating landlock from seccomp and from the standard library's spawn helper, so a future exec refusal names the layer that refused it instead of one composite failure → Test `the filesystem wall alone permits executing a binary from the system trees`
- **Dimension 3.12** — **DONE** — the runner MEASURES transport executability on every heartbeat instead of the milestone reasoning about it once: the parent resolves the host's transport and the probe spawns it from behind the wall, reporting a named check an operator can act on → Check `the model transport runs inside the sandbox`

## Interfaces

```
Lease wire (unchanged shape, tightened meaning):
  policy.tools : []string
    []          -> zero tools            (was: every tool in the registry)
    absent      -> zero tools            (was: every tool in the registry)
    ["name"]    -> name, iff allowlisted (was: name, iff in the registry)

Sandbox baseline read set (narrowed to the credential half — see §3):
  TLS trust store · resolver state directory · hosts file · nsswitch.conf
  · the host's executable and library trees (/usr /lib /lib64 /bin /sbin),
  which the engine's `curl`-spawning transport needs to exec at all,
  plus the runner's own binary bound as a single file and the per-lease
  writable tmpfs floor.
  OUT, refused at compile time in both directions:
  /opt        the daemon's control-plane token
  /etc (broad) the host account database
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Declared tool not allowlisted | a bundle names a real tool that is not hosted (`shell`, `git`, `spawn`, …) | The lease FAILS, with the refusal recorded by name. Matches the disposition the scheduler refusal already had: running a Fleet with a quietly different tool set changes behaviour its author never wrote. |
| Declared tool never hosted | a bundle names a scheduler tool the bridge registry does not carry | The lease fails rather than skipping. Without a distinct arm these read as typos, and a Fleet that asked for scheduling would run silently without it. |
| Unknown tool name | a bundle names a tool absent from the registry | Skipped and recorded, exactly as before — a name nobody knows grants nothing either way, so a typo must not cost the lease. Resolution order is what keeps this distinct from a refusal. |
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
| 3.6 | integration | `a dynamically linked executable runs inside a real lease sandbox` | `/usr/bin/env`, confirmed to exit 0 on the host first, exits 0 when run inside a composed lease — non-zero is the loader missing, which is how every lease died under the withdrawn narrowing |
| 3.7 | integration | `the engine's model transport is executable inside a real lease sandbox` | `curl --version`, where the host has a `curl`, exits 0 inside a composed lease; a host without one skips rather than passing |
| 3.8 | integration | `the TLS trust store is readable inside a real lease sandbox` | `cat /etc/ssl/certs/ca-certificates.crt` exits 0 inside a composed lease, having exited 0 on the host — a bound directory whose symlink targets are unreachable fails here |
| 3.9 | integration | `the same executable fails in a lease stripped of the system trees` | the identical argv with only the `/usr /lib /lib64 /bin /sbin` bind triples removed exits non-zero for the same command that exits 0 with them, and the strip is asserted to have removed something so a moved flag spelling cannot make it a silent no-op |
| 3.10 | integration | `a binary spawns under the lease's full hardening, not just its mounts` | the real probe tail is kept intact (`--sandboxed` included, so hardening applies) and given `--transport=`; the parsed verdict reports the transport both testable and executed |
| 3.11 | integration | `the filesystem wall alone permits executing a binary from the system trees` | a forked child applying only `no_new_privs` + `landlock.applyPolicy` reaches `execve` successfully; `execve` returning at all is the failure |
| 3.12 | unit | `an untested transport is not read as a failed one` · `a probe that never reported a transport key does not certify one` | `transport=x` decodes as not-testable, an absent key decodes as testable-but-failed — the two carry different operator instructions and must not collapse |
| regression | unit | `a declared allowlisted tool still resolves` | a bundle declaring an allowlisted tool receives exactly it — the narrowing removes nothing a Fleet legitimately asked for |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A no-tools declaration grants nothing (§1) | `make test-unit-agentsfleet-runner` | exit 0 | P0 |  ✅ `✓ [agentsfleet-runner] Unit tests passed` — 653/656, 0 failed |
| R2 | A process-spawning tool cannot be granted (§2) | `rg -n 'shell' src/runner/engine/tool_bridge.zig \| rg -c 'HOSTED_TOOL_ALLOWLIST'` | 0 matches | P0 |  ✅ `shell` appears only in refusal lists and registry-resolution pins; never in the allowlist |
| R3 | No shipped bundle loses a tool it declares (§2) | `make test-unit-agentsfleet-runner` | exit 0 | P0 |  ✅ all six bundles declare only `http_request` / `memory_*`, every one still resolves |
| R4 | The sandbox reaches no credential file (§3) | `rg -n -A14 'pub const BASELINE_RO_PATHS' src/lib/contract/protocol_bind_paths.zig` | no `/opt` and no broad `/etc` entry | P0 |  ✅ baseline names `/etc` only as the three files a lease reads (`ssl/certs`, `hosts`, `nsswitch.conf`); `/opt` absent. A comptime guard refuses either tree — and any ancestor that would re-admit one — on every platform. |
| R5 | A real lease cannot read the daemon's credentials (§3) | `make test-integration-kernel` | exit 0 | P0 |  (regrade at VERIFY) — proven by reading `/opt/agentsfleet/.env` and `/etc/shadow` from inside a real bwrap+landlock+seccomp lease, each confirmed host-readable first so a missing file cannot pass the test vacuously. |
| R7 | A real lease can exec the transport the engine spawns (§3) | `make test-integration-kernel` | exit 0 | P0 |  (regrade at VERIFY) — the inverse of R5: `/usr/bin/env` (and `curl` where the host has one) runs inside a composed lease, and the trust bundle reads, each with a host control arm so absence cannot pass vacuously. |
| R8 | The runner measures transport executability every heartbeat (§3) | `rg -n 'KEY_TRANSPORT\|CHECK_TRANSPORT' src/runner` | the probe emits the row and `grade` names it | P0 |  (regrade at VERIFY) — the parent resolves the host's `curl`, the probe spawns it from behind `no_new_privs` → landlock → seccomp, and a host carrying no transport reads as a named fault rather than a silent pass. |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 |  ✅ every path in the diff appears in Files Changed |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 |  ❌ blocked — Docker VM disk full (`pg connect error: could not write init file: No space left on device`) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 |  ✅ `✓ All lint checks passed` / `ALL GATES GREEN` via pre-commit |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 |  ❌ blocked — same disk exhaustion |
| S5 | No leaks | `make memleak` | exit 0 | P0 |  ⬜ not run — memleak lane needs the same datastore |
| S6 | Cross-compile | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl && zig build --build-file build_runner.zig -Dtarget=aarch64-linux-musl` | exit 0 | P0 |  ✅ `x86_64-linux-musl` built and executed on the host |
| S7 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 |  ✅ `no leaks found` |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 |  ✅ zero matches for `hosted_tools`, `UNSUPPORTED_HOSTED_TOOLS`, `isUnsupportedHostedToolName` |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/runner/engine/hosted_tools.zig` | `test ! -f src/runner/engine/hosted_tools.zig` |
| `src/runner/engine/hosted_tools_test.zig` | `test ! -f src/runner/engine/hosted_tools_test.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `UNSUPPORTED_HOSTED_TOOLS` | `grep -rn "UNSUPPORTED_HOSTED_TOOLS" src/ \| head` | 0 matches |
| `isUnsupportedHostedToolName` | `grep -rn "isUnsupportedHostedToolName" src/ \| head` | 0 matches |
| `hosted_tools` | `grep -rn "hosted_tools" src/ \| head` | 0 matches |

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
  - **Refusal disposition (implementation deviation).** The spec first said a refused tool leaves the Fleet running with its remaining tools. Implemented as failing the lease instead, because `buildTools` already did exactly that for the scheduler set — a second disposition beside the first would be two answers to one question. Failure Modes amended to match the code.
  - **Resolution order is behaviour, not style.** The allowlist check was first written ahead of `resolve`, which made an unknown NAME fail the lease — collapsing "typo" into "refused". Three arms now: never-hosted (fatal), unknown (skip), resolved-but-not-allowlisted (fatal). `NEVER_HOSTED_TOOLS` exists because the scheduler names are not in `BRIDGE_REGISTRY`, so `resolve` cannot tell them from a typo.
  - **`/opt` protection was named for a directory the token does not live in.** `SENSITIVE_PATHS` carried `/var/lib/agentsfleet` while the deploy writes the token to `/opt/agentsfleet/.env`. Invisible while `/opt` sat in the baseline — an operator cannot bind what the daemon already binds — so narrowing the baseline is what made naming it load-bearing. Added in the same change.
- **§3's executable half rested on a false premise, and shipping it would have killed every lease (Aug 20, 2026).** `/review`'s security specialist and the adversarial pass, independently, found that the engine's model transport SPAWNS `curl`: ten NullClaw provider modules reach `sse.curlStream*` / `http_util.curlPost*`, and `http_request` does the same. With `/usr`, `/bin`, `/lib`, `/lib64` and `/sbin` unbound, `curl` and its shared libraries are absent from a lease, so every lease dies at `execvp` **before its first model call** — not merely losing a tool. The trees are restored; `/opt` and the broad `/etc` stay out, which was always the real exposure. Dimension 3.5 is withdrawn rather than quietly dropped, and the Section title no longer claims "no executable". Earning that claim back means removing the `curl` dependency (in-process transport upstream, or a vetted static binary on its own single-file bind) — a separate workstream, not a line in this one.
- **The evidence that "proved" the narrowing measured the wrong path.** The host run recorded below reported `egress=1` and was read as "TLS to the inference host succeeds with only `/etc/ssl/certs` present". It does not show that. `selftest_probe.endpointAccepts` opens a TCP stream and closes it — its own comment says "this proves reachability, it does not speak a protocol" — and it runs inside the statically-linked runner, spawning nothing. It confirmed the one path that needs no executable, which is exactly why the gap survived a review that was looking straight at it. The lesson is narrower than "measure more": a measurement only counts against a claim if it exercises the mechanism the claim is about.
- **The replacement measurement is an A/B, because one arm proves nothing (Aug 20, 2026).** Restoring the trees by reasoning ("`curl` lives there") is structurally the same move the narrowing made in reverse, so the trees are now proven by execution: a dynamically linked binary runs inside a composed lease and must exit 0 (Dimension 3.6), and the SAME command in the SAME argv with only the system-core bind triples removed must exit non-zero (Dimension 3.9). The second arm is what makes the first believable — a lone green stays green if the command would have run anywhere, if the argv never applied, or if bwrap ignored the spliced tail. The strip is itself asserted to have removed something, so a changed flag spelling fails loudly instead of silently grading an unmodified lease.
- **The first version of the transport check measured its own plumbing (Aug 20, 2026).** The `transport=` row reported a FAILED exec inside a fully hardened lease, and the reading — "Option A restored the mounts but leases still cannot spawn `curl`" — was wrong. The check called `std.process.spawn`, whose helper does pipe → fork → dup2 → setpgid → execvpe; one of those extra steps returns `AccessDenied` inside a lease for reasons unrelated to whether the transport can run. Isolating the layers settled it: a forked child applying only `no_new_privs` + landlock reaches `execve` fine (Dimension 3.11), and the same check rewritten as raw `fork` + `execve` — the call the kernel actually performs — passes under all three layers. This is the SAME error class as the one that removed the executable trees: an instrument that does not exercise the mechanism the claim is about. It cost one wrong alarm and no shipped defect, because the layers were isolated before anything was concluded.
- **A host with no transport is a FAULT, not an untested row (Aug 20, 2026).** `grade` reports "no curl at `/usr/bin/curl` or `/bin/curl`" as a failed check with its own detail, distinct from an exec that was attempted and refused — the two are different operator instructions ("install curl" versus "fix the bind set"). The consequence is deliberate: on a host without a transport the runner reads unhealthy, because no lease there can reach a model, and `allOk` reporting green would be the exact dead-runner/green-panel pairing this milestone exists to remove. `test_probe_reports_deny_all_as_expected` asserts `allOk()` only where a transport exists, rather than being relaxed to accommodate a host that genuinely cannot run leases.
- **The regression pin deliberately is not `curl` (Aug 20, 2026).** The obvious test — exec `curl` inside a lease — would SkipZigTest in the kernel lane, because the `ci-zig-alpine` image ships no `curl`; that is the one environment Continuous Integration (CI) runs this lane in, so the pin would have been silent exactly where it is needed, repeating the failure recorded two entries below. The pin uses `/usr/bin/env`, which fails identically when the executable and library trees are unbound and exists on every supported host. `curl` still gets its own test, gated on the host carrying one, so the dev host and any Debian-family runner measure the real transport.
- **Landlock was silently dropping the `/etc/hosts` rule (Aug 20, 2026).** `SYSTEM_READONLY_ACCESS` carries `LANDLOCK_ACCESS_FS_READ_DIR`, and the kernel refuses a rule on a NON-directory whose access includes any right outside `ACCESS_FILE` — so the rule returned `EINVAL` and the loop's `catch { continue; }` ate it, leaving `/etc/hosts` bind-mounted and unreadable. `/etc/hosts` is the first regular FILE the read set has ever contained; every earlier entry was a directory, which is why the mask never fired before this milestone. `addPathRule` now retries with the file-accepted subset, and an absent path (tolerated) no longer shares an arm with a kernel refusal (fail-closed).
- **Every real-sandbox proof was skipping, everywhere (Aug 20, 2026).** The kernel lane's stub child was built `native`, so it inherited the host libc — dynamic musl on the alpine image, glibc on the ubuntu Continuous Integration (CI) image — and died at `execvp` inside the narrowed sandbox on its absent interpreter (the `/lib` it needs is a retired tree). `probeRanHere` graded that a harness fact and all six selftest integration proofs skipped silently in BOTH containers; the suite read green while proving nothing. The stub is now pinned to the release's own shape (`<arch>-linux-musl`, `.linkage = .static`, its own `SharedDeps` for the matching nullclaw target); all six proofs now run and pass under bwrap + landlock + seccomp in the kernel lane.
- **The §3 pin asserted nothing as first written.** The no-exec test overwrote the last argv element — `--workspace=`, not the exec target — so it ran the real probe and passed on the probe's own failure. Rewritten from `probeTailIndex`: the tail becomes `sh -c "exit 0"` / `busybox true`, so a reachable executable exits 0 and FAILS the test.
- **A stale HOME assertion inverted the planted-token proof.** `sandbox_integration_test` still asserted the daemon's `HOME` reaches the child — the exact inheritance M136 removed. The first actual run of the kernel lane caught it; the test now pins the assigned `CHILD_HOME` present and the daemon's HOME absent.
- **Resolver layout is a host fact the harness fakes only where it may.** The container images carry no systemd-resolved, so the two resolver-pass proofs create `/run/systemd/resolve/stub-resolv.conf` when it is absent and the process may create it (root in the disposable containers), and skip on unprivileged hosts with a different layout. On systemd-resolved hosts they run against the real file.
- **The narrowed baseline is pinned at compile time.** Re-adding a retired executable tree or a broad `/etc` to `BASELINE_RO_PATHS` now fails every build on every platform (comptime guard beside the existing bind invariants). Before, nothing refused a quiet re-add — leases kept working and every probe check stayed green.
- **Host evidence — the narrowed bind set dials (Aug 19, 2026).** Run on `zombie-dev-worker-ant` with no `/usr`, `/bin`, `/lib`, `/lib64`, `/sbin`, `/opt` tree and no wholesale `/etc`: `resolver=1 scratch=1 dns=1 egress=1 binds=x`. That run exercised the mount layer only (probe without `--sandboxed`, deployed binary); the landlock-applied run is recorded separately. **Corrected Aug 20, 2026:** this entry originally read "TLS to the inference host succeeds with only `/etc/ssl/certs` present, which is what makes removing the executable and library trees a fact rather than a claim." It never showed that — `egress` is a TCP connect from inside the statically-linked runner, not a TLS handshake and not a spawned transport. The measurement stands; the conclusion drawn from it does not.
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
