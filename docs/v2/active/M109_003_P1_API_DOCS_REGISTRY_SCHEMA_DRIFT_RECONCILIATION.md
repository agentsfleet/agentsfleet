<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M109_003: Reconcile 4 confirmed drifts between schema/error-registry and their claimed invariants

**Prototype:** v2.0.0
**Milestone:** M109
**Workstream:** 003
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Test Baseline:** unit=2272 integration=243 (recorded at CHORE(open), Jul 03, 2026, via `make _lint_zig_test_depth` on `main` @ `bb5bfc8b`)
**Priority:** P1 — none of these crash today, but each is a claimed invariant (a foreign key, a "single source of truth" comment, a hint's field list, a live-error-codes list) that is currently false, and each misleads either a future implementer or an API client.
**Categories:** API DOCS
**Batch:** B1 — independent of M109_001/002/004; no shared files.
**Branch:** feat/m109-003-registry-schema-drift
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, fleet-wide-refactor-audit `Workflow` run `wf_8ec169f4-8e4`, each finding independently re-verified against current source before this spec was drafted, Jul 02, 2026).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/SCHEMA_CONVENTIONS.md` (§4's FK convention is inferred from sibling tables, since the doc has no dedicated FK section — this workstream's §1 is itself evidence that section should exist; flagged in Discovery, not added here to keep this a patch).

---

## Implementing agent — read these first

1. `schema/018_fleet_events.sql:17` — `fleet_id UUID NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE` is the correct sibling shape §1 must match in `schema/022_fleet_runner_leases.sql` and `schema/023_fleet_runner_affinity.sql`.
2. `src/agentsfleetd/errors/error_entries.zig` — the single generation source for `docs/api-reference/error-codes.mdx` (in `~/Projects/docs`); §2 corrects the doc against this file, §3 corrects this file's hint text against `credential_probe.zig`.
3. `src/agentsfleetd/state/credential_probe.zig:84-100` — `probeSelfManagedCredential`'s actual validation logic (provider+model always required, `api_key` conditional on provider) that §3's hint text must accurately describe.
4. `src/agentsfleetd/http/handlers/api_keys/list.zig:39-45,60-63` — the one existing correct consumer of `pagination.zig`'s shared helper, and its fail-closed 400 on malformed input; §4 uses this as the reference for what "consumes the shared helper" actually looks like, to judge whether `fleets/list.zig` should be corrected to match it or the doc comment should be corrected to stop claiming it already does.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Reconcile schema FKs, error-registry docs, and a stale "single source of truth" comment with reality
- **Intent (one sentence):** A foreign key, a hint string, and a doc comment each currently claim something the code doesn't do — after this PR, all three are true.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

  **Restatement (Orly, Jul 03, 2026):** Three artifacts currently claim something the code/registry doesn't do; after this PR each is true. Concretely: (§1) two `fleet.*` tables gain a `fleet_id → core.fleets(id) ON DELETE CASCADE` FK so runner-lease/affinity rows can't outlive their fleet; (§2) two genuinely-dead error-code rows leave the docs; (§3) the `UZ-PROVIDER-003` hint stops lying about `api_key` being unconditional; (§4) the `pagination.zig` comment stops overclaiming its consumer set. This matches the Intent.

  **ASSUMPTIONS I'M MAKING (all cross-checked against current source at PLAN — this is an agent-generated spec, so every finding was re-verified; two required amending the spec, see Discovery):**
  1. **§1 blast radius is 15 files, not 2.** The spec's Files-Changed table lists only `022`/`023`. Re-verification (Discovery §1) found **13 integration-test files** that INSERT lease/affinity rows against a synthetic `fleet_id` with no `core.fleets` row — each fails the new FK. Fixing them (add `base.seedFleet(...)` + teardown before the insert) is in-scope by the spec's own Failure Modes row ("fixture must be corrected before this lands"); the spec merely under-counted. Proceeding to fix all 13.
  2. **§1's cross-schema FK is architecturally sound.** `schema/021_fleet_runners.sql:39` already carries `tenant_id → core.tenants(id) ON DELETE CASCADE` — a `fleet.* → core.*` FK-with-cascade is an established, deliberate pattern; the `fleet`/`core` trust boundary is about runner *identity* not living in `core`, not about forbidding referential FKs. Consult resolved in favour of the spec default (Discovery §1 consult).
  3. **§2 keeps `UZ-AUTH-021`, contra the spec's "delete 3 rows".** Commit `f64e20c` ("keep retired UZ-AUTH-021 visible", Kishore, Jul 03 — one day *after* this spec was drafted) deliberately made `UZ-AUTH-021` a struck-through historical row. Deleting it would revert a merged decision. Only the two genuinely-dead live-looking rows (`UZ-AUTH-009`, `UZ-AUTH-010`) are removed. `UZ-SLK-021` (line 182) is intentional gap-note prose and stays. Spec §2 amended.
  4. **§2's verification method (E8 / Dimension 2.1) was broken and is corrected.** The spec's grep sourced producers from `error_entries.zig` only — missing `error_entries_runtime.zig` (43 false positives) and named-constant entries like `e(S_UZ_INTERNAL_003, …)` (falsely flags the live `UZ-INTERNAL-003`). Corrected diff unions both entry files and excludes deliberately-documented historical/skipped codes.
  5. **§4 has 3 real consumers, not 1.** `parsePageParams` is imported by `api_keys/list.zig`, `fleet/runners_list.zig`, and `fleet/runner_events.zig`; `fleets/list.zig` genuinely does not (own keyset scheme). The corrected comment names all three and notes the keyset divergence. Default direction confirmed: correct the comment, no convergence.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an operator deletes a fleet in a test/staging environment and the runner-affinity/lease rows for it are cleaned up automatically (FK cascade) instead of becoming orphaned rows nobody notices; an API client reads a `UZ-PROVIDER-003` error's hint text and it exactly matches what the API actually requires, so they fix their payload on the first try; a docs reader never hits a dead error-code anchor.
2. **Preserved user behaviour** — every currently-passing credential-validation request keeps passing (the hint text changes, not the validation logic); every currently-working fleet/runner-affinity flow is unaffected until an actual fleet delete happens (which the missing FK doesn't currently block, it just fails to cascade).
3. **Optimal-way check** — the unconstrained-optimal fix for §1 is exactly this: add the FK. No larger schema redesign is implied.
4. **Rebuild-vs-iterate** — iterate. Each of the 4 findings is a one-file (or one-doc) correction against an existing, otherwise-correct pattern.
5. **What we build** — FK additions on `fleet_id` in `schema/022_fleet_runner_leases.sql` and `schema/023_fleet_runner_affinity.sql`; removal of 3 dead rows from `docs/api-reference/error-codes.mdx`; corrected hint text in `error_entries.zig`'s `UZ-PROVIDER-003` entry; a corrected doc comment on `pagination.zig` (or, if Discovery's consult decides fleets should converge instead — see §4 — a shared limit-parsing helper).
6. **What we do NOT build** — a general schema-wide FK audit beyond the two tables named (Discovery flags `schema/021_fleet_runners.sql` as already-correct, not touched); a rewrite of `docs/SCHEMA_CONVENTIONS.md` to add a formal FK-convention section (flagged as a follow-up, not blocking this patch).
7. **Fit with existing features** — §1's FK addition must not break any existing test/seed data that inserts `fleet_runner_leases`/`fleet_runner_affinity` rows for a `fleet_id` that doesn't exist in `core.fleets` (pre-v2.0.0 teardown-rebuild model per `SCHEMA_CONVENTIONS.md:7` — verify no such rows in seed fixtures before adding the constraint).
8. **Surface order** — API/schema-first for §1/§3/§4; docs-repo-only for §2 (own-branch flow per `AGENTS.md` Operational defaults — `~/Projects/docs` is a separate repo).
9. **Dashboard restraint** — N/A, no UI surface.
10. **Confused-user next step** — §2/§3 are themselves the "confused user" fix (accurate docs/hints are the self-serve move); §1's confused user is a future developer wondering why runner-affinity rows outlive their fleet — the FK is the enforced answer.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NLR (fix the doc comment that's actually wrong, don't just patch around it).
- **`docs/SCHEMA_CONVENTIONS.md`** — §1 touches `schema/*.sql`; pre-v2.0.0 teardown-rebuild model means this is an inline DDL edit to the existing migration files, not a new `ALTER TABLE` migration (per `SCHEMA_CONVENTIONS.md:7`).
- **`dispatch/write_zig.md`** — §3/§4 touch `*.zig` (`error_entries.zig`, `pagination.zig` or `fleets/list.zig`) if §4's Discovery consult picks convergence over doc-correction.

Cross-repo note: §2 edits `~/Projects/docs/api-reference/error-codes.mdx` — per `AGENTS.md` Operational defaults, verify `cd ~/Projects/docs && git status` shows `HEAD == main` before editing; commit on a fresh `chore/m109-003-error-codes-drift` branch off `main`, not on this repo's feature branch.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — §1 | this is a `DROP`/`ALTER`-adjacent edit to existing migration files (adding a constraint) under the pre-v2.0.0 teardown-rebuild model; confirm no seed fixture violates the new FK before landing. |
| ERROR REGISTRY | yes — §3 | hint text edit only, no code identifier (`UZ-PROVIDER-003`) changes; the gate's concern (every error code has a producer + doc entry) is unaffected. |
| ZIG GATE | yes — §3, §4 | cross-compile both linux targets after the hint-text/comment edits. |
| File & Function Length / UFS / PUB / UI / DESIGN TOKEN / LOGGING / LIFECYCLE | no | none of these four findings touch a pub surface, add a literal, or touch UI/logging. |

---

## Overview

**Goal (testable):** `schema/022_fleet_runner_leases.sql` and `schema/023_fleet_runner_affinity.sql`'s `fleet_id` columns declare `REFERENCES core.fleets(id) ON DELETE CASCADE`; `docs/api-reference/error-codes.mdx` lists zero error codes that have no producer in `error_entries.zig`; `UZ-PROVIDER-003`'s hint text matches `credential_probe.zig`'s actual field requirements; `pagination.zig`'s header comment accurately describes its actual consumer(s).

**Problem:** A fleet delete does not cascade to its runner-lease/runner-affinity rows (no FK), silently orphaning control-plane state. `docs/api-reference/error-codes.mdx` documents `UZ-AUTH-009`/`010`/`021` as live API responses when they were deleted from the registry in commit `8f81c356`, directly contradicting the doc's own claim (lines 28-30) that dead codes are removed. `UZ-PROVIDER-003`'s hint tells API clients `api_key` is always required, when `credential_probe.zig` makes it optional for an `openai-compatible` provider — clients following the hint literally will send an unnecessary field or, worse, assume a required-but-actually-optional field is the reason for an unrelated failure. `pagination.zig`'s comment claims it's the shared source of truth for "the fleet + api-key list parsers," but `fleets/list.zig` never calls it — only `api_keys/list.zig` does.

**Solution summary:** Add the missing FK to the two runner-scoped tables (mirroring `schema/018_fleet_events.sql`'s existing pattern). Delete the three dead rows from the external docs repo's error-codes table. Rewrite the `UZ-PROVIDER-003` hint to match the actual conditional-`api_key` rule (already correctly phrased once, in the external docs repo — copy that phrasing back). Resolve the `pagination.zig` comment's overclaim either by correcting the comment (fleets' cursor-based scheme is legitimately different) or by converging `fleets/list.zig` onto a shared helper — Discovery records which, before EXECUTE.

---

## Prior-Art / Reference Implementations

- **Schema (§1)** → `schema/018_fleet_events.sql:17` (`fleet_id UUID NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE`) + `docs/SCHEMA_CONVENTIONS.md`. **Alignment:** exact FK shape and ON DELETE behavior match 4 of 6 sibling `fleet_id`-bearing tables. **Divergence:** none.
- **Docs (§2)** → `docs/api-reference/error-codes.mdx`'s own `<Note>` (lines 28-30) already states the intended process ("codes that no longer have a producer have been removed"); this finding is that process not being followed for one commit. **Alignment:** just execute the doc's own stated process. **Divergence:** none.
- **API (§3)** → `docs/api-reference/error-codes.mdx:110` (external docs repo) already has the correct phrasing ("`api_key` is required for a named provider, optional for an `openai-compatible` endpoint") — copy it back into `error_entries.zig`'s hint. **Alignment:** exact text reuse. **Divergence:** none.
- **API (§4)** → `src/agentsfleetd/http/handlers/api_keys/list.zig:39-45,60-63` — the one existing correct consumer of `pagination.parsePageParams`, fail-closed on malformed input. **Alignment:** whichever direction Discovery resolves §4 in, this is the reference for "correct consumer" or "correct fail-closed limit parser" respectively.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/022_fleet_runner_leases.sql` | EDIT | add `REFERENCES core.fleets(id) ON DELETE CASCADE` to `fleet_id`. |
| `schema/023_fleet_runner_affinity.sql` | EDIT | same FK addition on its `fleet_id` column. |
| **13 integration-test files** (`fleet/*_test.zig`, `http/handlers/runner/*_test.zig`) | EDIT | **AMENDED at PLAN:** each inserts a lease/affinity row against a synthetic `fleet_id` with no `core.fleets` row and fails the new FK; add `base.seedFleet(...)` + teardown before the insert. Full list in Discovery §1. In-scope per this spec's own Failure Modes ("fixture must be corrected before this lands"). |
| stale no-FK comments (`account_teardown.zig`, `integration_session_continuation_test.zig`, `event_lifecycle_integration_test.zig`, `account_teardown_test.zig`) | EDIT | RULE NLR touch-it-fix-it: comments that assert "no FK here" become false after §1; correct them. |
| `~/Projects/docs/api-reference/error-codes.mdx` | EDIT (separate repo, own branch) | **AMENDED at PLAN:** delete ONLY the `UZ-AUTH-009`/`010` rows (both producerless). `UZ-AUTH-021` kept per `f64e20c`; `UZ-SLK-021` prose kept. See §2 + Discovery §2. |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | rewrite `UZ-PROVIDER-003`'s hint text to match `credential_probe.zig`'s actual conditional `api_key` requirement. |
| `src/agentsfleetd/http/handlers/pagination.zig` | EDIT (default direction) | correct the header comment to accurately state its actual consumer(s), unless Discovery's consult resolves toward convergence instead (see §4). |
| `src/agentsfleetd/http/handlers/fleets/list.zig` | EDIT (only if Discovery resolves toward convergence) | `parseLimitFromQs` calls a new shared, fail-closed limit-parsing helper in `pagination.zig` instead of hand-rolling `std.fmt.parseInt` with a silent fallback. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four independent one-file (or one-doc) corrections sharing a workstream because they're the same class of defect (a claimed invariant that's currently false) at the same priority.
- **Alternatives considered:** for §4, converging `fleets/list.zig` onto `pagination.zig` was considered as the default instead of correcting the comment — rejected as the *default* because fleets' cursor+`limit` scheme is a structurally different, arguably correct-for-its-use-case pagination model (keyset, not page-number), and forcing convergence risks changing fleets' actual pagination semantics (a bigger, riskier change) just to make a comment true. The comment-correction default is the smaller, safer patch; convergence is the documented alternative if Discovery's consult decides the drift itself (not just the comment) is the bug.
- **Patch-vs-refactor verdict:** **patch**, all four.

---

## Sections (implementation slices)

### §1 — Add the missing `fleet_id` foreign key

Two runner-scoped tables (`022_fleet_runner_leases`, `023_fleet_runner_affinity`) declare `fleet_id UUID NOT NULL` with no FK, unlike 4 of 6 sibling `fleet_id`-bearing tables. **Implementation default:** `REFERENCES core.fleets(id) ON DELETE CASCADE`, matching `schema/018_fleet_events.sql` — because runner-lease/affinity rows are per-fleet control-plane state with no reason to outlive the fleet.

- **Dimension 1.1** — deleting a `core.fleets` row cascades to delete its `fleet.runner_leases` and `fleet.runner_affinity` rows → Test `test_fleet_delete_cascades_to_runner_lease_and_affinity_rows`.
- **Dimension 1.2** — inserting a `fleet.runner_leases`/`fleet.runner_affinity` row with a non-existent `fleet_id` is rejected by the DB → Test `test_runner_lease_affinity_reject_orphan_fleet_id`.

### §2 — Remove the genuinely-dead error-code rows from the external docs repo

> **AMENDED at PLAN (Jul 03, 2026).** The spec as drafted said "delete the three rows (65, 66, 85) for `UZ-AUTH-009`/`010`/`021`." Re-verification found only **two** rows are genuinely-dead live-looking rows; the third (`UZ-AUTH-021`) was deliberately kept by a *later*, merged decision, and a fourth code the naive check flags is intentional prose. Corrected scope below; full evidence in Discovery §2.

`docs/api-reference/error-codes.mdx` lists `UZ-AUTH-009` (line 65) and `UZ-AUTH-010` (line 66) as live table rows, but both have zero producers anywhere in the registry (superseded by `UZ-AUTH-022`, already documented at line 67) — contradicting the doc's own stated generation process (`<Note>`, line 29). **Implementation (corrected):** delete only these two rows.

**Explicitly NOT deleted** (contra the spec's original draft):
- `UZ-AUTH-021` (line 85) — commit `f64e20c` ("keep retired UZ-AUTH-021 visible", Jul 03, one day after this spec was drafted) deliberately made it a struck-through *historical* row. Deleting it reverts a merged decision → kept.
- `UZ-SLK-021` (line 182) — not a row; it is intentional prose explaining a deliberately-skipped number ("superseded by the generic `UZ-CONN-002`"). Kept.
- `UZ-INTERNAL-003` (line 46) — LIVE; the naive grep missed its named-constant entry `e(S_UZ_INTERNAL_003, …)` in `error_entries.zig`. Not touched.

- **Dimension 2.1** — `docs/api-reference/error-codes.mdx` contains zero live-looking rows for a code with no producer in the registry → Test `test_error_codes_doc_matches_registry`. **Corrected method:** diff the doc's code column against the union of `e("UZ-…")` / `e(S_UZ_…)` producers across **both** `error_entries.zig` *and* `error_entries_runtime.zig`, then subtract the deliberately-documented historical/skipped set (`UZ-AUTH-021`, `UZ-SLK-021` — struck-through rows and gap-note prose). Run as part of this workstream's verification, not a new CI gate (separate follow-up per Out of Scope).

### §3 — Correct `UZ-PROVIDER-003`'s hint text

The hint claims `api_key` is unconditionally required; `credential_probe.zig` makes it optional for `provider == "openai-compatible"`. **Implementation default:** copy the already-correct phrasing from the external docs repo (`error-codes.mdx:110`) back into `error_entries.zig`'s hint string.

- **Dimension 3.1** — `UZ-PROVIDER-003`'s hint text states `api_key` is required for a named provider and optional for `openai-compatible` → Test `test_uz_provider_003_hint_matches_validation_rule` (string-content assertion against the registry entry).

### §4 — Resolve `pagination.zig`'s "single source of truth" overclaim

The header comment claims fleets and api-keys share this parser; only api-keys does. **Implementation default:** correct the comment to name its actual single consumer accurately (api-keys list) and explicitly note fleets uses a separate, intentionally different cursor+limit scheme — because forcing convergence risks changing fleets' pagination semantics, a larger change than this workstream's patch scope warrants. Discovery records the consult confirming this direction (or the convergence alternative) before EXECUTE.

- **Dimension 4.1** — `pagination.zig`'s header comment names only its actual current consumer(s), no longer claims parity with `fleets/list.zig` unless convergence was chosen → Test `test_pagination_header_comment_matches_actual_consumers` (or, if convergence chosen, `test_fleets_list_uses_shared_limit_parser` asserting fail-closed 400 on malformed `limit`, mirroring `api_keys/list.zig`'s existing test).

---

## Metrics & Observability

`not applicable — no product/operator signal changes`. All four sections correct a schema constraint, two docs, and a hint/comment string; none add or change an event.

---

## Interfaces

```
Schema: fleet.runner_leases.fleet_id and fleet.runner_affinity.fleet_id gain
REFERENCES core.fleets(id) ON DELETE CASCADE. No column type/name change;
existing INSERTs with a valid fleet_id are unaffected.

UZ-PROVIDER-003 error response: `code` and HTTP status unchanged; only the
`detail`/hint string content changes.

pagination.zig: no function signature change under the default (comment-only)
direction. If Discovery resolves toward convergence instead, a new
fail-closed limit-parsing helper is added to pagination.zig's existing public
surface (parsePageParams sits alongside it, not replaced).
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Orphan seed/fixture data pre-dates the FK | a test fixture inserts a `fleet_id` with no matching `core.fleets` row | caught at Dimension 1.2's test / migration apply time, not silently accepted; fixture must be corrected before this lands. |
| Doc/registry re-drift after this fix | a future error-code deletion doesn't update the external docs repo again | Dimension 2.1's diff check is documented as a manual-run verification here; a permanent CI gate for this is Out of Scope (follow-up). |
| §4 convergence breaks fleets' cursor pagination | if Discovery resolves toward convergence and the new helper doesn't preserve cursor+limit semantics | Dimension 4.1's test (convergence variant) asserts fail-closed behavior without asserting away the cursor-based scheme; any semantic change beyond fail-closed parsing is out of scope for that variant. |

---

## Invariants

1. Every `fleet_id`-bearing row in `fleet.runner_leases`/`fleet.runner_affinity` references an existing `core.fleets` row — enforced by the FK constraint (DB-level, not review discipline).
2. `docs/api-reference/error-codes.mdx` lists no code absent from `error_entries.zig`'s registry — enforced by Dimension 2.1's diff check (manual verification step, not yet a standing gate — see Out of Scope).
3. `UZ-PROVIDER-003`'s hint text is a byte-accurate description of `credential_probe.zig`'s actual field requirements — enforced by Dimension 3.1's string-content test.

No code-enforceable invariant for §4 beyond Dimension 4.1's test — the comment/convergence question is a documentation-accuracy fix, not a runtime guarantee.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|-----------------------------------------------|
| 1.1 | integration | `test_fleet_delete_cascades_to_runner_lease_and_affinity_rows` | delete a `core.fleets` row with existing lease/affinity children → both child rows are gone. |
| 1.2 | integration | `test_runner_lease_affinity_reject_orphan_fleet_id` | insert a lease/affinity row with a non-existent `fleet_id` → DB rejects with a foreign-key-violation error. |
| 2.1 | integration (docs-repo script) | `test_error_codes_doc_matches_registry` | diff the doc's code column against `error_entries.zig`'s producer list → zero codes present in the doc but absent from the registry. |
| 3.1 | unit | `test_uz_provider_003_hint_matches_validation_rule` | `error_entries.zig`'s `UZ-PROVIDER-003` hint string contains the conditional `api_key` phrasing, not the unconditional "all three required" phrasing. |
| 4.1 | unit or integration (per Discovery's resolved direction) | `test_pagination_header_comment_matches_actual_consumers` or `test_fleets_list_uses_shared_limit_parser` | comment variant: static string-content check against actual `@import` consumers of `pagination.zig`. Convergence variant: malformed `limit` on `fleets/list.zig`'s endpoint → 400, mirroring `api_keys/list.zig`'s existing fail-closed test. |

Regression: Dimension 1.1/1.2 include a valid (non-orphan) insert case proving existing legitimate rows are unaffected. Idempotency/replay: N/A — no retry semantics in this workstream.

---

## Acceptance Criteria

- [ ] Fleet delete cascades to runner-lease/affinity rows — verify: `zig build test --summary all` (Dimension 1.1)
- [ ] Orphan `fleet_id` insert rejected — verify: `zig build test --summary all` (Dimension 1.2)
- [ ] Docs repo lists no dead error codes — verify: manual diff script (Dimension 2.1), run in `~/Projects/docs`
- [ ] `UZ-PROVIDER-003` hint matches actual validation — verify: `zig build test --summary all` (Dimension 3.1)
- [ ] `pagination.zig` comment (or convergence) resolved per Discovery — verify: `zig build test --summary all` (Dimension 4.1)
- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (§1 touches schema)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: full test suite
zig build test --summary all && echo "PASS" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -5
# E3: schema apply against a fresh DB (teardown-rebuild model)
make test-integration 2>&1 | tail -10
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: docs-repo dead-code diff (run inside ~/Projects/docs on its own branch)
# CORRECTED at PLAN: producers come from BOTH entry files (runtime file was missing),
# and named-constant entries (e(S_UZ_…)) are captured by grepping every UZ- token in the
# entry files, not just the e("UZ-…") literal form. Deliberately-documented historical /
# skipped codes are then subtracted (they are documentation, not dead rows).
grep -oE '`UZ-[A-Z]+-[0-9]+`' api-reference/error-codes.mdx | tr -d '`' | sort -u > /tmp/doc-codes.txt
grep -hoE 'UZ-[A-Z]+-[0-9]+' \
  ~/Projects/agentsfleet/src/agentsfleetd/errors/error_entries.zig \
  ~/Projects/agentsfleet/src/agentsfleetd/errors/error_entries_runtime.zig | sort -u > /tmp/registry-codes.txt
# Deliberately-documented (kept): UZ-AUTH-021 struck-through row, UZ-SLK-021 gap-note prose.
printf 'UZ-AUTH-021\nUZ-SLK-021\n' | sort -u > /tmp/documented-historical.txt
comm -23 /tmp/doc-codes.txt /tmp/registry-codes.txt | comm -23 - /tmp/documented-historical.txt
# expect: empty (no live-looking doc row lacks a producer)
```

---

## Dead Code Sweep

**1. Orphaned references — zero remaining rows/mentions.**

| Deleted symbol/row | Grep | Expected |
|---------------------|------|----------|
| `UZ-AUTH-009` / `UZ-AUTH-010` in `~/Projects/docs/api-reference/error-codes.mdx` (AMENDED: 021 kept per `f64e20c`) | `grep -n "UZ-AUTH-009\|UZ-AUTH-010" ~/Projects/docs/api-reference/error-codes.mdx` | 0 matches |

No files deleted from this repo; §1/§3/§4 edit existing files in place.

---

## Discovery (consult log)

- **§4 consult (RESOLVED at PLAN, Jul 03):** correct the comment; no convergence. Rationale unchanged from the spec default — `fleets/list.zig` uses a keyset (`cursor`+`limit`) scheme that is structurally different from `parsePageParams`' offset (`page`+`page_size`); forcing convergence would change fleets' pagination semantics. Refinement: the comment overclaimed in the *opposite* direction too — `parsePageParams` actually has **three** consumers (`api_keys/list.zig`, `fleet/runners_list.zig`, `fleet/runner_events.zig`), not just api-keys. Corrected comment names all three and records the `fleets/list.zig` keyset divergence.
- **§1 scope note:** the original audit named only `023_fleet_runner_affinity.sql` and `021_fleet_runners.sql`; re-verification found `021` already has a correct FK (to `core.tenants`, not the FK gap) and `022_fleet_runner_leases.sql` has the *same* gap as `023` — this spec's §1 fixes `022`+`023`, not `021` (already correct, not touched).
- **§1 consult — cross-schema FK boundary (RESOLVED at PLAN, Jul 03):** adding `fleet.runner_leases.fleet_id`/`fleet.runner_affinity.fleet_id → core.fleets(id)` crosses the `fleet`↔`core` schema boundary that `021_fleet_runners.sql`'s header calls a deliberate trust separation. Resolved **in favour of the FK**: `021` itself already carries `tenant_id UUID … REFERENCES core.tenants(id) ON DELETE CASCADE` (line 39), so a `fleet.* → core.*` FK-with-cascade is an established pattern. The boundary is about untrusted runner *identity* not sitting in the tenant-data schema — not about forbidding referential integrity on `fleet_id`. (`docs/SCHEMA_CONVENTIONS.md` has no dedicated FK section — a formal one is flagged Out of Scope.)
- **§1 blast-radius discovery (Jul 03) — the spec under-counted the fixture work.** The spec's Files-Changed listed only `022`/`023`. A fixture audit found **13 integration-test files** that INSERT a lease/affinity row against a synthetic `fleet_id` with no `core.fleets` row; each fails the new FK and needs `base.seedFleet(...)` (shared helper at `src/agentsfleetd/db/test_fixtures.zig:141`, INSERT at :150) + matching teardown before the insert. The 13:
  - `fleet/`: `control_plane_integration_test.zig` (2 tests — "stale fencing token…" @319, "release is token-guarded…" @523), `service_renew_integration_test.zig`, `renewal_integration_test.zig`, `renewal_metering_test.zig`, `renewal_edge_test.zig`, `renewal_malformed_test.zig`, `concurrency_lease_test.zig` (via real `affinity.claim`), `concurrency_renew_test.zig` (two fleet ids), `liveness_sweeper_integration_test.zig`, `service_token_splits_wire_test.zig`.
  - `http/handlers/runner/`: `memory_fencing_test.zig`, `memory_loop_integration_test.zig`, `credentials_mint_integration_test.zig`.
  - Safe (already seed a fleet, no change needed): `integration_roundtrip_test.zig`, `integration_session_continuation_test.zig`, `event_lifecycle_integration_test.zig`, `placement_eligibility_test.zig`, `account_teardown_test.zig`, `connectors/slack/channel_memory_integration_test.zig`.
  - Cascade direction confirmed safe: no test expects lease/affinity rows to outlive their fleet; `account_teardown_test.zig` (@230-231) already asserts they're gone after purge, and production `account_teardown.zig` `PURGE_STATEMENTS` deletes them explicitly before `core.fleets` (cascade is redundant-but-compatible there).
- **§2 reconciliation (Jul 03) — spec drafted before a merged sibling decision.** Spec draft (Jul 02) said "delete 3 rows: `UZ-AUTH-009`/`010`/`021`." Verification of current `~/Projects/docs` (HEAD `main`): (a) `UZ-AUTH-009`/`010` are producerless live-looking rows → delete; (b) `UZ-AUTH-021` was deliberately made a struck-through historical row by commit `f64e20c` ("keep retired UZ-AUTH-021 visible", Kishore, Jul 03 12:46 — *after* this spec was drafted) → **kept, not deleted** (deleting reverts a merged decision, forbidden); (c) `UZ-SLK-021` is intentional gap-note prose (line 182), not a row → kept; (d) `UZ-INTERNAL-003` is LIVE — the spec's grep missed its `e(S_UZ_INTERNAL_003, …)` named-constant entry (`error_entries.zig:54`) → not touched. The spec's E8/Dimension-2.1 method was corrected (both entry files + named-constant tokens + subtract documented-historical); see amended §2 and E8.
- **Metrics review:** no product/operator signal changes in this workstream.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/SCHEMA_CONVENTIONS.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test --summary all` | {paste snippet} | |
| Integration tests | `make test-integration` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Docs-repo dead-code diff | E8 above | {paste snippet} | |

---

## Out of Scope

- A standing CI gate that diffs `error-codes.mdx` against `error_entries.zig` on every commit (Dimension 2.1 is a one-time manual verification here) — flagged as a follow-up spec, likely a `write_spec`-style doc-freshness gate mirroring M107's route-registration freshness gate.
- Adding a formal FK-convention section to `docs/SCHEMA_CONVENTIONS.md` — the convention is inferred from sibling tables in this spec; documenting it formally is a separate, smaller follow-up.
- Any schema table beyond `022`/`023` — `021_fleet_runners.sql` is already correct and not touched.
