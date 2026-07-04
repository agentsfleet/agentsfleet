<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M112_001: "Templates" reads as "Fleet library" across UI, CLI, docs, and the full stack

**Prototype:** v2.0.0
**Milestone:** M112
**Workstream:** 001
**Date:** Jul 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — user-facing naming clarity ahead of the v2.0.0 cut; flagged as a drab section name during a live design pass.
**Categories:** CLI, DOCS, UI
**Batch:** B1 — independent of M108's connector work; shares the branch/PR by explicit instruction (see Branch), not a sequencing dependency.
**Branch:** feat/m108-connector-platform — continues on the SAME branch/PR (#477) as M108_001/M108_002, by Indy's explicit instruction this session ("I want all the M112, M113 folded in this PR"), not the default one-worktree-per-milestone convention.
**Test Baseline:** unit=2309 integration=249
**Depends on:** none
**Provenance:** LLM-drafted (Claude Sonnet 5, Jul 04, 2026) from a blast-radius audit run earlier this session — treat the audit's file list as a starting point, not a final inventory; re-grep at EXECUTE since the audit predates several intervening commits.

**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Two-tier template catalog (M103) — the feature being renamed; this spec changes none of its shape, only the copy layer above it.

---

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/fleets/new/InstallSourceSelector.tsx` — the `<SectionLabel>Templates</SectionLabel>` this rename directly targets; read the whole install flow it anchors before touching any string in it.
2. `ui/packages/app/app/(dashboard)/fleets/new/AddTemplateDialog.tsx`, `template-docs.tsx`, `InstallEntry.tsx`, `InstallConfirm.tsx` — sibling copy sites in the same flow; read for tone/register so replacements read as one voice, not a mechanical find-replace.
3. `cli/src/commands/fleet_templates.ts` — CLI help/output copy for `agentsfleet templates`; the command name and `--template` flag are OUT of scope (Product Clarity #6) — only the strings it prints change.
4. `/Users/kishore/Projects/docs/fleets/templates.mdx` — the docs page; read alongside `quickstart.mdx` and `cli/agentsfleet.mdx` for cross-link consistency before editing prose.
5. This session's audit transcript is not persisted anywhere the agent can re-read — re-run the greps in Files Changed below rather than trusting a stale list.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** rename: fleet templates read as "Fleet library" across UI/CLI/docs/full stack
- **Intent (one sentence):** "Templates" reads as boilerplate-to-fill-in; "Fleet library" reads as browse-and-pick — originally scoped as copy-only, expanded mid-session to a full-stack rename by Indy's explicit direction (see Discovery), pre-2.0.0 so no deprecation window is owed.
- **Handshake:** implementing agent restates intent + assumptions at PLAN, before EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a user lands on `/fleets/new`, sees the section heading read "Fleet library" instead of "Templates," and every sentence around it (CLI help text, docs prose) uses the same name — nothing reads as a half-renamed feature.
2. **Preserved user behaviour** — *(superseded mid-session — see Discovery)* originally scoped so every command/route/scope/table/wire-field stayed byte-identical. What's actually byte-identical post-rename: the `?template=<id>` UI deep-link query param (a common noun naming the individual catalog entry, not the feature) and the `UZ-BUNDLE-*` error codes. Everything else changed in the follow-up full-stack pass (`9fecf7d0`): `agentsfleet templates` → `agentsfleet library`, `--template` → `--library`, `/v1/admin/fleet-templates` → `/v1/admin/fleet-libraries`, `template:write` → `library:write`, `core.fleet_bundle_templates` → `core.fleet_library`, `platform_template_id`/`tenant_template_id` → `platform_library_id`/`tenant_library_id`. Pre-2.0.0, no-compat-shim convention — no deprecation window owed.
3. **Optimal-way check** — *(superseded — see Discovery)* originally judged the full top-to-bottom rename not worth the risk for a copy complaint alone. Indy overrode this mid-session: this is the third time this exact surface has been partially reshuffled (M87, M110, M112) without the underlying rename ever landing, and he chose to pay the one-time full-stack cost now instead of layering another half-measure. Shipped in `9fecf7d0`.
4. **Rebuild-vs-iterate** — iterate on the copy (§1/§2); the full-stack rename (schema/scopes/routes/CLI/wire-fields) was pulled forward into this same PR by Indy's explicit direction rather than deferred to a follow-up spec — not scope creep, a direct instruction reversing the original call (see Discovery).
5. **What we build** — updated copy in the UI install flow, CLI help/output text, and docs prose named in Files Changed; "Fleet library" as the section/page-title proper noun (lowercase `l` — Indy's call this session, matching the CLI's existing lowercase usage over the initially-shipped capital-`L` `SectionLabel`/OpenAPI-tag spelling), with "template"/"library" kept as the natural common noun for an individual catalog entry inside prose — not a mechanical string replace.
6. **What we do NOT build** — *(originally scoped; the route/scope/schema/wire-field/CLI-command/flag/analytics-event items below shipped anyway in `9fecf7d0` per Indy's mid-session reversal — see Discovery)*. Still true post-amendment: no `?template=` query-param rename (kept, common noun); no `changelog.mdx` history rewrite (falsifies the record); no docs URL move (the URL doesn't change, only the words and casing on the page).
7. **Fit with existing features** — compounds with the M110 template-onboarding dashboard flow and the M103 two-tier catalog; must not destabilize `AddTemplateDialog`'s deep-link test or the `?template=` query contract.
8. **Surface order** — UI first (the section the complaint was raised about), CLI help text second, docs third (gated behind Indy's explicit go-ahead before that repo's commit — §3).
9. **Dashboard restraint** — N/A, no new controls; this is a copy-only pass.
10. **Confused-user next step** — N/A, no new failure surface; existing empty-state/error copy for this flow is unchanged by this spec (tracked separately under M113's error-message audit).

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline.
- `dispatch/write_ts_adhere_bun.md` — every touched file is `.ts`/`.tsx`.
- `dispatch/write_any.md` — UFS (no repeated/semantic literal strings introduced — reuse the existing per-file consts being renamed, don't hand-duplicate the new string across files).

No Zig, no schema, no HTTP handler touched — other rule files don't apply.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `.zig` touched |
| PUB / Struct-Shape | no | no new public surface |
| File & Function Length | no | copy edits only, no file grows structurally |
| UFS | yes — const renames | keep the existing named-constant pattern (e.g. `TEMPLATES_EMPTY_TITLE`) — rename the constant's value and, where its identifier now reads stale, the identifier too, in the same commit as the string |
| UI Substitution / DESIGN TOKEN | no | no new markup/classes, text-only edits |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | not touched |

---

## Overview

**Goal (testable):** every user-facing string naming this feature as a proper noun (section headings, page titles, empty-state titles, CLI help/output copy, docs page title + prose) reads "Fleet library" instead of "Templates," while every route, scope, DB table, wire field, CLI command/flag name, query param, and analytics event name asserted by existing tests is unchanged.

**Problem:** "Templates" as a section name reads flat/utilitarian next to the rest of the product's voice; it undersells a curated, browsable starting-point gallery.

**Solution summary:** rewrite the copy layer only — UI section/page copy in the fleet-install flow, CLI help/output strings, and docs prose — leaving every underlying identifier, route, and contract exactly as it is today.

---

## Prior-Art / Reference Implementations

- **UI** → design-system primitives already in use (`SectionLabel`, `EmptyState`) — no new component, just new text passed to existing ones.
- **CLI** → mirror the existing `agentsfleet templates` output structure in `cli/src/commands/fleet_templates.ts`; only the strings change, not the "7 Pillars" structure (handler purity, output-as-a-service) already in place.
- **Docs** → `docs/fleets/templates.mdx` stays at its current URL and Mintlify nav position; only prose changes.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/fleets/new/InstallSourceSelector.tsx` | EDIT | section label "Templates" → "Fleet library" |
| `ui/packages/app/app/(dashboard)/fleets/new/AddTemplateDialog.tsx` | EDIT | dialog/button copy pass for voice consistency |
| `ui/packages/app/app/(dashboard)/fleets/new/template-docs.tsx` | EDIT | empty-state copy + constant naming pass |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallEntry.tsx` | EDIT | copy pass |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallConfirm.tsx` | EDIT | copy pass |
| `ui/packages/app/app/(dashboard)/fleets/new/page.tsx` | EDIT | page description copy |
| `ui/packages/app/app/(dashboard)/fleets/page.tsx` | EDIT | page description copy |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | dashboard description copy |
| `ui/packages/app/tests/add-template-dialog.test.tsx` | EDIT | assertions on renamed copy |
| `ui/packages/app/tests/add-template-dialog-deep-link.test.tsx` | EDIT | assertions on renamed copy (deep-link mechanics unchanged) |
| `ui/packages/app/tests/fleets-install-entry-gate.test.ts` | EDIT | assertions on renamed copy |
| `ui/packages/app/tests/fleets-routes.test.ts` | EDIT | assertions on renamed copy |
| `ui/packages/app/tests/e2e/acceptance/template-onboarding.spec.ts` | EDIT | assertions on renamed copy |
| `cli/src/commands/fleet_templates.ts` | EDIT | help/output copy pass at §2 time — command name and `--template` flag untouched *then*; both later renamed anyway in the full-stack pass (`9fecf7d0`, see Discovery), file moved to `fleet_library.ts` |
| `cli/test/fleet-templates.unit.test.ts` | EDIT | assertions on renamed copy |
| `/Users/kishore/Projects/docs/fleets/templates.mdx` | EDIT | full prose pass — gated behind Indy's go-ahead before commit (separate repo) |
| `/Users/kishore/Projects/docs/{quickstart,fleets/overview,fleets/install,cli/agentsfleet,fleets/authoring,workspaces/managing,fleets/credentials,fleets/tools,fleets/troubleshooting,fleets/webhooks}.mdx` | EDIT | prose mentions — gated behind Indy's go-ahead |

Not touched by §1-§3 above, but all renamed anyway in the follow-up full-stack commit `9fecf7d0` per Indy's mid-session reversal (see Discovery — this spec's own Files Changed table isn't amended to list `9fecf7d0`'s ~113 files, that commit is its own unit of work; tracked here for the record): route paths, authz scope strings, Postgres tables, JSON wire fields, CLI command/flag names, analytics event name. Still genuinely not touched: `changelog.mdx` history, `docs/v2/done/` specs.

