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

# M122_001: Reconcile documented surface with served surface — continuation chains, admin model catalogue

**Prototype:** v2.0.0
**Milestone:** M122
**Workstream:** 001
**Date:** Jul 09, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — user-facing docs promise a non-existent safety ceiling (a "10 continuation" cap) and a non-existent failure label (`chunk_chain_escalate_human`); the served admin model-catalogue routes ship undocumented. Neither crashes, so this is doc/parity truth-in-advertising, not a runtime bug.
**Categories:** API, DOCS
**Batch:** B1 — independent; touches no source shared with any pending workstream.
**Branch:** `feat/m122-served-doc-parity`
**Test Baseline:** unit=2402 integration=267
**Depends on:** M122_005 (budget enforcement) — §1 only. The rewritten pages describe enforced per-fleet budgets, so they must not merge ahead of the code that enforces them. Both workstreams share one branch and one Pull Request (PR); §2 and §3 depend on nothing.
**Provenance:** agent-generated (pre-spec, Jul 02 2026 `fleet-wide-refactor-audit`; both findings re-verified against HEAD `7a06fb5d` on Jul 09 2026 by the `audit-open-items-recheck` workflow, each surviving an adversarial refutation pass).
**Canonical architecture:** `docs/REST_API_DESIGN_GUIDELINES.md` §1/§7 (URL shape + route-registration freshness — the coverage gate in §3 extends that family); `docs/SCHEMA_CONVENTIONS.md` is untouched.

---

## Overview

**Goal (testable):** the docs-repo context-lifecycle page describes what the daemon actually does when a run approaches its context cap (the runtime observes and logs; the fleet itself wraps up; nothing re-enqueues — no continuation cap, no `chunk_chain_escalate_human` label) and which ceilings really stop a run; the four served `/v1/admin/models` routes appear in `public/openapi.json`; and a mechanical Continuous Integration (CI) gate fails whenever any served public route lacks a documented (or explicitly allowlisted) OpenAPI entry.
**Problem:** three verified drifts, two of them phantom safety ceilings. (1) `concepts/context-lifecycle.mdx` and `fleets/authoring.mdx` document a continuation-chain feature — the runtime re-enqueues the exhausted event, a chain caps at 10 continuations, the 11th is labelled `chunk_chain_escalate_human` — that does not exist: `chunk_chain`/`escalate_human`/`MAX_CONTINUATION` return zero hits across `src/` and `cli/`, and `service_report.zig` finalizes a non-processed run without ever re-enqueuing. `running.mdx` and `cli/agentsfleet.mdx` compound it by advertising an `actor=continuation:<original_actor>` value that only test fixtures ever write. (2) The platform-admin model catalogue Create/Read/Update/Delete (CRUD) — GET+POST `/v1/admin/models`, PATCH+DELETE `/v1/admin/models/{uid}` — is live-served but absent from every OpenAPI source, and nothing prevents the next served route from drifting the same way. (3) Found while walking §1's golden path: `daily_dollars`/`monthly_dollars` are required in every `TRIGGER.md`, parsed into `FleetConfig.budget`, and read by nothing but `config_parser_test.zig`; the `budget_breach` label they promise has zero hits in `src/`. So `context-lifecycle.mdx`'s "three independent ceilings stop it" answer names three ceilings of which **none** fires: the context cap only writes a log line (`runner_progress.zig:247`), the continuation cap never existed, and the budget cap is dead config.
**Solution summary:** rewrite the five affected docs-repo pages to describe real, grep-provable mechanisms and delete the phantom-feature claims; add `public/openapi/paths/admin-models.yaml` mirroring the sibling `admin.yaml` platform-keys shape, wire it into the bundle, and regenerate `public/openapi.json`; and add `scripts/check_openapi_route_coverage.py` to `make check-openapi` so a served-route-without-a-documented-entry fails CI from now on. Drift (3) is a *code* gap, not a doc gap — Indy's call (Discovery, Jul 10 2026) is to build the enforcement rather than delete the promise, so **M122_005** lands per-fleet budget enforcement on this same branch and PR, and §1's budget prose then describes shipped behaviour rather than an unbuilt ceiling.

## PR Intent & comprehension handshake

