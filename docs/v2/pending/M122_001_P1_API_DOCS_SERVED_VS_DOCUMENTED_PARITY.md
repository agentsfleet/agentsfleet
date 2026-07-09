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
**Status:** PENDING
**Priority:** P1 — user-facing docs promise a non-existent safety ceiling (a "10 continuation" cap) and a non-existent failure label (`chunk_chain_escalate_human`); the served admin model-catalogue routes ship undocumented. Neither crashes, so this is doc/parity truth-in-advertising, not a runtime bug.
**Categories:** API, DOCS
**Batch:** B1 — independent; touches no source shared with any pending workstream.
**Branch:** {added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, Jul 02 2026 `fleet-wide-refactor-audit`; both findings re-verified against HEAD `7a06fb5d` on Jul 09 2026 by the `audit-open-items-recheck` workflow, each surviving an adversarial refutation pass).
**Canonical architecture:** `docs/REST_API_DESIGN_GUIDELINES.md` §1/§7 (URL shape + route-registration freshness — the coverage gate in §3 extends that family); `docs/SCHEMA_CONVENTIONS.md` is untouched.

---

## Overview

**Goal (testable):** the docs-repo context-lifecycle page describes what the daemon actually does on context exhaustion (a run terminates as `fleet_error` with a checkpoint — no automatic re-enqueue, no continuation cap, no `chunk_chain_escalate_human` label); the four served `/v1/admin/models` routes appear in `public/openapi.json`; and a mechanical Continuous Integration (CI) gate fails whenever any served public route lacks a documented (or explicitly allowlisted) OpenAPI entry.
**Problem:** two independently-verified drifts. (1) `concepts/context-lifecycle.mdx` and `fleets/authoring.mdx` document a continuation-chain feature — the runtime re-enqueues the exhausted event, a chain caps at 10 continuations, the 11th is labelled `chunk_chain_escalate_human` — that does not exist: `chunk_chain`/`escalate_human`/`MAX_CONTINUATION` return zero hits across `src/` and `cli/`, and `service_report.zig` finalizes a non-processed run without ever re-enqueuing. A reader tunes knobs and waits for a ceiling that will never fire. (2) The platform-admin model catalogue Create/Read/Update/Delete (CRUD) — GET+POST `/v1/admin/models`, PATCH+DELETE `/v1/admin/models/{uid}` — is live-served but absent from every OpenAPI source, and nothing prevents the next served route from drifting the same way.
**Solution summary:** rewrite the two docs-repo pages to describe the real terminal behaviour and delete the phantom-feature claims (docs are wrong, code is right — no runtime change); add `public/openapi/paths/admin-models.yaml` mirroring the sibling `admin.yaml` platform-keys shape, wire it into the bundle, and regenerate `public/openapi.json`; and add `scripts/check_openapi_route_coverage.py` to `make check-openapi` so a served-route-without-a-documented-entry fails CI from now on.

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
| `~/Projects/docs/concepts/context-lifecycle.mdx` | EDIT (separate repo, own branch) | rewrite the continuation-chain + "Will it run forever?" claims to the real `fleet_error`-termination behaviour; drop the 10-cap and `chunk_chain_escalate_human` label |
| `~/Projects/docs/fleets/authoring.mdx` | EDIT (separate repo, own branch) | correct the `<Note>` (line 154) claiming the runtime auto re-enqueues `actor=continuation:<original_actor>` |
| `public/openapi/paths/admin-models.yaml` | CREATE | document GET/POST `/v1/admin/models` + PATCH/DELETE `/v1/admin/models/{uid}`, mirroring `admin.yaml` |
| `public/openapi/root.yaml` | EDIT | register the two new paths as `$ref`s into `admin-models.yaml` |
| `public/openapi.json` | EDIT | regenerated bundle (`redocly bundle`) now carrying the admin/models paths |
| `scripts/check_openapi_route_coverage.py` | CREATE | new gate: every served public `/v1` route is documented in `openapi.json` or in a justified allowlist |
| `make/quality.mk` | EDIT | wire `check_openapi_route_coverage.py` into the `check-openapi` target |

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

### §1 — Rewrite the context-lifecycle docs to the real termination behaviour

