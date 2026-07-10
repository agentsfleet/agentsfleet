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

# M122_003: Release the affinity claim on reclaim error; delete the collapsed role-claim machinery

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 003
**Date:** Jul 09, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — one bounded, self-healing single-fleet stall on a transient error path, and one dead-code hygiene cleanup; neither loses data, neither is a live production incident.
**Categories:** API, CLI
**Batch:** B1 — independent; no shared files with any other open workstream.
**Branch:** `feat/m122-error-path-residue`
**Test Baseline:** unit=2402 integration=267
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, Jul 02, 2026 `fleet-wide-refactor-audit`; both findings re-verified against HEAD `7a06fb5d` on Jul 09, 2026 by the `audit-open-items-recheck` workflow, each survivor passing an adversarial refutation pass that corrected the original inflated severities down).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/architecture/runner_fleet.md` — the per-fleet lease slot (`fleet.runner_affinity`), its atomic claim, and the reclaim-vs-fresh acquisition path §1 hardens.

---

## Overview

**Goal (testable):** a reclaim-stage failure after a runner has won a fleet's lease slot releases that slot immediately (claimable by the next poll) instead of holding it for the full lease Time To Live (TTL); and the byte-identical duplicate role-namespace constant plus its dead probes are removed from the Command-Line Interface (CLI) Java Web Token (JWT) decoder with a standing guard against their return.

**Problem:** In `tryCandidate` (`src/agentsfleetd/fleet/assign.zig:122`) the reclaim probe is a bare `try reclaim.reclaimPriorActive(...)` with no release on the error path — unlike `acquireFresh`, which releases the won slot on every one of its failure branches. When the reclaim query, row-drain, or an allocation errors after a win, the claim propagates out uncleaned; the slot stays leased until its own expiry, so that one fleet stalls. The stall is **bounded and self-healing**: the claim's `leased_until` is `now + LEASE_TTL_MS` (~30s), after which another runner re-claims — no data loss, within the system's existing recovery envelope. Separately, `cli/src/program/auth-token.ts:46-47` declares `ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` as **byte-identical** strings and probes both at lines 79-82; the M109_004 §3 that would have restored a real dev/prod split was dropped before implementation (static roles were removed from the product), so this is dead machinery — a redundant no-op probe, never a wrong answer — not a dropped lookup.

**Solution summary:** mirror `acquireFresh`'s discipline on the reclaim path — release the won slot before the reclaim-stage error propagates, and log the release so the recovery is observable. On the CLI side, collapse the two identical constants into one `ROLE_NAMESPACE`, drop the duplicate probes (role resolution is unchanged — one namespace still resolves), and add a regression test that fails if a duplicate namespace constant is ever reintroduced.

## PR Intent & comprehension handshake

- **PR title (eventual):** Release the affinity claim on reclaim error; drop the duplicate CLI role namespace
- **Intent (one sentence):** a transient reclaim-stage error frees the fleet's lease slot at once rather than stalling it for ~30s, and the CLI JWT decoder stops carrying a byte-identical duplicate of its role-namespace probe.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/fleet/assign.zig` — `acquireFresh` releases the won slot on every failure branch (lines 138, 150, 158, 163); `tryCandidate`'s reclaim probe (line 122) is the lone omission §1 fixes. Mirror the branch style verbatim.
2. `src/agentsfleetd/fleet/affinity.zig` — `release(conn, fleet_id, token)` is token-guarded and idempotent (a no-op if the token was bumped or the row is gone), so a release-then-propagate is safe even if `acquireFresh` later releases the same token; `claim` sets `leased_until = now + LEASE_TTL_MS`.
3. `dispatch/write_zig.md` — errdefer placement / RULE OWN (one release path per exit), cross-compile both linux targets, `make memleak` when lease lifecycle is touched.
4. `cli/src/program/auth-token.ts` — the two byte-identical `ROLE_NAMESPACE_*` consts and the four probes at lines 79-82; the sole consumer is `cli/src/cli.ts:233` (a last-priority role-gate UI hint), confirmed by grep — nothing else reads the role claim.
5. `docs/v2/done/M109_004_P1_API_CLI_UI_CROSS_SURFACE_HYGIENE.md` — the Jul 02 scope amendment dropping §3 with Indy's ack (static roles nuked): the reason this is a deletion, not a restore of the dev/prod split.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/fleet/assign.zig` | EDIT | release the won slot + log before a reclaim-stage error propagates from `tryCandidate` |
| `src/agentsfleetd/fleet/event_lifecycle_reclaim_integration_test.zig` | EDIT | add the negative test asserting the slot is released on an injected reclaim-stage failure |
| `cli/src/program/auth-token.ts` | EDIT | collapse the two identical `ROLE_NAMESPACE_*` consts into one; drop the duplicate probes |
| `cli/test/auth-token.unit.test.ts` | EDIT | add the anti-reappearance regression guard; existing role-resolution asserts stay unchanged |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (remove the duplicate constant + its dead probes), **NLR** (touch-it-fix-it: collapse the vestigial namespace machinery in the same diff that touches the file), **UFS** (the byte-identical literal held by two consts is a unified-form violation — one named const), **ORP** (grep `ROLE_NAMESPACE_DEV` / `ROLE_NAMESPACE_COM` → 0 after the collapse), **OWN** (the won claim has exactly one release path per exit — no `defer`+`errdefer` overlap on the same slot), **OBS** (the new release-on-error branch emits a scoped log line), **TST-NAM** (new test identifiers carry no milestone/section IDs).
- **`dispatch/write_zig.md`** — §1 edits Zig: errdefer/branch-release placement, tagged shapes untouched, cross-compile both linux targets, `make memleak`.
- **`dispatch/write_ts_adhere_bun.md`** — the `.ts` edit: `const` discipline, no new repeated literal, Bun-native test in the existing suite.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `assign.zig` + its integration test | cross-compile `x86_64-linux` + `aarch64-linux`; `make memleak` (lease lifecycle touched) |
| PUB / Struct-Shape | no — `tryCandidate` stays private; the test drives the public `select`; no new `pub` symbol | PUB GATE: skipped — no pub surface change |
| File & Function Length (≤350/≤50/≤70) | no — `assign.zig` is 201 lines, `tryCandidate` ~10; the fix adds ~4 lines | no split needed |
| UFS (repeated/semantic literals) | yes — one `ROLE_NAMESPACE` const replaces the byte-identical pair | single named const referenced by every probe |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes; others no | reuse the file's `.runner_assign` scope for the release-on-error line; no new UZ error code, no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/fleet/assign.zig` `acquireFresh` — the exact release-on-every-failure-branch shape §1 replicates on the reclaim path; divergence: none.
- **Reference:** `cli/test/auth-token.unit.test.ts` — the existing role-resolution assertions that must stay green prove the single-namespace path is unchanged; the new guard is a source-inspecting regression test in the same file.