- **PR title (eventual):** Reconcile documented vs served surface: continuation-chain docs, admin/models OpenAPI, route-coverage gate
- **Intent (one sentence):** a reader's docs match the running daemon — the context-lifecycle page stops promising an unbuilt safety ceiling, the admin model routes are documented, and a gate keeps served-vs-documented parity true after this PR.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/fleet/service_report.zig` — `finalize()` is the real terminal path: on a non-processed outcome it sets `STATUS_FLEET_ERROR`, persists a granular `failure_label`, checkpoints, `XACK`s, releases affinity — and never re-enqueues. Its header comment states "continuation is a no-op." This is the behaviour §1's rewrite must describe; read it BEFORE writing a word of the doc.
2. `public/openapi/paths/admin.yaml` + `public/openapi/root.yaml:104-107` — the platform-keys shape and the `$ref` registration pattern §2 mirrors verbatim for the model routes (sibling platform-admin surface, already documented).
3. `scripts/check_openapi_url_shape.py` + `scripts/check_route_registration_doc.py` — the mechanical-gate pattern §3 mirrors: read `public/openapi.json`, a small allowlist with a one-line justification per entry, exit non-zero listing each violation.
4. `src/agentsfleetd/http/routes.zig` — the canonical `Route` registry ("All Route variants are registered here"); each variant's trailing comment carries its served path + verbs. This is §3's served-route source of truth.
5. `docs/v2/done/M109_003_P1_API_DOCS_REGISTRY_SCHEMA_DRIFT_RECONCILIATION.md` — prior art from the same audit: how a docs-repo drift fix runs on the `~/Projects/docs` own-branch flow alongside an in-repo change.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/concepts/context-lifecycle.mdx` | EDIT (separate repo, own branch) | rewrite the continuation-chain section, the "Context full" + "Budget breach" rows, and the "Will it run forever?" answer to the ceilings that really fire; drop the 10-cap and `chunk_chain_escalate_human` label |
| `~/Projects/docs/fleets/authoring.mdx` | EDIT (separate repo, own branch) | correct the `<Note>` (line 154) claiming the runtime auto re-enqueues `actor=continuation:<original_actor>`; correct the Budgets section (line 177) to M122_005's enforcement |
| `~/Projects/docs/fleets/troubleshooting.mdx` | EDIT (separate repo, own branch) | the `budget_breach` symptom row (line 16) — real once M122_005 lands; confirm the label and the §5 anchor match |
| `~/Projects/docs/fleets/running.mdx` | EDIT (separate repo, own branch) | drop `continuation:<original_actor>` (line 55) from the actor-tag list — no production code writes it |
| `~/Projects/docs/cli/agentsfleet.mdx` | EDIT (separate repo, own branch) | drop `continuation:*` (line 208) from the `--actor` filter examples — same phantom actor |
| `public/openapi/paths/admin-models.yaml` | CREATE | document GET/POST `/v1/admin/models` + PATCH/DELETE `/v1/admin/models/{uid}`, mirroring `admin.yaml` |
| `public/openapi/root.yaml` | EDIT | register the two new paths as `$ref`s into `admin-models.yaml`; widen the `Admin` tag description to admit the model catalogue |
| `public/openapi.json` | EDIT | regenerated bundle (`redocly bundle`) now carrying the admin/models paths |
| `scripts/check_openapi_route_coverage.py` | CREATE | new gate: every served public `/v1` route is documented in `openapi.json` or in a justified allowlist |
| `scripts/check_openapi_route_coverage_test.py` | CREATE | the gate's self-tests — negative case (§3.1), carve-out rot, and the HEAD regression guard |
| `make/quality.mk` | EDIT | wire the coverage script + its self-tests into the `check-openapi` target |

`~/Projects/docs/changelog.mdx` line 1660 also carries the phantom `actor=continuation:<original_actor>` prose. It is **deliberately not edited**: shipped changelog entries are historical record (`docs/CHANGELOG_VOICE.md` — history is archived, never rewritten). The correction rides the new `<Update>` this workstream appends at CHORE(close).

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NLG** (no docs describing an unbuilt feature pre-2.0.0 — §1 removes the phantom continuation feature rather than framing it "planned"), **NDC** (no dead machinery documented as live), **ORP** (sweep `chunk_chain_escalate_human` / `continuation` re-enqueue prose to zero across the docs repo after the rewrite), **UFS** (path-prefix and allowlist literals in the coverage script are named constants), **CIV** (the coverage script must verify `$ref`-carried paths, not skip them — it reads the bundled `openapi.json`, where refs are resolved), **EMB** (route/OpenAPI parity is enforced by an external Python script, never `@embedFile` across the `src/` boundary), **TST-NAM** (new test identifiers milestone-free).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — §1 URL shape (admin/models paths are already plural-noun shaped) and §7 route-registration freshness (the coverage gate joins that gate family).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` edited; the daemon behaviour is read, not changed |
| PUB / Struct-Shape | no | no new Zig pub surface |
| File & Function Length (≤350/≤50/≤70) | yes — the new Python script | keep `check_openapi_route_coverage.py` ≤350 lines and each function ≤50; `admin-models.yaml` mirrors `admin.yaml`'s size, well under the cap |
| UFS (repeated/semantic literals) | yes — the script | `SPEC_PATH`, the `/v1` prefix, and allowlist route strings as module constants |
| UI Substitution / DESIGN TOKEN | no | no UI |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no handler, error code, or schema change |