The daemon does not implement continuation chains. On a context-exhausted (non-processed) run, `service_report.zig:finalize()` marks the event `fleet_error`, persists a granular `failure_label`, checkpoints session state, `XACK`s, and releases affinity — it never re-enqueues a follow-up event; the `EVENT_TYPE_CONTINUATION = "continuation"` ingest type has no production producer. Rewrite `context-lifecycle.mdx` (the "Context full" row, the "Continuation chains" section, the "Will it run forever?" answer) and the `authoring.mdx:154` `<Note>` to describe that a run which approaches the context cap ends with a checkpoint and terminates as `fleet_error`; there is no automatic continuation, no 10-cap, and no `chunk_chain_escalate_human` label. **Implementation default:** establish the exact terminal status/label and the treatment of every knob from `service_report.zig`/`event_rows.zig` BEFORE editing — describe only mechanisms with a grep-provable code backing; a knob or ceiling with no code owner is deleted, not softened. Runs on the `~/Projects/docs` own-branch flow (branch off a clean `main`; never commit onto a dirty `main`) — no docs-repo edit happens during authoring.

- **Dimension 1.1** — after the rewrite, `chunk_chain_escalate_human`, "10 continuations", and "continuation cap" appear zero times in `context-lifecycle.mdx` and `authoring.mdx` → Test `test_docs_no_phantom_continuation`
- **Dimension 1.2** — both pages describe the real terminal outcome (context exhaustion → `fleet_error` + checkpoint, no automatic re-enqueue): the phrase "re-enqueues the same event" is gone and `fleet_error` termination is present → Test `test_docs_describe_fleet_error_termination`
- **Dimension 1.3** — every tuning knob the rewritten page still names resolves to a real identifier under `src/` (e.g. `context_cap_tokens`); a knob with no code backing is removed → Test `test_docs_knobs_have_code_backing`

### §2 — Document the served admin model-catalogue routes

The four `/v1/admin/models` routes are live-served (registered in `routes.zig`, dispatched in `route_table_invoke.zig`) but absent from every OpenAPI source, while the sibling `/v1/admin/platform-keys` is documented. Add `admin-models.yaml` covering GET+POST `/v1/admin/models` and PATCH+DELETE `/v1/admin/models/{uid}`, register both paths in `root.yaml`, and regenerate `openapi.json`. **Implementation default:** mirror `admin.yaml` for operation ids, tags (`Admin`), and the shared `Error` response `$ref`; read the request/response shapes off `handlers/admin/model_caps_admin.zig` so the documented bodies match what the handlers actually accept and emit.

- **Dimension 2.1** — `public/openapi.json` contains path items for `/v1/admin/models` and `/v1/admin/models/{uid}` → Test `test_openapi_documents_admin_models`
- **Dimension 2.2** — the bundle is clean: `admin-models.yaml` has no dangling `$ref` and passes Redocly lint → Test existing `make check-openapi` green (`redocly bundle` + `lint`)
- **Dimension 2.3** — the documented verbs equal the served verbs (GET, POST on the collection; PATCH, DELETE on the item) → Test `test_admin_models_verbs_match_routes`

### §3 — Route-coverage gate (the durable recurrence fix)

`check_openapi_url_shape.py` and `check_openapi_errors.py` only validate paths already in `openapi.json`; nothing asserts that every served route is present there — which is exactly how admin/models drifted. Add `check_openapi_route_coverage.py`: enumerate the daemon's served public `/v1` routes from the `routes.zig` registry, and fail if any is neither documented in `openapi.json` nor in a small allowlist of deliberately-internal control-plane routes (runner leases, service report, internal webhook receive), each carrying a one-line justification. **Implementation default:** derive served paths from `routes.zig` (the canonical registry), normalizing `{param}` segments to the OpenAPI template form; the allowlist mirrors the URL-shape script's justified-carve-out style. Wire it as a fourth step in `check-openapi`.

