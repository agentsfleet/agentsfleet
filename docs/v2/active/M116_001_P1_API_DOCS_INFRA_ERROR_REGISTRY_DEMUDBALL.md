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

# M116_001: De-mudball internalOperationError, delete dead codes, fix the raw-tag leak, and fence it

**Prototype:** v2.0.0
**Milestone:** M116
**Workstream:** 001
**Date:** Jul 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a raw internal error name leaks to callers, 35 sites leak jargon, 5 codes are dead, and nothing stops the mudball recurring; all are user/operator hygiene.
**Categories:** API, DOCS, INFRA
**Batch:** B1 — standalone cleanup workstream.
**Depends on:** none — follow-up to M114_001 §8 (the error-registry curation review artifact).
**Provenance:** agent-generated (pre-spec, the M114_001 §8 "agentsfleetd error registry — full inventory & curation review" artifact)
**Canonical architecture:** `src/agentsfleetd/errors/error_entries.zig` (registry source of truth); `docs/LOGGING_STANDARD.md` §error-codes (authoring rule home). No dedicated errors architecture doc exists.
**Branch:** feat/m116-error-registry-demudball
**Test Baseline:** unit=2327 integration=249 via `make _lint_zig_test_depth`

---

## Overview

**Goal (testable):** No `internalOperationError()` call site leaks a raw Zig error tag or internal jargon; the 5 never-thrown codes are deleted; the model-catalogue user copy says "library" (matching `core.model_library`); `error-codes.mdx` is generated from the registry; and standing gates make a new leak or a silent new mudball site fail closed.

**Problem:** `common.internalOperationError(res, detail, req_id)` collapses 86 distinct internal failures onto one code (`UZ-INTERNAL-003`) plus a free-text `detail` — so the `detail` is what surfaces. One site (`http/server.zig:247`) leaks the raw Zig error name via `@errorName(e)`; 35 leak internal jargon; 5 registry codes are never thrown. Separately: the model-provider errors say "catalogue" while the table and product term is **library** (`core.model_library`); the published error-codes page has no generator or parity guard, so it silently drifts; and nothing stops the next dev adding a 87th mudball site.