## Prior-Art / Reference Implementations

- **Reference (§2):** `public/openapi/paths/admin.yaml` — `admin-models.yaml` copies its operation/response/`$ref` shape for a sibling platform-admin surface; divergence: four verbs across two paths instead of platform-keys' set.
- **Reference (§3):** `scripts/check_openapi_url_shape.py` and `scripts/check_route_registration_doc.py` — same read-only, allowlist-with-justification, exit-non-zero-per-violation gate shape; the coverage gate is a fourth check in the same `check-openapi` family. No pytest harness exists in-repo (scripts self-validate by running); the negative case runs the script against a crafted temporary spec.
- **Reference (§1):** `docs/v2/done/M109_003_…md` §2 — the docs-repo own-branch drift-reconciliation precedent from the same audit.

## Sections (implementation slices)

### §1 — Rewrite the drifted docs pages to grep-provable behaviour

The daemon implements no continuation chains. When a run's prompt approaches the model's context cap, the runtime **observes and logs, it does not act**: `runner_progress.zig:247` computes `prompt_tokens / context_cap_tokens` after every LLM round-trip and, past `stage_chunk_threshold`, emits a `chunk_threshold_breached` line. Its own comment is explicit — *"NullClaw doesn't expose mid-loop interrupt, so the runtime cannot force a chunk — the fleet does it via SKILL prose."* A fleet that follows that prose consolidates findings into memory and ends the run normally: the child exits 0, `child_supervisor_result.zig:classify` reads `exit_ok: true`, and `service_report.zig:finalize()` marks the event **`processed`** and checkpoints the session. Nothing re-enqueues it; `EVENT_TYPE_CONTINUATION` has no production producer, and `actor=continuation:<…>` is written only by `events_integration_test.zig` fixtures. `fleet_error` is the terminal status for the ten `FailureClass` kills (`timeout_kill`, `oom_kill`, `renewal_terminate`, `runner_crash`, …), not for context exhaustion.

Rewrite the five pages listed in Files Changed so that every named mechanism, label, and ceiling has a grep-provable owner in `src/`. **Implementation default:** establish the exact status/label from `service_report.zig`/`event_rows.zig`/`execution_result.zig` BEFORE editing — a knob or ceiling with no code owner is deleted, not softened. The one exception is the budget ceiling: M122_005 gives it a code owner in this same PR, so §1 describes it as enforced. Runs on the `~/Projects/docs` own-branch flow (branch off a clean `main`; never commit onto a dirty `main`) — no docs-repo edit happens during authoring.

