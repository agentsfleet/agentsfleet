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
# M143_004: Changed Zig branches are provably exercised

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 004
**Date:** Jul 25, 2026
**Status:** PENDING
**Priority:** P2 — a coverage instrument, valuable across every backend change but blocking none of them
**Categories:** INFRA
**Batch:** B4 — tooling, consumed by later workstreams rather than consuming them
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none
**Provenance:** split out of M143_003 §3 during CTO spec review (Jul 25, 2026); original draft LLM-authored (Codex via Amp, Jul 24, 2026)
**Canonical architecture:** `docs/architecture/observability.md` §Evidence

---

## Overview

**Goal (testable):** A reviewer can tell, from a committed artifact, which branches introduced by a diff were executed by that diff's tests and which were not.

**Problem:** Zig 0.16 emits no native source branch mapping, so the repository has line coverage and no way to know whether a new `catch`, `orelse`, or `switch` prong ever ran. A diff can add error handling that no test touches and still show healthy coverage.

**Solution summary:** Enumerate the executable edges a diff introduces with a pinned parser, mark the ones tests reach with a probe that compiles to nothing in production, and publish the ratio as a JSON artifact with a floor.

**Why this is its own workstream.** It was drafted as §3 of M143_003. It is a parser, an identity scheme, a probe lane, a manifest checker, and a threshold — an instrument, not evidence about the library. Left inside M143_003 it would take an agent's whole branch and starve the performance work it was meant to support. Nothing in M143_001, M143_002, or M143_003 waits on it.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(infra): prove changed Zig branches are exercised
- **Intent (one sentence):** A diff that adds an untested error path is visible before merge rather than after an incident.
- **Handshake** — at PLAN, restate the Intent and assumptions; mismatch means STOP before edits.

## Implementing agent — read these first