## Sections (implementation slices)

### §1 — Release the affinity claim on the reclaim-stage error

`tryCandidate` wins the slot (`affinity.claim`), then probes `reclaim.reclaimPriorActive` with a bare `try`. On a reclaim-stage error the win leaks: the slot holds until its own `leased_until` expiry, stalling that one fleet for up to the lease TTL. Release the slot before the error propagates, mirroring `acquireFresh`'s branch style. **Implementation default:** wrap the reclaim call in an explicit `catch` that calls `affinity.release(conn, fleet_id, won.token)` (its own error swallowed, idempotent), logs `reclaim_error_released` on the `.runner_assign` scope, then re-raises the error so `select`'s existing catch still logs `assign_failed` — a single release path per exit, no broad `errdefer` overlapping `acquireFresh`'s own releases (RULE OWN).

- **Dimension 1.1** — a reclaim-stage failure after a won claim releases the slot: its `leased_until` is reset to ~now (claimable by the next poll), not held for the full TTL → Test `test_reclaim_error_releases_claim` (injected at the Postgres layer — see Discovery)
- **Dimension 1.2** — happy-path reclaim and fresh acquisition are byte-identical to before → existing `event_lifecycle_reclaim_integration_test` suite green

### §2 — Collapse the duplicate role-namespace constant + probes

`ROLE_NAMESPACE_DEV` and `ROLE_NAMESPACE_COM` are the same string; both are probed. Replace the pair with one `ROLE_NAMESPACE` const and drop the duplicate probe entries so the candidate list carries each namespace exactly once. Role resolution is unchanged — the single live namespace still resolves for every claim shape.

- **Dimension 2.1** — one `ROLE_NAMESPACE` const remains; the candidate probe list has no byte-identical duplicate; every existing role-resolution assertion (top-level, metadata, custom/app_metadata, namespaced, whitespace-reject, null-token) passes unchanged → existing `auth-token.unit.test.ts` role asserts green