- **Dimension 1.1** — after the rewrite, `chunk_chain_escalate_human`, "10 continuations", "continuation cap", and `continuation:` actor prose appear zero times across the four continuation-drifted pages → Test `test_docs_no_phantom_continuation`
- **Dimension 1.2** — the pages describe the real terminal outcomes: a fleet that voluntarily wraps up on a full context ends `processed` with a checkpoint, `fleet_error` carries a `FailureClass` label, and nothing re-enqueues either. The phrase "re-enqueues the same event" is gone → Test `test_docs_describe_real_termination`. *(Amended Jul 10 2026: the spec as authored asserted "context exhaustion → `fleet_error`", which the code refutes — a voluntary wrap-up exits 0 and is `processed`. Writing `fleet_error` would have replaced one falsehood with another.)*
- **Dimension 1.3** — every tuning knob the rewritten pages still name resolves to a real identifier under `src/` (`context_cap_tokens`, `tool_window`, `memory_checkpoint_every`, `stage_chunk_threshold` — all four are parsed in `config_parser.zig` under `x-agentsfleet.context`); a knob with no code backing is removed → Test `test_docs_knobs_have_code_backing`
- **Dimension 1.4** — the "Will it run forever?" answer names only ceilings that fire: the lease deadline (`timeout_kill`), `/renew` policy stops (`renewal_terminate` — max-runtime or tenant credit exhausted), the cgroup limits (`oom_kill` / `resource_kill`), and — once M122_005 lands — the per-fleet budget (`budget_breach`) → Test `test_docs_ceilings_are_real`

### §2 — Document the served admin model-catalogue routes

The four `/v1/admin/models` routes are live-served (registered in `routes.zig`, dispatched in `route_table_invoke.zig`) but absent from every OpenAPI source, while the sibling `/v1/admin/platform-keys` is documented. Add `admin-models.yaml` covering GET+POST `/v1/admin/models` and PATCH+DELETE `/v1/admin/models/{uid}`, register both paths in `root.yaml`, and regenerate `openapi.json`. **Implementation default:** mirror `admin.yaml` for operation ids, tags (`Admin`), and the shared `Error` response `$ref`; read the request/response shapes off `handlers/admin/model_caps_admin.zig` so the documented bodies match what the handlers actually accept and emit.

- **Dimension 2.1** — `public/openapi.json` contains path items for `/v1/admin/models` and `/v1/admin/models/{uid}` → Test `test_admin_models_is_documented_at_head` → **DONE** (bundle carries both; 59 paths, was 57)
- **Dimension 2.2** — the bundle is clean: `admin-models.yaml` has no dangling `$ref` and passes Redocly lint → Test existing `make check-openapi` green (`redocly bundle` + `lint`) → **DONE** (`AdminModel` + `AdminModelRates` hoist into root components; lint green, 28 pre-existing warnings unchanged)
- **Dimension 2.3** — the documented verbs equal the served verbs (GET, POST on the collection; PATCH, DELETE on the item) → Test `test_admin_models_is_documented_at_head` → **DONE** (asserts the verb sets against `routes.zig:80-81`)

### §3 — Route-coverage gate (the durable recurrence fix)

`check_openapi_url_shape.py` and `check_openapi_errors.py` only validate paths already in `openapi.json`; nothing asserts that every served route is present there — which is exactly how admin/models drifted. Add `check_openapi_route_coverage.py`: enumerate the daemon's served public `/v1` routes from the `routes.zig` registry, and fail if any is neither documented in `openapi.json` nor in a small allowlist of deliberately-internal control-plane routes (the runner self-plane, `/v1/fleets/streams`), each carrying a one-line justification. **Implementation default:** derive served paths from `routes.zig` (the canonical registry) by reading each variant's own comment block, normalizing `{param}` names away so `{ws}` and `{workspace_id}` compare equal, and expanding the `{gate_id}:approve|:deny` colon-op alternation; the allowlists mirror the URL-shape script's justified-carve-out style, including its stale-entry sweep. Wire it as a fourth step in `check-openapi`.

Four `Route` variants (`create_auth_session`, `create_workspace`, `approval_webhook`, `workspace_fleet_memories`) carry no path in their comment. Rather than skip them — a silent coverage hole — they are mapped to their served path in a justified `PATHLESS_VARIANT_PATHS` table, so all 65 served `/v1` routes are checked. Adding the missing comments to `routes.zig` is the durable fix and would let the table shrink to nothing; that edit is outside this workstream's Files-Changed scope and is left for a follow-up. Any *new* variant the script cannot resolve is a hard failure, so the hole cannot reopen.