1. `make/test-unit.mk` and `make/quality.mk` — how existing lanes are declared and gated.
2. `src/agentsfleetd/tests.zig` — test-root reachability, which this instrument must not disturb.
3. `docs/greptile-learnings/RULES.md` — NDC and NLR, which govern probes left behind in production code.
4. `dispatch/write_zig.md` — comptime discipline for the no-op production path.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/agentsfleetd/testing/branch_probe.zig` | CREATE | Test-only edge marker; comptime no-op outside test builds. |
| `scripts/enumerate-changed-zig-branches.ts` | CREATE | Parse the diff, emit the enumerated-edge manifest. |
| `scripts/check-changed-backend-branches.ts` | CREATE | Compare executed against enumerated, write the artifact, enforce the floor. |
| `scripts/changed-branch-identity.ts` | CREATE | Content-addressed edge identity shared by both scripts. |
| `make/quality.mk` | EDIT | `coverage-changed-backend-branches` target with a single caller. |
| `package.json`; `bun.lock` | EDIT | Pin the parser package and version. |
| `scripts/check-changed-backend-branches.test.ts`; `scripts/changed-branch-identity.test.ts` | CREATE | Checker and identity unit tests. |
| `docs/architecture/observability.md` | EDIT | Record what the artifact means and what it does not. |

**Scope grading.** Rubric R3 compares `git diff --name-only origin/main` against this table, so every cell is an exact path. A path that turns out to be genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, NLR, NLG, UFS, FLL, ORP.
- **`dispatch/write_zig.md`, `dispatch/write_ts_adhere_bun.md`, `dispatch/write_any.md`** — comptime shape, Bun/TS discipline, named constants.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| ZIG GATE / PUB | yes | probe is one small module; production path is comptime-empty; both Linux targets build |
| File & Function Length | yes | parser, identity, and checker are separate scripts |
| UFS | yes | constants for the floor, artifact path, and edge kinds |
| UI Substitution / DESIGN TOKEN | no | no UI |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no runtime surface, no schema, no user-facing error |

## Prior-Art / Reference Implementations

- **Lane shape:** existing `make/quality.mk` gates that read a diff and fail on a threshold.
- **Artifact shape:** `test-results/` JSON already produced by other lanes.
- **What is deliberately not reused:** kcov line coverage, which cannot distinguish an executed line from an executed edge.

## Sections (implementation slices)

### §1 — Edge identity that survives reformatting

The identity of an edge is **not** `file:line`. A line number shifts when an unrelated import is added above it, which would invalidate a manifest for a diff that never touched the branch, and would let a stale manifest silently pass.

Identity is `sha256(normalized_enclosing_function_path + edge_kind + ordinal_within_function)` truncated to 16 hex characters, where `edge_kind` is one of `if_then`, `if_else`, `switch_prong`, `catch`, `orelse`, and `ordinal_within_function` counts edges of that kind in source order inside the enclosing function. Reformatting, comment changes, and unrelated edits above the function leave identity unchanged. Renaming the function or reordering its branches changes it, which is correct: that is a different edge.

- **Dimension 1.1** — identity is stable under reformatting and unrelated edits → Test `test_branch_identity_is_reformat_stable`
- **Dimension 1.2** — identity distinguishes genuinely different edges → Test `test_branch_identity_separates_distinct_edges`

### §2 — Enumeration from the diff

A pinned parser package and version, recorded in the root lockfile, parses each changed `.zig` file at `HEAD` and enumerates executable edges inside changed hunks only. Unchanged edges in a changed file are out of scope: the instrument measures what a diff introduces, not the repository's history.

Enumeration is reproducible: the same diff and the same pinned parser produce a byte-identical manifest. A parse failure fails the lane loudly rather than emitting a short manifest, because a silently empty enumeration reads as perfect coverage.

- **Dimension 2.1** — enumeration covers exactly the changed hunks and is byte-reproducible → Test `test_changed_edge_enumeration_is_reproducible`
- **Dimension 2.2** — a parse failure fails loudly and never yields an empty pass → Test `test_parse_failure_does_not_pass_silently`

### §3 — Probing without taxing production

`branch_probe.zig` exposes `mark(comptime id: []const u8) void`. Outside a test build it is comptime-empty and must leave no instruction and no symbol in the binary. Inside a test build it records the id into a process-local set the checker reads.

Probes are **advisory, not mandatory**. The checker reports enumerated edges with no probe as uncovered; it does not reject the build for a missing probe. This is a deliberate reversal of the original draft, which rejected any changed edge absent from the manifest. That rule makes every added branch a build break until hand-instrumented, which turns a measuring instrument into a tax on ordinary work and pressures agents to avoid adding error handling.

- **Dimension 3.1** — production builds carry no probe artifact → Test `test_probe_is_absent_from_production_build`
- **Dimension 3.2** — an unprobed changed edge counts as uncovered, not as a failure to build → Test `test_unprobed_edge_is_uncovered_not_fatal`

### §4 — The lane and its floor

`make coverage-changed-backend-branches BASE_REF=origin/main` enumerates, runs the backend unit lane, writes `test-results/changed-backend-branches.json`, and exits non-zero when the ratio of executed to enumerated edges is below the floor.

The floor is `CHANGED_BRANCH_COVERAGE_FLOOR = 0.50`, a named constant. A diff enumerating zero edges passes with ratio reported as `null` and an explicit `reason: "no changed edges"`, never as `1.0`, so a no-op diff cannot look like perfect coverage.

- **Dimension 4.1** — the lane writes the artifact and enforces the floor → Test `test_changed_branch_lane_enforces_floor`
- **Dimension 4.2** — zero enumerated edges reports null, not a perfect score → Test `test_zero_edge_diff_reports_null_ratio`

## Interfaces

`test-results/changed-backend-branches.json`:

```
{
  "schema_version": 1,
  "base_ref": string,
  "head_sha": string,
  "parser": {"package": string, "version": string},
  "enumerated": [{"id": string, "file": string, "kind": string, "function": string}],
  "executed": [string],
  "ratio": number | null,
  "floor": number,
  "reason": string | null,
  "pass": boolean
}
```

`branch_probe.mark(comptime id: []const u8) void` — the only public surface in Zig.

## Failure Modes

Every row is also a Test Specification row. The two tables name the same tests on purpose; neither is a subset of the other.

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| Parser failure | unparseable or unsupported syntax | malformed fixture file | lane fails loudly, no artifact written | `test_parse_failure_does_not_pass_silently` |
| Line drift | unrelated edit shifts line numbers | fixture with prepended imports | identity unchanged, manifest still valid | `test_branch_identity_is_reformat_stable` |
| Empty diff | no changed `.zig` edges | no-op diff fixture | ratio `null`, `reason` set, pass true | `test_zero_edge_diff_reports_null_ratio` |
| Below floor | changed edges untested | synthetic diff with unprobed edges | non-zero exit, artifact records ratio | `test_changed_branch_lane_enforces_floor` |
| Probe leakage | probe reaches production build | release build symbol scan | zero probe symbols | `test_probe_is_absent_from_production_build` |
| Stale manifest | manifest from a different base | mismatched base ref fixture | rejected, names the mismatch | `test_changed_edge_enumeration_is_reproducible` |

## Invariants

1. Edge identity is content-addressed, never positional, so reformatting cannot invalidate or forge a manifest.
2. The production binary contains no probe symbol and no probe instruction.
3. A missing probe lowers the ratio; it never breaks the build.
4. Zero enumerated edges reports `null`, never `1.0`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| changed-branch coverage artifact | engineering | the lane runs | counts, ratio, edge ids, file paths | repository paths only; no request, tenant, or secret data | `test_changed_branch_lane_enforces_floor` |

## Test Specification (tiered)

This table is the complete set. Every row is mandatory.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit | `test_branch_identity_is_reformat_stable` | identity survives prepended lines, comment edits, and gofmt-style reflow |
| 1.2 | unit | `test_branch_identity_separates_distinct_edges` | distinct kinds, ordinals, and functions never collide |
| 2.1 | unit | `test_changed_edge_enumeration_is_reproducible` | same diff and parser produce byte-identical manifests; base-ref mismatch is rejected |
| 2.2 | unit | `test_parse_failure_does_not_pass_silently` | a parse error exits non-zero and writes no artifact |
| 3.1 | integration | `test_probe_is_absent_from_production_build` | release binary symbol scan finds no probe |
| 3.2 | unit | `test_unprobed_edge_is_uncovered_not_fatal` | an unprobed edge lowers the ratio and does not fail the build on its own |
| 4.1 | integration | `test_changed_branch_lane_enforces_floor` | artifact written; below-floor exits non-zero; at-or-above exits zero |
| 4.2 | unit | `test_zero_edge_diff_reports_null_ratio` | ratio `null`, `reason` populated, `pass` true |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Instrument tests pass | `make test-unit-all` | exit 0 | P0 | |
| R2 | Lane runs against its own diff | `make coverage-changed-backend-branches BASE_REF=origin/main` | exit 0 and artifact written | P0 | |
| R3 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | |
| S1 | Lint/conform/build | `make lint-all && make harness-verify && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S2 | Memory/secrets | `make memleak && gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line. Every P0 must pass.

## Dead Code Sweep

No file deletion. The probe module is the only new production-visible symbol and must compile to nothing outside test builds; a release symbol scan proves it.

## Out of Scope

- Repository-wide branch coverage, historical backfill, and any floor above the named constant.
- Replacing kcov line coverage, which stays as it is.
- M143_001 API work, M143_002 UI work, and M143_003 performance evidence.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a reviewer opens the artifact and sees which new error paths no test touched.
2. **Preserved user behaviour** — none changes; the instrument is invisible at runtime.
3. **Optimal-way check** — content-addressed identity plus an advisory probe measures without taxing authors.
4. **Rebuild-vs-iterate** — new instrument, because no existing lane distinguishes edges from lines.
5. **What we build** — identity, enumeration, probe, lane, artifact, floor.
6. **What we do NOT build** — mandatory probes, repository-wide coverage, or a gate that blocks adding error handling.
7. **Fit with existing features** — sits beside kcov line coverage and the existing `test-results/` artifacts.
8. **Surface order** — instrument first; workstreams adopt it as they touch backend code.
9. **Dashboard restraint** — a JSON artifact, no panel.
10. **Confused-user next step** — the artifact names each uncovered edge with its file and enclosing function.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** identity, enumeration, probe, and lane are four separable slices with their own tests.
- **Alternatives considered:** `file:line` identity, rejected because reformatting silently invalidates it; mandatory probes, rejected because they make every new branch a build break; kcov line coverage, rejected because it cannot see edges.
- **Patch-vs-refactor verdict:** **new instrument** — nothing existing measures this, so there is nothing to patch.

## Discovery (consult log)

- **Consults** — split from M143_003 §3 during CTO spec review; the mandatory-probe rule and `file:line` identity were both reversed there, with reasons recorded in §1 and §3.
- **Metrics review** — repository-local artifact only; no product telemetry.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.