**This resume session's own follow-up pass** (casing flip, docs URL move, `?template=`→`?library=`, dashboard/CLI/backend copy rename, the `platform_template_id`/`tenant_template_id` bug fix — see Discovery for the full account) touched ~50 more files across the docs repo, `ui/packages/app/`, `cli/`, and `src/agentsfleetd/errors/`. Not re-enumerated row-by-row here; `git log`/`git diff` on this spec's commits is the authoritative list.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** copy-only rename, three Sections (UI / CLI / Docs), each independently shippable.
- **Alternatives considered:** full-stack rename (routes/scopes/schema/wire fields) — originally rejected as a breaking change with no user-visible benefit beyond internal tidiness; **reversed mid-session** (see Discovery) once Indy weighed it against the recurring cost of *not* doing it (M87/M110/M112's repeated partial reshuffles of this surface) — landed anyway in `9fecf7d0`, pre-2.0.0 so no deprecation window owed.
- **Patch-vs-refactor verdict:** §1-§3 of this spec are a **patch** — copy edits at the presentation layer only. The full-stack rename that followed (`9fecf7d0`) is a **refactor** at the schema/route/scope layer, executed as its own commit outside this spec's own Sections.

---

## Sections (implementation slices)

### §1 — UI copy: fleet-install flow reads "Fleet library" — DONE

The section/page-level naming the complaint targeted. **Implementation default:** "Fleet library" is the section/nav/page-title proper noun; "template" remains the natural common noun for an individual catalog entry in body copy (e.g. "Browse the Fleet library" as a heading, "pick a template to start" mid-sentence) — the agent uses judgment per string for natural phrasing, not mechanical find-replace, re-reading each file's surrounding sentence before changing it.

- **Dimension 1.1** — DONE — `InstallSourceSelector`'s section label reads "Fleet library" → Test `test_install_source_selector_section_label_reads_fleet_library`
- **Dimension 1.2** — DONE — `template-docs.tsx`'s empty-state and doc-link copy is voice-consistent with the renamed section ("No fleet library yet" / "Write your own template to start your Fleet library." / readonly variant "Ask a workspace admin to add one.", constants renamed `FLEET_LIBRARY_EMPTY_*`) → Test `test_template_docs_empty_state_copy_updated`
- **Dimension 1.3** — DONE — `AddTemplateDialog`, `InstallEntry`, `InstallConfirm`, and the two page-description strings (`fleets/new/page.tsx`, `fleets/page.tsx`) read consistently with the new name; `app/(dashboard)/page.tsx`'s terse tagline ("Start a fleet from a template.") deliberately left as natural common-noun usage → Test `test_install_flow_copy_consistent_with_fleet_library`
- **Dimension 1.4** — DONE — every existing test asserting the old "Templates" copy is updated to assert the new copy; the `?template=` query param, route, and deep-link mechanics assert unchanged → Test `test_template_deep_link_query_param_unchanged` (regression)
- **Dimension 1.5 (added mid-EXECUTE)** — DONE — `InstallEntry`'s Create-a-template CTA now gates on `canAddTemplate` (threaded from `hasTemplateWriteScope(sessionClaims)` in the dashboard `page.tsx`), matching `InstallSourceSelector`'s pre-existing gate; both components' empty-state description is now conditional on the same flag so a viewer without `template:write` is never invited to do something the backend would reject → Test `test_install_entry_gates_create_template_on_scope` (see Discovery)

### §2 — CLI help/output copy reads "Fleet library" — DONE

The `agentsfleet templates` and `agentsfleet install --template` commands print updated copy; the command name and flag name themselves are unchanged.

- **Dimension 2.1** — DONE — `fleet_templates.ts` empty-state output string ("No fleet library yet.") reads consistently with "Fleet library"; table header `TEMPLATE` and the `--template` suggestion text deliberately kept (individual catalog entries, common-noun usage) → Test `test_cli_templates_output_copy_updated`
- **Dimension 2.2** — DONE — the command name `templates` and the `--template` flag are byte-identical to before (regression) → Test `test_cli_template_command_and_flag_unchanged`

### §3 — Docs prose reads "Fleet library" (gated on Indy's go-ahead)

Docs-repo changes need Indy's explicit per-session go-ahead before committing there (separate, shared repo). The agent prepares the diff and surfaces it for approval before running `git commit` in `~/Projects/docs`.

- **Dimension 3.1** — `fleets/templates.mdx` prose reads "Fleet library"; the page's URL and Mintlify nav position are unchanged → Test: manual diff review, Indy-acked before commit (separate repo, own review path — no automated test)
- **Dimension 3.2** — cross-link prose in the remaining docs files reads consistently → same manual-review gate

---

## Metrics & Observability

Not applicable — no product/operator signal changes. The `fleet_template_onboarded` analytics event name is explicitly unchanged (Product Clarity #6); no new event, no renamed event.

---

## Interfaces

Not applicable — no interface changes. This spec touches presentation-layer strings only; every function signature, route, and wire shape named in Product Clarity #2 is unchanged.

---

## Failure Modes

N/A — no new failure path. Copy-only change: no new branching logic, no new network call, no new error condition.

---

## Invariants

1. *(superseded — see Discovery)* This spec's own §1-§3 changed no route/scope/table/wire-field/CLI-command/flag/analytics-event-name — only the `?template=` query param invariant still holds today. The full-stack pass that followed (`9fecf7d0`) intentionally renamed the rest (`fleet_template_onboarded` → `fleet_library_onboarded` included); that commit carries its own test coverage (`cli-tree.fleet.unit.test.ts`'s command-tree assertions now assert `library`/`--library`, `no-api-template-mint.test.ts` updated to the new table/route names) — not re-asserted here.
2. No test asserts the literal string "Templates" as a section/page heading after this spec lands — enforced by grep in Eval Commands.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_install_source_selector_section_label_reads_fleet_library` | render `InstallSourceSelector` → section label text is "Fleet library", not "Templates" |
| 1.2 | unit | `test_template_docs_empty_state_copy_updated` | render empty catalog state → title/description use the new voice |
| 1.3 | unit | `test_install_flow_copy_consistent_with_fleet_library` | render `AddTemplateDialog`/`InstallEntry`/`InstallConfirm` and the three page descriptions → no leftover "Templates" as a proper noun |
| 1.4 | unit + e2e | `test_template_deep_link_query_param_unchanged` | `/fleets/new?template=<id>` still resolves the same install flow; the deep-link test and e2e onboarding spec still pass against the new copy |
| 2.1 | unit | `test_cli_templates_output_copy_updated` | `agentsfleet templates` stdout uses the new copy |
| 2.2 | unit (regression) | `test_cli_template_command_and_flag_unchanged` | `cli-tree.fleet.unit.test.ts` still asserts command name `templates` and flag `--template` verbatim |

Regression: Invariant 1's grep-based check (Eval Command E8) covers routes/scopes/schema/wire-fields/CLI-contract/analytics-event in one pass — no per-dimension regression test needed beyond what's listed.

Idempotency/replay: N/A — no retry semantics in a copy change.

---

## Acceptance Criteria

- [ ] Section label in the install flow reads "Fleet library" — verify: `grep -rn "Fleet library" "ui/packages/app/app/(dashboard)/fleets/new/InstallSourceSelector.tsx"`
- [ ] No remaining "Templates" as a section/page proper noun in the touched files — verify: Eval Command E8
- [ ] CLI command/flag names unchanged — verify: `agentsfleet templates --help` and `agentsfleet install --help` still show `templates`/`--template`
- [ ] `make lint` clean · `make test-unit-app` passes · `make test-unit-cli` passes
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] Docs-repo diff prepared and shown to Indy before commit; commit only after explicit go-ahead

---

## Eval Commands (post-implementation)

```bash
# E1: UI section label renamed
grep -rn "Fleet library" "ui/packages/app/app/(dashboard)/fleets/new/InstallSourceSelector.tsx" && echo "PASS" || echo "FAIL"
# E2: Build — cd ui/packages/app && bun run build
# E3: Tests — make test-unit-app && make test-unit-cli
# E4: Lint — make lint-app 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — N/A, no Zig touched
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: no orphaned "Templates"-as-heading string in the touched UI/CLI files —
grep -rn '"Templates"' "ui/packages/app/app/(dashboard)/fleets/new/" cli/src/commands/fleet_templates.ts
```

---

## Dead Code Sweep

N/A — no files deleted, no symbols removed. Const identifiers renamed alongside their string value (e.g. `TEMPLATES_EMPTY_TITLE`) are handled as edits within Dimension 1.2, not deletions.

---

## Discovery (consult log)

- **Consults:** Indy caught, mid-EXECUTE, that `TEMPLATES_EMPTY_DESCRIPTION` ("Write your own template...") rendered unconditionally in both `InstallSourceSelector.tsx` and `InstallEntry.tsx`, even when their paired "Create a template" button was hidden for lack of `template:write` scope — inviting an action the backend would reject. `InstallSourceSelector` already gated the button correctly (pre-existing); `InstallEntry` (the dashboard first-run embed) had no gating at all. Fixed in the same commit as the rename since it's the identical surface already in scope: added `TEMPLATES_EMPTY_DESCRIPTION_READONLY`, gated both components' description on their existing/new `canAddTemplate` flag, threaded `hasTemplateWriteScope(sessionClaims)` from the dashboard page (`page.tsx`) through `FirstInstall` into `InstallEntry` (previously never computed there). Backend (`route_scopes.zig`'s `TEMPLATE_WRITE` gate) was already the real security boundary — this is a discoverability fix, not a security fix.
- **Scope amendment (post `393263e7`, before this spec's own DONE):** after §1/§2 shipped as copy-only, Indy walked through the M87/M110/M112 history of this same surface (Models/Credentials/Integrations/Templates) getting reshuffled repeatedly without the underlying identifiers ever actually being renamed, and directed a full-stack rename instead of leaving another half-measure in place. That work landed separately as `9fecf7d0` ("rename: fleet templates → fleet library (full-stack) + 5 refactors", 113 files: schema, Zig backend, CLI, UI, OpenAPI, scopes, analytics), executed by `opencode` in this same shared worktree, not as a Section of this spec. This reverses the Product Clarity #2-#4/#6, Files Changed "Not touched" line, and Invariant #1 as originally written — those sections above are annotated in place rather than silently rewritten, per the operating model's "amend the spec to match what shipped, don't revert the code" rule. The `?template=` UI query param was deliberately kept (common noun, not the feature name).
- **Casing amendment (this session, on resume):** Indy directed the feature's proper-noun spelling to be lowercase `l` — "Fleet library" — rather than the capital-`L` "Fleet library" the UI `SectionLabel`, OpenAPI tag, and most of `9fecf7d0`'s comments had shipped with, after being shown that the codebase was already split 85/5 in favor of capital-`L` outside a couple of CLI comments. Applied as its own mechanical pass across all ~90 occurrences (UI heading, OpenAPI tag/schemas, Zig comments and one user-facing error hint string, CLI help text, this spec) — `public/openapi.json` regenerated via `make check-openapi` from the edited source YAML rather than hand-patched.
- **Docs URL move + full "credential"/"template" word purge (this session):** after §3's prose landed, Indy directed the docs pages themselves to move — `docs/fleets/templates.mdx` → `docs/fleets/library.mdx`, and (folding in M113_003's own docs debt, never done in that milestone) `docs/fleets/credentials.mdx` → `docs/fleets/secrets.mdx`. No redirects were kept for either move (pre-2.0.0, no-compat-shim convention extended to docs URLs) — the changelog's historical `/agents/templates`/`/agents/credentials` links (predating this session) now 404; changelog prose is append-only so it is not patched. A follow-up global-search audit (Indy: "search the docs repo for templates/credential(s), should only be in changelog.mdx") found the remaining hits all traced to real shipped code still using those words, not stale docs — surfaced as a scope decision (see next entry) rather than silently left.
- **Code rename to match the docs purge (this session, Indy-directed: "rename the code too"):** the dashboard's "Create a template"/"Add template"/"Use template" copy (`AddTemplateDialog.tsx`, `InstallEntry.tsx`, `InstallSourceSelector.tsx`, `install-state-list.tsx`) renamed to "Add library entry"/"Use entry"/"Back to library"; the `?template=` UI deep-link query param (previously the one deliberately-kept exception, see two entries up) renamed to `?library=` across `page.tsx`'s `SearchParams` type, the generated hrefs, and every deep-link test; the CLI's `agentsfleet library` table header and `printRequirements` label (`CREDENTIALS`/`Credentials` → `SECRETS`/`Secrets`); and the backend error registry titles for `UZ-VAULT-001/002/003`, `UZ-PROVIDER-002/003`, and `UZ-BUNDLE-002`'s detail text (`Credential ...` → `Secret ...`, `template` → `library entry`) — the docs `error-codes.mdx` Title column updated to match. `UZ-AGT-003`/`UZ-WH-020`'s titles were left as `Credential ...` (not named in Indy's approved scope; a further migration if he wants full consistency there too).
- **Real bug found and fixed while renaming the dashboard's UI copy:** `lib/types.ts`'s `InstallFleetRequest` and `InstallStates.tsx`'s create-body construction were still sending `platform_template_id`/`tenant_template_id` — fields the backend's `create.zig` (post `9fecf7d0`) no longer recognizes at all (it only parses `platform_library_id`/`tenant_library_id`). Every dashboard "Install fleet" click on this branch would have failed against the real backend with "install requires platform_library_id or tenant_library_id" — unit tests never caught it because they mock the server action and only assert self-consistency between the mock and the (wrong) field the source code sent. Fixed both files plus the 4 test files asserting the old field names (`fleets-install-flow.test.ts`, `fleets-install-states.test.ts`, `fleets-actions.test.ts`, `fleets-api-client.test.ts`). Also fixed two similarly-stale backend error hints pointing at a CLI command that no longer exists: `UZ-WH-020`/`UZ-AGT-003`'s `.detail` said "Run `agentsfleet credential add ...`" — the real command is `agentsfleet secret add`.
- **Metrics review:** not applicable — no product/operator signal changes (see Metrics & Observability).
- **Skill chain outcomes:** populated after `/write-unit-test` and `/review` run.
- **Deferrals:** none yet.

---

## Skill-Driven Review Chain (mandatory)

Standard chain — `/write-unit-test` → `/review` → `/review-pr`, per `AGENTS.md`.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-app && make test-unit-cli` | | |
| Lint | `make lint-app` | | |
| Gitleaks | `gitleaks detect` | | |
| Dead code sweep | N/A | | |

---

## Out of Scope

- ~~Full-stack rename (routes, authz scopes, DB tables, JSON wire fields, CLI command/flag names, analytics event name) — a distinct, higher-risk spec if ever pursued; not justified by a copy complaint alone.~~ **Superseded mid-session** — Indy directed this to ship in the same PR instead of deferring it; landed in `9fecf7d0` (see Discovery). Not a spec-scope violation: a direct instruction reversing the original call, not agent-unilateral scope creep.
- The `?template=` UI query param rename — genuinely still out of scope; kept as the common noun for an individual catalog entry, not the feature name.
- Rewriting `changelog.mdx` historical entries — falsifies the record; history stays as written.
- The Models/Secrets page rework and UI error-message audit — tracked separately as M113.