- **Dimension 3.1** — run against a temporary `openapi.json` with a served, non-allowlisted route removed → the script exits non-zero and names the missing path → Test `test_flags_a_served_route_missing_from_the_spec` → **DONE** (unit; verified end-to-end too — deleting `/v1/admin/models` from a scratch copy of the bundle exits 1 with `UNDOCUMENTED ROUTE: /v1/admin/models (routes.zig: admin_models)`)
- **Dimension 3.2** — run at HEAD after §2 (admin/models documented; internal routes allowlisted) → the script exits 0 → Test `test_head_is_clean` → **DONE** (`OK: route coverage — 65 served /v1 routes, 10 internal carve-outs, all documented`)
- **Dimension 3.3** — `check-openapi` invokes the coverage script → **DONE** (`make/quality.mk` runs the self-tests then the gate; `grep -c check_openapi_route_coverage make/quality.mk` → 2)
- **Dimension 3.4** — the gate cannot silently under-cover: a `Route` variant it can neither resolve to a `/v1` path nor find in a justified carve-out table is a hard failure, and a carve-out that stops naming a real route/variant is flagged stale → Tests `test_variant_with_no_path_comment_fails_rather_than_being_skipped`, `test_internal_allow_entry_no_longer_served_is_flagged` → **DONE** (15 self-tests green)

## Interfaces