### §3 — Guard against the duplicate namespace reappearing

A standing regression test in the CLI suite inspects the decoder source and fails if more than one role-namespace constant (or a byte-identical duplicate probe) is reintroduced — so the collapse cannot silently regress in a later edit.

- **Dimension 3.1** — the source declares exactly one role-namespace constant and no `ROLE_NAMESPACE_DEV` / `ROLE_NAMESPACE_COM` identifier survives → Test `test_no_duplicate_role_namespace_constant`

## Interfaces

```
No public interface changes.
- Lease/runner wire shape: unchanged (no field added or removed).
- affinity.release / reclaim.reclaimPriorActive / affinity.claim: signatures unchanged.
- extractRoleFromToken(token: unknown): RoleClaim | null  — signature and every
  resolved value unchanged; only the internal candidate probe list is deduped.
This spec changes internal error-path control flow and deletes a duplicate constant.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reclaim-stage error after a won claim | reclaim query/drain failure or an Out of Memory (OOM) on the row dupe | slot released (`leased_until` = now), `reclaim_error_released` logged, error re-raised → `select` logs `assign_failed` and returns null; the next poll re-claims immediately |
| Duplicate namespace constant reintroduced | a future edit re-adds a byte-identical `ROLE_NAMESPACE_*` pair | `test_no_duplicate_role_namespace_constant` fails in Continuous Integration (CI) before merge |
| Malformed / absent / whitespace role claim | bad or missing JWT claim | `extractRoleFromToken` returns null (existing behavior, preserved by the retained resolution asserts) |

## Invariants

1. Every won `affinity` claim in `tryCandidate` is released on every non-success exit — enforced by the explicit release-before-re-raise on the reclaim path (compiler-scoped, cannot be skipped on the error branch), verified by `test_reclaim_error_releases_claim`.
2. The CLI JWT decoder declares exactly one role-namespace constant — enforced by `test_no_duplicate_role_namespace_constant` running in the CI test suite (a runtime check that blocks merge, not review discipline).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_assign.reclaim_error_released` (log line) | ops | a reclaim-stage error releases a won slot | `fleet_id`, `err`, `fencing_token` | no secret/token material — fencing token is a monotonic counter, not a credential | `test_reclaim_error_releases_claim` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_reclaim_error_releases_claim` | seed a fleet with a prior `active` lease and an unclaimed affinity slot; arm a `NOT VALID` CHECK constraint on `fleet.runner_leases` rejecting the reclaim's `status = 'expired'` write, so `reclaimPriorActive` errors at row-read **after** `affinity.claim` won → the lease poll yields no lease AND the `runner_affinity` row's `leased_until` is ≤ now, so a fresh `affinity.claim` wins immediately with a strictly higher fencing token, rather than the slot being held for the full TTL |
| 1.2 | integration (regression) | existing `event_lifecycle_reclaim_integration_test` | happy-path reclaim + fresh acquisition unchanged |
| 2.1 | unit (regression) | existing `auth-token.unit.test.ts` role asserts | every retained role-resolution case (top-level, metadata, custom_claims, app_metadata, namespaced, whitespace-reject, null-token) yields the same result after the collapse |
| 3.1 | unit | `test_no_duplicate_role_namespace_constant` | the decoder source contains exactly one role-namespace constant; grep for `ROLE_NAMESPACE_DEV` / `ROLE_NAMESPACE_COM` → 0 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Reclaim-stage error releases the claim (§1) | `make test-integration` | exit 0; `test_reclaim_error_releases_claim` passes | P0 | |
| R2 | No duplicate namespace constant (§2/§3) | `grep -rn "ROLE_NAMESPACE_DEV\|ROLE_NAMESPACE_COM" cli/src` | no output | P0 | |
| R3 | CLI role-resolution + guard green (§2/§3) | `cd cli && bun test test/auth-token.unit.test.ts` | exit 0 | P0 | |
| R4 | Diff touches exactly the 4 Files-Changed code paths | `git diff --name-only origin/main \| grep -vE '\.md$' \| wc -l` | `4` | P0 | |
| S1 | Zig unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks (lease lifecycle touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `ROLE_NAMESPACE_DEV` / `ROLE_NAMESPACE_COM` | `grep -rn "ROLE_NAMESPACE_DEV\|ROLE_NAMESPACE_COM" cli/src` | 0 matches |

## Out of Scope

- Restoring a real dev/prod role-namespace split — the static-role concept it fed was removed from the product (M109_004 scope amendment, Indy-acked); reintroducing it is plumbing for a claim nothing reads.
- Reworking the lease TTL, sticky routing, or the fresh-acquisition path — §1 only closes the reclaim-path release gap.
- `cli/dist/**` build artifacts — gitignored and regenerated by `cd cli && bun run build`; not part of the diff.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a runner hits a transient database (DB) blip mid-reclaim; instead of that fleet's slot sitting idle for ~30s, the slot frees on the same pass and the next poll re-leases the pending event — the operator never sees the stall.
2. **Preserved user behaviour** — happy-path reclaim, fresh acquisition, fencing/renewal semantics, and every CLI role-gate resolution keep their exact outcomes; only the error-path cleanup and an internal duplicate constant change.
3. **Optimal-way check** — releasing the won slot on the reclaim error path is the most direct fix (it mirrors the sibling `acquireFresh` branch already in the same function); no new abstraction, no lease-model change.
4. **Rebuild-vs-iterate** — iterate: two contained fixes on merged surfaces; nothing here wants a redesign, and neither trades away run-to-run determinism.
5. **What we build** — one release-before-re-raise branch (+ its log), one negative integration test, one constant collapse, one anti-reappearance guard.
6. **What we do NOT build** — a broad `errdefer` over the whole win-to-return span (would overlap `acquireFresh`'s own releases — RULE OWN); a restored role-namespace split; any lease-model change.
7. **Fit with existing features** — compounds the runner-fleet lease lifecycle (`affinity` + `reclaim`) and the CLI JWT decoder; must not destabilize fencing-token monotonicity or role resolution — both proven unchanged by the retained suites.
8. **Surface order** — backend + CLI internals; no new user surface.
9. **Dashboard restraint** — N/A — no user surface; the only new signal is an operator log line on an error path that already logged `assign_failed`.
10. **Confused-user next step** — N/A — no user surface; an operator seeing `reclaim_error_released` alongside `assign_failed` reads the recovery directly from the logs.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — one backend error-path fix, one CLI deletion, one standing guard — each independently testable and DONE-markable.
- **Alternatives considered:** (a) a single broad `errdefer affinity.release(...)` covering the whole win-to-return span — rejected: it overlaps `acquireFresh`'s existing per-branch releases, muddying the single-owner rule even though `release` is idempotent; the explicit reclaim-path catch keeps one release per exit. (b) Restoring the `ROLE_NAMESPACE_DEV`/`ROLE_NAMESPACE_COM` split — rejected: the product removed static roles, so a split would re-add plumbing for a claim nothing reads (Indy-acked in M109_004).
- **Patch-vs-refactor verdict:** this is a **patch** — a four-line release branch plus a dead-code deletion; the only structural change (dropping a duplicate probe) shrinks the decoder rather than restructuring it.

## Discovery (consult log)

- **Consults** —
  - **Dimension 1.1 injection mechanism corrected at CHORE(open).** The authored plan (a `std.testing.FailingAllocator` "pinned to that allocation") is not implementable: `select` takes an `Hx` and reads `hx.alloc` (the request arena) rather than accepting an allocator, and both `listCandidates` and `affinity.claim` allocate from it *before* the reclaim probe, so no stable `fail_index` isolates the reclaim's `alloc.dupe`. Probing the count first is destructive — `reclaimPriorActive` marks the lease `expired` on its first (successful) pass. Replaced with a Postgres-level injection: a `NOT VALID` CHECK constraint rejecting `status = 'expired'` fails the reclaim `UPDATE ... RETURNING` deterministically, after the claim is won, on the identical `catch` branch. Same branch proven; no production signature widened for testability.
  - **Release-after-error is safe on the same pooled connection (verified in `pg.zig`).** A server-side error surfaces at `Result.next()` (Bind already completed), leaving `conn._state == .query`; `reclaimPriorActive`'s `defer q.deinit()` → `PgQuery.drain()` consumes the trailing `ReadyForQuery`, restoring `_state = .idle` before the error reaches `tryCandidate`. So `affinity.release(conn, ...)` in the catch runs on a usable connection, and `pool.release` sees an idle conn (no reconnect churn). The OOM sub-case drains identically. This is why the release belongs in `tryCandidate` — after `reclaimPriorActive` has fully unwound — and not inside it.
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