- **Dimension 3.1** — run against a temporary `openapi.json` with a served, non-allowlisted route removed → the script exits non-zero and names the missing path → Test `test_route_coverage_flags_undocumented`
- **Dimension 3.2** — run at HEAD after §2 (admin/models documented; internal routes allowlisted) → the script exits 0 → Test `test_route_coverage_clean_at_head`
- **Dimension 3.3** — `check-openapi` invokes the coverage script → Test `test_check_openapi_runs_coverage` (grep the make target)

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
3. The daemon implements no continuation re-enqueue / cap / escalation — enforced by the existing production code (`chunk_chain`/`escalate_human`/`MAX_CONTINUATION` grep to zero across `src/` + `cli/`); §1 corrects the docs to match, and the §1.1 grep keeps the phantom strings out of the two pages on the branch.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | this workstream edits docs, OpenAPI sources, and a CI script; it adds, renames, and removes no event | unchanged | unchanged | existing suites stay green |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit (grep, docs repo) | `test_docs_no_phantom_continuation` | `grep -c "chunk_chain_escalate_human\|10 continuations\|continuation cap"` over both pages → 0 |
| 1.2 | unit (grep, docs repo) | `test_docs_describe_fleet_error_termination` | "re-enqueues the same event" absent; `fleet_error` termination language present on both pages |
| 1.3 | unit (grep, cross-repo) | `test_docs_knobs_have_code_backing` | each knob identifier named in the rewritten page matches a token under `src/`; unbacked knobs removed |
| 2.1 | unit (bundle assertion) | `test_openapi_documents_admin_models` | `openapi.json` `paths` contains `/v1/admin/models` and `/v1/admin/models/{uid}` |
| 2.2 | integration (regression) | existing `make check-openapi` | `redocly bundle` + `lint` exit 0 with the new YAML wired in |
| 2.3 | unit | `test_admin_models_verbs_match_routes` | documented verbs = {GET, POST} on the collection, {PATCH, DELETE} on the item |
| 3.1 | unit (negative) | `test_route_coverage_flags_undocumented` | served route removed from a temp spec → script exit ≠ 0, path named in output |
| 3.2 | unit (positive) | `test_route_coverage_clean_at_head` | at HEAD post-§2 → script exit 0 |
| 3.3 | unit (grep) | `test_check_openapi_runs_coverage` | `check-openapi` target invokes `check_openapi_route_coverage.py` |

Regression: §2.2 proves the pre-existing OpenAPI bundle + lint still pass. Idempotency/replay: N/A — no retry semantics.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Phantom continuation claims gone (§1) | `grep -rn "chunk_chain_escalate_human\|10 continuations\|continuation cap" ~/Projects/docs/concepts/context-lifecycle.mdx ~/Projects/docs/fleets/authoring.mdx` | no output | P0 | |
| R2 | Auto re-enqueue claim gone (§1) | `grep -rn "re-enqueues the same event" ~/Projects/docs/concepts/context-lifecycle.mdx ~/Projects/docs/fleets/authoring.mdx` | no output | P0 | |
| R3 | admin/models documented (§2) | `grep -c "/v1/admin/models" public/openapi.json` | output ≥ 2 | P0 | |
| R4 | OpenAPI bundle + all coverage checks pass (§2/§3) | `make check-openapi` | exit 0 | P0 | |
| R5 | Coverage gate catches an undocumented route (§3) | `python3 scripts/check_openapi_route_coverage.py` after deleting a served path from a temp `openapi.json` copy | exit ≠ 0, path named | P0 | |
| R6 | Coverage gate wired into check-openapi (§3) | `grep -c check_openapi_route_coverage make/quality.mk` | output ≥ 1 | P0 | |
| R7 | Diff stays inside Files Changed (§2/§3) | `git diff --name-only origin/main \| grep -vE 'admin-models\.yaml\|openapi/root\.yaml\|public/openapi\.json\|check_openapi_route_coverage\.py\|make/quality\.mk'` | no output | P0 | |
| S1 | Lint clean | `make lint` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S3 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; §1 rewrites two docs pages in place, §2/§3 create new files.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/prose | Grep | Expected |
|-----------------------|------|----------|
| `chunk_chain_escalate_human` (phantom failure label in docs) | `grep -rn "chunk_chain_escalate_human" ~/Projects/docs` | 0 matches |
| auto re-enqueue continuation prose | `grep -rn "re-enqueues the same event\|actor=continuation" ~/Projects/docs` | 0 matches |

## Out of Scope

- Implementing an actual continuation-chain / context-exhaustion re-enqueue in `service_report.zig` — the decision here is docs-correct-to-code, not build-the-feature; a future spec may add it.
- Removing the ingest-only `EVENT_TYPE_CONTINUATION` type from `event_rows.zig` — a separate dead-machinery call, not this doc/parity workstream.
- Adding the admin/models routes to the docs-repo navigation — they are an internal platform-admin surface; OpenAPI parity is the parity fix, and public-nav placement is a separate editorial decision.
- Documenting or allowlisting-then-narrowing every currently-internal control-plane route beyond what the coverage gate needs to pass at HEAD.

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

- **Consults** — Architecture / Legacy-Design / gate-flag triage: (empty at creation).
- **Metrics review** — (empty at creation).
- **Skill-chain outcomes** — (empty at creation).
- **Deferrals** — (empty at creation).