```
Served routes documented by §2 (verbs must match, shapes mirror admin.yaml):
  GET    /v1/admin/models          → list catalogue entries
  POST   /v1/admin/models          → create a catalogue entry
  PATCH  /v1/admin/models/{uid}    → update an entry (uid = uuidv7)
  DELETE /v1/admin/models/{uid}    → delete an entry
No route, verb, handler, or runtime behaviour changes — documentation only.

scripts/check_openapi_route_coverage.py behaviour:
  reads public/openapi.json; exit 0 when every served public /v1 route is
  documented or allowlisted; exit non-zero listing each undocumented,
  non-allowlisted served path. No arguments; run from repo root.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A new served route ships undocumented | future PR adds a `/v1` route, forgets the OpenAPI entry | `check_openapi_route_coverage.py` exits non-zero in `check-openapi`; CI fails naming the path (§3.1) |
| Dangling `$ref` in `admin-models.yaml` | typo'd component reference | `redocly bundle` fails before `openapi.json` regenerates; `check-openapi` red (§2.2) |
| Docs re-drift | someone re-adds a `chunk_chain` claim | the §1 greps (1.1/1.2) catch it on the docs branch; honest bound — no standing CI gate exists in this repo for the separate docs repo |
| Internal control-plane route falsely flagged | runner-lease/report path is intentionally undocumented | the gate's allowlist carries it with a one-line justification; adding an entry is a review surface (§3.2) |
| admin/models route renamed, YAML left stale | rename without updating `admin-models.yaml` | served path no longer matches a documented path → coverage gate fails (§3.1) |

## Invariants

1. Every served public `/v1` route is present in `public/openapi.json` or in the coverage gate's justified allowlist — enforced by `check_openapi_route_coverage.py` in `make check-openapi` (CI), not review discipline.
2. `public/openapi.json` is a clean bundle of `public/openapi/**` with no dangling `$ref` — enforced by `redocly bundle` + `redocly lint` in `check-openapi`.
3. The daemon implements no continuation re-enqueue / cap / escalation — enforced by the existing production code (`chunk_chain`/`escalate_human`/`MAX_CONTINUATION` grep to zero across `src/` + `cli/`); §1 corrects the docs to match, and the §1.1 grep keeps the phantom strings off the four pages on the branch.
4. The coverage gate never silently under-covers: every `Route` variant resolves to a checked path, a justified pathless-variant mapping, or a justified non-`/v1` carve-out — an unresolvable variant fails CI rather than being skipped (`test_variant_with_no_path_comment_fails_rather_than_being_skipped`).
5. No runtime behaviour changes **in this workstream** — §1/§2/§3 touch docs, OpenAPI sources, and a CI script only. The runtime change that makes §1's budget prose true lives in M122_005, which carries its own Invariants and Failure Modes.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | this workstream edits docs, OpenAPI sources, and a CI script; it adds, renames, and removes no event | unchanged | unchanged | existing suites stay green |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit (grep, docs repo) | `test_docs_no_phantom_continuation` | `grep -c "chunk_chain_escalate_human\|10 continuations\|continuation cap\|continuation:"` over the four continuation-drifted pages → 0 |
| 1.2 | unit (grep, docs repo) | `test_docs_describe_real_termination` | "re-enqueues the same event" absent; `processed` (voluntary wrap-up) and `fleet_error` (`FailureClass` kill) both described |
| 1.3 | unit (grep, cross-repo) | `test_docs_knobs_have_code_backing` | each knob identifier named in the rewritten pages matches a token under `src/`; unbacked knobs removed |
| 1.4 | unit (grep, cross-repo) | `test_docs_ceilings_are_real` | every ceiling named in "Will it run forever?" resolves to a `FailureClass` tag or a gate label under `src/` |
| 2.1 | unit (bundle assertion) | `test_admin_models_is_documented_at_head` | `openapi.json` `paths` contains `/v1/admin/models` and `/v1/admin/models/{uid}` |
| 2.2 | integration (regression) | existing `make check-openapi` | `redocly bundle` + `lint` exit 0 with the new YAML wired in |
| 2.3 | unit | `test_admin_models_is_documented_at_head` | documented verbs = {GET, POST} on the collection, {PATCH, DELETE} on the item |
| 3.1 | unit (negative) | `test_flags_a_served_route_missing_from_the_spec` | served route absent from the spec → violation names the path and its `routes.zig` variant |
| 3.2 | unit (positive) | `test_head_is_clean` | at HEAD post-§2 → zero violations, >50 served routes parsed |
| 3.3 | unit (grep) | `make check-openapi` | the target runs the self-tests and the coverage script |
| 3.4 | unit (rot guards) | `test_variant_with_no_path_comment_fails_rather_than_being_skipped`, `test_internal_allow_entry_no_longer_served_is_flagged`, `test_pathless_entry_that_gained_a_comment_is_flagged` | an unresolvable variant fails; a carve-out naming nothing real is flagged stale |

Regression: §2.2 proves the pre-existing OpenAPI bundle + lint still pass. §3's `test_head_is_clean` is a standing regression guard for the exact drift this workstream fixes. Idempotency/replay: N/A — no retry semantics.

The §1 tests are greps, not a harness: the docs live in a separate repository with no test runner, so they run as the R1/R2 rubric commands on the docs branch. This is an honest bound — no standing CI gate exists in this repo for the docs repo, and §3's gate covers only the OpenAPI surface.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Phantom continuation claims gone (§1) | `grep -rn "chunk_chain_escalate_human\|10 continuations\|continuation cap\|continuation:" ~/Projects/docs/concepts/context-lifecycle.mdx ~/Projects/docs/fleets/authoring.mdx ~/Projects/docs/fleets/running.mdx ~/Projects/docs/cli/agentsfleet.mdx` | no output | P0 | |
| R2 | Auto re-enqueue claim gone (§1) | `grep -rn "re-enqueues the same event" ~/Projects/docs/` | no output | P0 | |
| R3 | admin/models documented (§2) | `grep -c "/v1/admin/models" public/openapi.json` | output ≥ 2 | P0 | ✅ `4` |
| R4 | OpenAPI bundle + all coverage checks pass (§2/§3) | `make check-openapi` | exit 0 | P0 | ✅ `✓ [openapi] Bundle + lint + error-schema + url-shape + route-coverage all green` |
| R5 | Coverage gate catches an undocumented route (§3) | `python3 scripts/check_openapi_route_coverage.py` after deleting a served path from a temp `openapi.json` copy | exit ≠ 0, path named | P0 | ✅ `UNDOCUMENTED ROUTE: /v1/admin/models  (routes.zig: admin_models)`, exit=1 |
| R6 | Coverage gate wired into check-openapi (§3) | `grep -c check_openapi_route_coverage make/quality.mk` | output ≥ 1 | P0 | ✅ `2` |
| R7 | This workstream's own diff stays inside its Files Changed (§2/§3) | `git diff --name-only origin/main \| grep -E 'openapi\|scripts/\|make/' \| grep -vE 'admin-models\.yaml\|openapi/root\.yaml\|public/openapi\.json\|check_openapi_route_coverage(_test)?\.py\|make/quality\.mk'` | no output | P0 | |
| R8 | Every ceiling named in the docs has a code owner (§1) | `grep -rn "budget_breach\|timeout_kill\|renewal_terminate\|oom_kill" src/ \| grep -v test \| wc -l` | output ≥ 4 | P0 | |
| S1 | Lint clean | `make lint` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S3 | No oversize hand-written source file | `git diff --name-only origin/main \| grep -vE '\.md$\|^public/openapi\.json$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**R7 note (amended Jul 10 2026):** the branch also carries M122_005's enforcement code (`src/**`) and both specs (`docs/v2/**`), so R7 narrows to the OpenAPI/scripts/make surface this workstream owns; M122_005 grades its own Files-Changed scope. **S3 note:** `public/openapi.json` is excluded — it is a `redocly bundle` artefact (6,946 lines) and §2 mandates regenerating it, so the row as originally written was unsatisfiable by construction.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; §1 rewrites two docs pages in place, §2/§3 create new files.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/prose | Grep | Expected |
|-----------------------|------|----------|
| `chunk_chain_escalate_human` (phantom failure label in docs) | `grep -rn "chunk_chain_escalate_human" ~/Projects/docs` | 0 matches |
| auto re-enqueue continuation prose | `grep -rn "re-enqueues the same event" ~/Projects/docs` | 0 matches |
| phantom `continuation:` actor value | `grep -rn "actor=continuation\|continuation:" ~/Projects/docs --exclude=changelog.mdx` | 0 matches |

**`changelog.mdx` carve-out.** `changelog.mdx:1660` carries `actor=continuation:<original_actor>` in a shipped entry. Shipped changelog entries are historical record — `docs/CHANGELOG_VOICE.md` archives history, never rewrites it — so the repo-wide grep in this table excludes it. The correction is published forward, in the new `<Update>` this workstream appends at CHORE(close), which states plainly that continuation chains never shipped. Rewriting the old entry would erase the evidence that the claim was ever made.

## Out of Scope

- Implementing an actual continuation-chain / context-exhaustion re-enqueue in `service_report.zig` — the decision here is docs-correct-to-code, not build-the-feature; a future spec may add it.
- Removing the ingest-only `EVENT_TYPE_CONTINUATION` type from `event_rows.zig` — a separate dead-machinery call, not this doc/parity workstream.
- Adding the admin/models routes to the docs-repo navigation — they are an internal platform-admin surface; OpenAPI parity is the parity fix, and public-nav placement is a separate editorial decision.
- Documenting or allowlisting-then-narrowing every currently-internal control-plane route beyond what the coverage gate needs to pass at HEAD.
- Adding the four missing served-path comments to `routes.zig` so `PATHLESS_VARIANT_PATHS` can be emptied — the durable fix for §3's one manual table, deferred to keep this workstream's in-repo diff inside R7.
- **Per-fleet budget enforcement itself** — the code that makes §1's budget prose true is M122_005, a sibling workstream on this same branch and PR. This spec only *describes* the enforced behaviour; it does not implement it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reading the context-lifecycle page sees exactly what happens when a long run runs out of context (it ends as `fleet_error`, state checkpointed) and stops waiting for a "10th continuation" ceiling that was never real; an API integrator browsing the reference finds the admin model routes they can already call.
2. **Preserved user behaviour** — every served endpoint keeps its exact behaviour; the daemon is not touched. Existing OpenAPI consumers see only additions (the admin/models paths), never a changed or removed one.
3. **Optimal-way check** — the most direct fix: docs describe the code (no feature build), OpenAPI gains one mirrored file, and one script closes the parity gap for good. No larger refactor is implied.
4. **Rebuild-vs-iterate** — iterate: three contained slices (two doc pages, one OpenAPI file + bundle, one gate), no redesign; determinism is unaffected.
5. **What we build** — a truthful context-lifecycle rewrite, `admin-models.yaml` + its bundle entry, and `check_openapi_route_coverage.py` wired into `check-openapi`.
6. **What we do NOT build** — an actual continuation feature; removal of the ingest-only continuation type; docs-nav entries for internal admin routes (all in Out of Scope).
7. **Fit with existing features** — the coverage gate compounds the existing `check-openapi` gate family (URL shape, error schema, route-registration freshness); it must not destabilize the bundle step — it runs after the bundle, read-only.
8. **Surface order** — docs-repo-first for §1 (own-branch flow), in-repo API/tooling for §2/§3; no CLI or UI surface.
9. **Dashboard restraint** — N/A — no UI surface; the only "control" added is a CI gate, which surfaces nothing to end users.
10. **Confused-user next step** — a reader who expected a continuation cap now reads the accurate `fleet_error` termination and its checkpoint behaviour inline; an integrator who called an undocumented admin route now finds it in the reference. No ticket required.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections mirroring the two findings plus the durable recurrence fix — one docs correction (F22), one OpenAPI parity add (F17), one coverage gate (F17's root cause). Each is independently testable and DONE-markable.
- **Alternatives considered:** (a) for F22, build the documented continuation feature to match the docs instead — rejected: it invents a runtime capability from a doc bug and trades determinism/scope for no requested outcome (Out of Scope as a possible future spec); (b) for F17, document admin/models without the coverage gate — rejected: it fixes the instance and leaves the class, so the next undocumented route drifts identically; the gate is the load-bearing durable fix.
- **Patch-vs-refactor verdict:** this is a **patch** — two doc pages corrected to code, one mirrored OpenAPI file, and one small gate script in an existing gate family; nothing is restructured.

## Discovery (consult log)

- **Consults** — Architecture (`dispatch/name_architecture.md`, Jul 10 2026): read `docs/architecture/billing_and_provider_keys.md` before naming the budget flow. It defines posture as tenant-scoped and says nothing about per-fleet dollar ceilings, so M122_005 introduces that concept and owns the corresponding architecture-doc diff. Gate-flag triage: none fired (no Zig, no schema, no UI in this workstream).

- **Golden-path walk (Jul 10 2026)** — walking §1's end-to-end path before editing surfaced three facts that refute the spec as authored:
  1. **Context exhaustion does not produce `fleet_error`.** `runner_progress.zig:247` only *observes* `prompt_tokens / context_cap_tokens` and logs `chunk_threshold_breached`; its comment states the runtime "cannot force a chunk." A fleet that wraps up voluntarily exits 0 → `exit_ok: true` → status `processed`. `fleet_error` is reserved for the ten `FailureClass` kills. Dimension 1.2 and its test were amended; writing `fleet_error` would have swapped one falsehood for another.
  2. **`budget_breach` is a phantom label** — zero hits across `src/` and `cli/`, yet named in three docs pages.
  3. **Per-fleet budgets are never enforced.** `FleetConfig.budget` (`config_types.zig:149`) is read by exactly one thing in the repo: `config_parser_test.zig`. `daily_dollars`/`monthly_dollars` are required in every `TRIGGER.md`, parsed, validated, and then ignored. Combined with (1) and the continuation phantom, all three ceilings named in `context-lifecycle.mdx:44`'s "Will it run forever?" answer are fiction.

- **Decisions (Indy, Jul 10 2026)** — a cost-safety boundary is not an agent-unilateral call, so both were escalated:
  - *Budget gap:* **build the enforcement**, rather than delete the promise from the docs. → M122_005.
  - *Enforcement points:* **pre-run gate + mid-run `/renew`, with a new `budget_breach` label** — so an in-flight run is stopped, matching the behaviour the docs already promise, and triage can tell a budget stop from a credit stop.
  - *Spec shape:* **a new spec (M122_005) on this same worktree and PR**, not a wholesale amendment of M122_001. Rationale: M122_001 keeps its three Sections and its rubric; money-handling code gets its own Failure Modes, Invariants, and Test Specification; and the docs rewrite cannot merge ahead of the code it describes.
  - *Docs sweep width:* **every affected page except `changelog.mdx`** (history is append-only; the correction rides the new `<Update>`).

- **Metrics review** — unchanged for this workstream (no product or operator signal added, renamed, or removed). M122_005 adds the `budget_breach` terminal label and its `metrics_runner` failure bucket; that surface is reviewed in M122_005's own Metrics table.

- **Skill-chain outcomes** — `/write-unit-test` + `/review` + `kishore-babysit-prs`: pending (run at VERIFY / CHORE(close)).

- **Deferrals** — one, agent-proposed and requiring an Indy ack before CHORE(close): adding the four missing served-path comments to `routes.zig` so `PATHLESS_VARIANT_PATHS` can be emptied (Out of Scope, kept out to hold R7's diff bound). No other item is deferred; nothing in this spec is claimed complete that is not.