**Solution summary:** Delete the 5 dead codes. Replace the `@errorName(e)` leak with a stable mapping. Triage all 35 Jargon sites — promote the reachable/operator-actionable/semantically-distinct ones to first-class codes, scrub the pure-internal-transient ones (alloc/OOM/serialize/dedup) to clean details, leave the 50 Generic on the catch-all. Rename "catalogue" → "library" in the `UZ-PROVIDER-004/006/007/008` copy. Generate `error-codes.mdx` from the registry (make the page's `<Note>` true). Then **fence it**: standing gates that block a raw-tag/jargon detail, a new `internalOperationError()` without inline justification, and a dashboard-reachable code declared without a `user_message`.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m116): de-mudball internalOperationError, drop dead codes, fix raw-tag leak, fence recurrence
- **Intent (one sentence):** Internal failures stop leaking tags/jargon, dead codes leave the registry, model copy uses "library", the docs page is generated, and gates keep it that way.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/errors/error_entries.zig` + `error_entries_runtime.zig` — registry source of truth; `e()` = uncurated, `eu()` = curated with `user_message`. Dead codes deleted, new codes added, `catalogue`→`library` renamed here.
2. `src/agentsfleetd/errors/error_registry.zig` — `ERR_*` constants + comptime lookup; every registry change is paired here (`audits/error-codes.sh` enforces declared==used).
3. `src/agentsfleetd/http/handlers/common.zig` — the `internalOperationError()` helper; understand its mapping before rewiring sites.
4. `src/agentsfleetd/errors/internal_op_error_sweep_test.zig` — `BASELINE_CALL_SITE_COUNT = 86`; §3 recomputes it, §5 upgrades it to require per-add justification.
5. The M114_001 §8 artifact (this spec's provenance) — the authoritative enumeration of the 35 Jargon sites (file:line + detail) and the 5 dead codes; use it as the work-list.
6. `audits/error-codes.sh` + `docs/LOGGING_STANDARD.md` §error-codes — the gate script and repo-local authoring-rule home that §5 extends (repo-local by design — avoids the dotfiles-dispatch Invariance Suite).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | Delete 5 dead rows; add promoted codes; `catalogue`→`library` on PROVIDER-004/006/007/008 |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Same, for entries in the runtime table |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Remove 5 `ERR_*`; add promoted-code constants |
| `src/agentsfleetd/errors/internal_op_error_sweep_test.zig` | EDIT | Recompute baseline; add per-add justification check (§5) |
| `src/agentsfleetd/http/server.zig` | EDIT | Replace `@errorName(e)` leak at ~:247 with a stable mapping |
| `src/agentsfleetd/http/handlers/**` (the ~17 files carrying the 35 Jargon sites — enumerated in the M114_001 §8 artifact: connectors/{slack/events,catalog}, fleets/{create,create_fleet_bundle,messages,secrets}, webhooks/{github,fleet}, runner/{bundles,credentials_mint}, library/{onboard,pipeline}, approvals/resolve, memory/handler, auth/identity_events_clerk, workspaces/lifecycle, tenant_provider) | EDIT | Promote or scrub each Jargon site |
| `**/*_test.zig` (co-located) | EDIT/CREATE | One negative test per promoted code |
| `audits/error-codes.sh` | EDIT | Standing guards: raw-tag/jargon-detail ban, reachable⇒`eu()` check (§5) |
| `docs/LOGGING_STANDARD.md` | EDIT | §error-codes authoring rule: distinct failure ⇒ distinct code; reachable ⇒ `user_message` (§5) |
| `make/*.mk` | EDIT | `gen-error-codes` target rendering the mdx from the registry (§4) |
| `~/Projects/docs/api-reference/error-codes.mdx` | EDIT | Regenerated from the registry (own docs-repo branch) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (delete dead codes at write time), NLR (touch-it-fix-it on each jargon site), UFS (new `UZ-…` literals live only in the registry file), ORP (orphan sweep after deletion).
- **`dispatch/write_zig.md`** — every edited `*.zig`: pg-drain, `errdefer`, tagged-union results, file ≤350 / fn ≤50, cross-compile both linux targets.
- **`docs/LOGGING_STANDARD.md` §error-codes** — L3: registry entry lands in the same commit as first reference; §5 extends this doc (repo-local — no `dispatch/` or `AGENTS.md` edit, so the Invariance Suite does not fire).
- **`dispatch/write_any.md` §Error Registry Gate** — new codes match `UZ-<CAT>-<NNN>`; `audits/error-codes.sh --staged` stays green.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; no new queries so pg-drain unaffected |
| File & Function Length (≤350/≤50/≤70) | yes | in-place swaps; split `error_entries*.zig` only if a table crosses 350 |
| UFS | yes | every new `UZ-…` literal + shared detail is a named constant; call sites use the `ERR_*` symbol |
| ERROR REGISTRY | yes | `audits/error-codes.sh` declared==used; §5 adds the raw-tag/jargon/reachable checks the gate then enforces on every diff |
| LOGGING | yes | §5 authoring rule lands in `LOGGING_STANDARD.md`; log-vs-return split preserved (raw tag → log only) |
| PUB / LIFECYCLE / SCHEMA / UI | no | no pub surface, no lifecycle, no schema, no UI in this spec |

## Prior-Art / Reference Implementations

- **Reference (promoted code shape):** `UZ-CONN-001` ("operator must provision the platform app") and `UZ-PROVIDER-009` (M114_001 §8.1) — the shape a promoted config/invariant code follows.
- **Reference (standing count guard):** `internal_op_error_sweep_test.zig` already pins the count; §5 upgrades it to justify-per-add, the same override idiom as other gates.
- **Reference (generated-from-source doc):** the rate-constants precedent (`CHANGELOG_VOICE.md:13`) is the anti-pattern to fix — §4 replaces hand-sync with generation so drift is impossible.

## Sections (implementation slices)

### §1 — Delete the 5 dead codes + fix the model copy ("catalogue"→"library")

Removes codes no producer emits and corrects user-facing terminology to match the entity. **Implementation default:** delete `UZ-BUNDLE-006`, `UZ-GRANT-001`, `UZ-RUN-007`, `UZ-EXEC-001`, `UZ-EXEC-002` + their `ERR_*`; the other 50 REACHABLE=No rows are **live** non-dashboard paths — kept, marked out-of-curation-scope. Rename "catalogue"→"library" in `UZ-PROVIDER-004/006/007/008` (title + hint + `user_message`), matching `core.model_library`.

- **Dimension 1.1** — the 5 dead codes are absent from the registry and have no producer in `src/**` → Test `test_dead_codes_removed`
- **Dimension 1.2** — the ERROR REGISTRY gate stays green (declared==used) after deletion → Test `test_registry_gate_declared_equals_used`
- **Dimension 1.3** — no user-facing PROVIDER copy says "catalogue"; the activate-failure message reads "…isn't in our library yet" → Test `test_model_copy_says_library`

### §2 — Stop the raw-Zig-tag leak (server.zig:247)

The auth-middleware failure path passes `@errorName(e)` as `detail`, exposing the internal Zig error identifier to every scoped route's caller. **Implementation default:** map middleware failure to a stable, non-leaking detail (or a dedicated code); the raw tag goes to logs only.

- **Dimension 2.1** — `server.zig` no longer passes `@errorName(e)` (or any raw tag) as a caller-visible `detail` → Test `test_no_raw_error_tag_in_response`
- **Dimension 2.2** — the middleware-failure response is deterministic (stable code + message; raw tag logged, not returned) → Test `test_auth_mw_failure_response_stable`

### §3 — Triage & de-mudball the 35 Jargon sites

Every Jargon site stops leaking internal language. **Implementation default — the triage rule:** **promote** a site to its own `UZ-<CAT>-<NNN>` when dashboard-reachable OR operator-actionable OR semantically distinct (config/secret-not-configured, invariant violation, install rollback, broker-not-configured); **scrub** it in place to a clean generic `detail` when pure internal-transient (alloc / OOM / serialization / dedup-key overflow / idempotency-check). The `connectors/catalog.zig` sites ("catalog configured/connected lookup failed") are the live example behind the current dashboard "Couldn't load connectors" — promote to a distinct connector-catalog code so the failure is diagnosable.

- **Dimension 3.1** — each promoted code has a registry entry, `ERR_*` constant, and a negative test asserting the code + non-jargon message → Test `test_promoted_codes_have_negative_tests`
- **Dimension 3.2** — no surviving `internalOperationError()` `detail` across the 35 sites contains jargon (component/schema names, "alloc"/"OOM", state-machine phrasing) → Test `test_no_jargon_in_internal_details`
- **Dimension 3.3** — `BASELINE_CALL_SITE_COUNT` recomputed to the post-promotion count; sweep test green → Test `test_sweep_baseline_recomputed`

### §4 — Generate error-codes.mdx from the registry

The page has no generator and silently drifts; the `<Note>` claiming generation is aspirational. **Implementation default:** a `make gen-error-codes` target renders the mdx from `error_entries.zig` (the registry holds every field: code, http_status, title, hint, docs_uri, user_message), plus a parity check that fails on divergence. Generation, not hand-sync — drift becomes impossible.

- **Dimension 4.1** — `make gen-error-codes` renders `error-codes.mdx` from the registry; re-running on a clean tree is a no-op → Test `test_gen_error_codes_idempotent`
- **Dimension 4.2** — the set of `UZ-` codes in the mdx equals the registry set (no dead rows, all promoted present) → Test `test_mdx_registry_parity`

### §5 — Fence the mudball (standing authoring guards)

Turns the one-shot cleanup into permanent gates so the leak and the mudball cannot recur. **Implementation default:** extend `audits/error-codes.sh` (already run pre-commit + in HARNESS VERIFY) with three checks, and record the authoring rule in `docs/LOGGING_STANDARD.md`. All repo-local — no `dispatch/`/`AGENTS.md` edit, so the Invariance Suite does not fire.

- **Dimension 5.1** — the audit blocks any caller-visible `detail` matching the raw-tag/jargon denylist (`@errorName`, `alloc`, `OOM`, schema/component tokens) → Test `test_guard_blocks_jargon_detail`
- **Dimension 5.2** — a new `internalOperationError()` call added without an inline `// mudball-ok: <reason>` fails the sweep (justify-per-add, not a silent baseline bump) → Test `test_guard_requires_mudball_justification`
- **Dimension 5.3** — every `e()` entry carries a `// reachable: no — <reason>` annotation; a `reachable: yes` entry without a `user_message` (i.e. not `eu()`) fails the audit → Test `test_guard_reachable_requires_user_message`
- **Dimension 5.4** — `docs/LOGGING_STANDARD.md` §error-codes states the rule (distinct failure ⇒ distinct code; reachable ⇒ `user_message`; `error-codes.mdx` is generated) → Test `test_authoring_rule_documented`

## Interfaces

```
No new HTTP routes. RFC 7807 problem+json body shape UNCHANGED:
  { docs_uri, title, detail, error_code, request_id }
Registry Entry shape UNCHANGED: { code, http_status, title, hint, docs_uri, user_message? }
Deltas are data + copy: -5 dead entries, +N promoted, "catalogue"→"library" on 4 PROVIDER entries.
internalOperationError() signature UNCHANGED. Promoted codes reuse existing category prefixes.
New tooling: `make gen-error-codes` (registry → error-codes.mdx); audits/error-codes.sh gains 3 checks.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Deleting a live code | Mistaking a REACHABLE=No live path for dead | Only the 5 artifact-confirmed never-thrown codes deleted; §1 grep-proves no producer first |
| Registry gate breaks | A promoted code referenced before its entry lands | Entry + `ERR_*` land in the same commit as first use (LOGGING_STANDARD L3) |
| Sweep test red | Baseline not recomputed after promotion | §3.3 recomputes; §5.2 makes future adds justify explicitly |
| Docs drift | mdx edited by hand instead of generated | §4 generates from the registry; §4.2 parity test fails on divergence |
| Guard false-positive | A legitimate detail trips the jargon denylist | The `// mudball-ok:` / annotation override is the escape hatch; denylist is conservative (tags + alloc/OOM only) |
| Over-promotion | Minting a public code for a transient users can't act on | Triage rule scrubs transients; §3.2 requires no-jargon, not a code-per-site |

## Invariants

1. Every `UZ-<CAT>-<NNN>` literal appears only in the registry file — `audits/error-codes.sh` raw-literal check.
2. `declared == used` for the registry — `audits/error-codes.sh` orphan check.
3. `internalOperationError()` count never grows without an inline justification — §5.2 sweep upgrade.
4. No caller-visible `detail` contains a raw Zig error tag or jargon — §5.1 audit check.
5. A dashboard-reachable code always has a `user_message` — §5.3 audit check.
6. `error-codes.mdx` == registry — §4 generator + parity test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | — | error-hygiene refactor + tooling; no product/operator signal added, no analytics/funnel change | — | raw error tags removed from responses (a leak reduction) | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_dead_codes_removed` | grep each of the 5 codes in `src/**` (non-test) → 0 producer matches; absent from registry |
| 1.2 | unit | `test_registry_gate_declared_equals_used` | `audits/error-codes.sh` exit 0 after deletion |
| 1.3 | unit | `test_model_copy_says_library` | PROVIDER-004/006/007/008 user-facing text contains "library", not "catalogue" |
| 2.1 | integration | `test_no_raw_error_tag_in_response` | inject auth-mw failure → response `detail` has no `@errorName`-style tag |
| 2.2 | integration | `test_auth_mw_failure_response_stable` | same → deterministic code + curated message; raw tag in the log line only |
| 3.1 | unit | `test_promoted_codes_have_negative_tests` | each promoted code has entry + `ERR_*` + a negative test referencing it |
| 3.2 | unit | `test_no_jargon_in_internal_details` | the 35 sites' surviving `detail` literals match no jargon-denylist token |
| 3.3 | unit | `test_sweep_baseline_recomputed` | sweep test passes with new baseline == actual count |
| 4.1 | integration | `test_gen_error_codes_idempotent` | `make gen-error-codes` twice → second run leaves the mdx unchanged |
| 4.2 | verification | `test_mdx_registry_parity` | `comm -3` of registry codes vs mdx anchors → empty |
| 5.1 | unit | `test_guard_blocks_jargon_detail` | a fixture detail with `@errorName`/`OOM` → audit non-zero exit |
| 5.2 | unit | `test_guard_requires_mudball_justification` | an added `internalOperationError(` without `// mudball-ok:` → sweep fails |
| 5.3 | unit | `test_guard_reachable_requires_user_message` | an `e()` marked `reachable: yes` → audit non-zero exit |
| 5.4 | verification | `test_authoring_rule_documented` | `LOGGING_STANDARD.md` §error-codes states the distinct-code + reachable-message rule |
| — | regression | `test_generic_sites_unchanged` | the 50 Generic sites still resolve to `UZ-INTERNAL-003` (no accidental promotion) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | 5 dead codes gone, no producer (§1) | `grep -rEo 'UZ-BUNDLE-006\|UZ-GRANT-001\|UZ-RUN-007\|UZ-EXEC-001\|UZ-EXEC-002' src --include='*.zig' \| grep -v _test.zig` | no output | P1 | |
| R2 | Model copy says "library" (§1) | `grep -in 'catalogue' src/agentsfleetd/errors/error_entries*.zig` | 0 user-facing matches | P1 | |
| R3 | No raw error tag returned (§2) | `grep -n '@errorName' src/agentsfleetd/http/server.zig` | 0 in a caller-visible detail | P1 | |
| R4 | No jargon / sweep intact (§3) | `make test-unit-agentsfleetd` | exit 0 | P0 | |
| R5 | mdx generated & parity (§4) | `make gen-error-codes && comm -3 <(grep -oE 'UZ-[A-Z]+-[0-9]+' src/agentsfleetd/errors/error_entries*.zig\|sort -u) <(grep -oE 'UZ-[A-Z]+-[0-9]+' ~/Projects/docs/api-reference/error-codes.mdx\|sort -u)` | empty | P1 | |
| R6 | Standing guards live & green (§5) | `bash audits/error-codes.sh` | exit 0 | P0 | |
| R7 | Diff inside Files Changed | `git diff --name-only origin/main` (both repos) | 0 paths missing from the table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan sweep (deleted codes) | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted (only registry rows) | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `UZ-BUNDLE-006` / `ERR_*` | `grep -rn 'UZ-BUNDLE-006\|ERR_BUNDLE_006' src/` | 0 matches |
| `UZ-GRANT-001` / `ERR_*` | `grep -rn 'UZ-GRANT-001\|ERR_GRANT_001' src/` | 0 matches |
| `UZ-RUN-007` / `ERR_*` | `grep -rn 'UZ-RUN-007\|ERR_RUN_007' src/` | 0 matches |
| `UZ-EXEC-001` / `ERR_*` | `grep -rn 'UZ-EXEC-001\|ERR_EXEC_001' src/` | 0 matches |
| `UZ-EXEC-002` / `ERR_*` | `grep -rn 'UZ-EXEC-002\|ERR_EXEC_002' src/` | 0 matches |

## Out of Scope

- **Curating the 50 live REACHABLE=No codes** — non-dashboard audiences; a `user_message` is wasted. Kept, marked out-of-scope.
- **Curating the 50 Generic `internalOperationError()` sites** — already plain-English; they stay on the catch-all, bounded by the sweep test.
- **The connectors "Couldn't load" runtime failure** — a live catalog 500 needing the HTTP response to diagnose; a separate bugfix. This spec only de-mudballs the jargon detail it emits.
- **The Models page platform-default UI affordance** — showing a `model:admin` control to a tenant is a UI-scope fix, not an error-registry change.
- **Reworking `internalOperationError()` itself** — the helper stays; this is about what flows through it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user hits an internal failure and reads a clean sentence with a stable code (never `error.Foo` or "OOM building steer actor"); a model-activate failure says "…isn't in our **library** yet"; an operator sees a distinct, actionable config code.
2. **Preserved user behaviour** — RFC 7807 shape and every currently-emitted code a client keys on are unchanged; only leaking details, 5 phantom codes, and "catalogue" wording change.
3. **Optimal-way check** — Yes: promote-or-scrub + generate + fence is the most direct durable fix; a blanket code-per-site over-mints for transients.
4. **Rebuild-vs-iterate** — Iterate. Registry + call-site + tooling edits; helper and body shape unchanged; determinism preserved.
5. **What we build** — 5 deletions, 1 leak fix, ~35 site triage, N promoted codes + tests, a copy rename, a doc generator, 3 standing guards.
6. **What we do NOT build** — Generic-site codes, live-No curation, the connectors runtime fix, the Models UI fix, a helper rewrite (all Out of Scope).
7. **Fit with existing features** — Compounds with M114_001 curation and the ERROR REGISTRY gate; must not destabilize the sweep-test guard or declared==used.
8. **Surface order** — Backend registry + handlers first, generated docs second, guards last; all one milestone.
9. **Dashboard restraint** — N/A — no UI built; promoted codes gain a dashboard `user_message` only once proven reachable (now enforced by §5.3).
10. **Confused-user next step** — The stable `error_code` + its generated `docs_uri` anchor is the self-serve path.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, five Sections by concern (delete+copy / leak / de-mudball / generate-docs / fence). Each independently verifiable; all share "error-registry hygiene, made durable."
- **Alternatives considered:** (a) Full literal de-mudball (code for all 86) — rejected (Indy: Critical + all 35 Jargon; 50 Generic stay). (b) Delete all 55 REACHABLE=No — rejected: 50 are live. (c) Separate governance spec for §5 — rejected in favour of "fix it and fence it in one PR", kept single-repo by homing the rule in `LOGGING_STANDARD.md` (not the dotfiles `dispatch/`), so the Invariance Suite does not fire.
- **Patch-vs-refactor verdict:** a **patch** — data + call-site + tooling edits against a stable helper and body shape. **Two-repo delivery:** `~/Projects/docs` (generated mdx) on its own `chore/m116-*` branch/PR; the `agentsfleet` repo carries registry, handlers, tests, audit, generator, spec, changelog.

## Discovery (consult log)

- **Consults** — Indy decided at authoring (capture verbatim at CHORE(open)): §1 delete scope = 5 dead only; §3 depth = Critical + all 35 Jargon; §5 fold-in approved ("Yes A) fold into M116"); §1.3 copy fix directed ("error message should use library as opposed to catalogue"); §5 rule homed in `LOGGING_STANDARD.md` to avoid the dotfiles Invariance Suite.
- **Metrics review** — {empty at creation — expected "no analytics/funnel change; error-hygiene + tooling only"}
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: {empty at creation}
- **Deferrals** — {empty at creation}
