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

# M130_001: The platform catalog row tells the truth, and the operator owns it

**Prototype:** v2.0.0
**Milestone:** M130
**Workstream:** 001
**Date:** Jul 13, 2026
**Status:** PENDING
**Priority:** P1 — the catalog badge currently asserts the opposite of what install does; the operator cannot correct a mistyped repository without deleting the row.
**Categories:** API, UI
**Batch:** B1 — standalone; no sibling workstream shares its files.
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M128_001 (built the catalog surface this corrects; already in `done/`)
**Provenance:** LLM-drafted (claude-opus-4-8, Jul 13, 2026) — authored from a live read of `catalog.zig`, `library_store.zig`, `sql.zig`, and the admin surface; Indy chose the invalidate-and-unpublish model in session.
**Canonical architecture:** `docs/architecture/fleet_bundles.md`

---

## Overview

**Goal (testable):** A platform catalog row whose bundle is absent can never render as `PUBLISHED`, and an operator can rewrite a row's name, repository, and ref in place — with a source change nulling the bundle and withdrawing the row, so the catalog never advertises a source it is not serving.

**Problem:** Every row on `/admin/fleet-libraries` currently shows a green **PUBLISHED** badge whose tooltip promises *"Live in every workspace gallery"* — while showing `—` for its bundle. Both the gallery query and the install query require `content_hash IS NOT NULL`, so those rows are in fact invisible to every workspace and uninstallable. The badge asserts the exact opposite of what the system does. Separately, an operator who adds a fleet from a mistyped repository has no way to correct it: `Edit` reaches only the description and the credential copy, so the only remedy is unpublish → delete → re-add, which discards the curated copy.

**Solution summary:** Close the hole in the derived status so a public row with no bundle gets its own honest state instead of borrowing `PUBLISHED`. Widen `PATCH /v1/admin/fleet-libraries/{id}` from three curatable fields to six, admitting `name`, `source_repo`, and `source_ref`; a change to either source field nulls `content_hash` and stages the row back to `draft` in the same statement, which is exactly what a refetch already does — so a row can never point at one repository while serving another's bytes. Because the operator now owns `name`, drop it from the refetch upsert so a rename survives `Fetch update`, mirroring how `description` is already preserved. Since the declared credential set moves when the repository moves, prune the reason map to the credentials the new bundle actually declares. Finally, make the repository cell a link and correct the admin button, which says "Add fleet" for a thing that is neither.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m130): the catalog row cannot lie, and the operator can rewrite it
- **Intent (one sentence):** A platform operator can correct a fleet library's repository in place and always sees a status badge that matches what a workspace will actually get.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/library/catalog.zig` — the PATCH handler being widened. Its header states the two guards that must survive this change verbatim (a published row always has a bundle; a published row is never deleted). `applyPatch` already wraps its statements in one BEGIN/COMMIT and grades every write on `RETURNING id` — mirror that shape, do not invent a second one.
2. `src/agentsfleetd/fleet_library/sql.zig` — `INSERT_PLATFORM`'s `ON CONFLICT … DO UPDATE SET` list is the refetch overwrite policy. `description` and `required_credentials_reasons` are *absent* from it on purpose; `name` joins them in §3. `UPDATE_CATALOG_VISIBILITY` carries the publish-needs-a-bundle guard in SQL — it stays.
3. `ui/packages/app/lib/types.ts` — `catalogStatus()` is the single derivation of a row's state; the status is deliberately never a wire field. The hole is that it consults `visibility` alone on the public branch.
4. `src/agentsfleetd/http/handlers/fleet_bundles/resolve.zig` (`parseOwnerRepo`) and `src/agentsfleetd/fleet_library/github_source.zig` (`validSegment`) — the existing source validation the widened PATCH must reuse rather than re-derive.
5. `docs/architecture/fleet_bundles.md` — the bundle lifecycle and the content-addressed store this spec must not contradict.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/library/catalog.zig` | EDIT | `PatchBody` gains three fields; `applyPatch` gains the source-invalidation write and reuses the existing source validators. |
| `src/agentsfleetd/fleet_library/sql.zig` | EDIT | New statement for the identity/source update (nulls `content_hash`, stages `draft`); `name` leaves `INSERT_PLATFORM`'s conflict SET; the refetch statement prunes the reason map. |
| `src/agentsfleetd/fleet_library/library_store.zig` | EDIT | Params struct for the new update; the refetch path passes the declared credential set through for pruning. |
| `ui/packages/app/lib/types.ts` | EDIT | `catalogStatus()` gains the fourth state; `PlatformCatalogPatch` gains the three fields. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/catalog-status.ts` | EDIT | The new state's label/help/tone and its `RowActions` verdict. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/library-copy.ts` | EDIT | Button verb corrected; new status copy; edit-dialog copy for the identity fields. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/EditFleetDialog.tsx` | EDIT | Name / repository / ref fields; the un-explained-credential surface. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx` | EDIT | Repository cell becomes a link when the value is `owner/repo` shaped. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx` | EDIT | Consumes the corrected button verb and title constant. |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | The source-changed operator event. |
| `src/agentsfleetd/http/handlers/library/catalog_patch_test.zig` | CREATE | Unit coverage for the widened patch: validation, invalidation, no-op equality. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/EditFleetDialog.test.tsx` | EDIT | The new fields and the un-explained-credential surface. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.test.tsx` | EDIT | The fourth status, its row actions, the repository link. |
| `ui/packages/app/tests/admin-fleet-libraries-page.test.ts` | EDIT | Page-level copy assertions follow the corrected verbs. |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | The operator walks the correct-a-mistyped-repository path end to end. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every new status label, copy string, and SQL identifier is a named constant — no inline literals), **NSQ** (named constants, schema-qualified SQL for the new statements), **NDC** (the old `FLEET_LIBRARIES_TITLE` spelling goes, it does not linger), **ORP** (renaming the copy constants is a cross-layer rename — sweep the tests and the e2e spec), **NLR** (touch-it-fix-it on the files opened), **FLS** (every `conn.query()` in the new path drains before `deinit`), **XCC** (cross-compile both linux targets), **ERR** (any new refusal cites a registered `UZ-` code — reuse `UZ-BUNDLE-001` for a malformed source; do not mint a code that duplicates it), **FLL** (`catalog.zig` and `EditFleetDialog.tsx` both grow — split before the cap, not after), **TSC**/**TSJ** (TypeScript conventions on every `.ts`/`.tsx` touched).
- **`dispatch/write_zig.md`** — the Zig surface: tagged-union results, `errdefer` placement, pg-drain, file ≤350 / fn ≤50.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the PATCH is a public API surface; §4 (every 409 carries `current_state`) already governs this handler's refusals and must keep governing them.
- **`dispatch/write_ts_adhere_bun.md`** — the admin surface: design-system primitive over raw HTML, token utility over arbitrary value.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `catalog.zig`, `sql.zig`, `library_store.zig` | Cross-compile `x86_64-linux` + `aarch64-linux`; `make check-pg-drain` clean; `errdefer` on every partial allocation in the new params path. |
| PUB / Struct-Shape | yes — new params struct in `library_store.zig` | Shape verdict per new pub surface; mirror the existing `PlatformInsertParams`. |
| File & Function Length (≤350/≤50/≤70) | yes — `catalog.zig` grows a write path; `EditFleetDialog.tsx` gains three fields | Split the identity/source write into its own function; if `catalog.zig` nears 350, lift the patch application into a sibling module rather than trimming comments. |
| UFS (repeated/semantic literals) | yes | Every status label, copy string, GitHub host prefix, and SQL identifier is a named constant. The `owner/repo` pattern is already single-sourced in `lib/fleet-library-source.ts` — reuse it, do not re-spell it. |
| UI Substitution / DESIGN TOKEN | yes — the repository link, the new form fields | Design-system primitives only (no raw `<a>`/`<input>`); token utilities only (no `text-[…]`). |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | ERROR REGISTRY yes; SCHEMA **no** | No DDL: `content_hash` is already nullable and every column exists — this spec changes no migration and does not touch `schema/embed.zig`. Refusals reuse the registered `UZ-CATALOG-001…004` / `UZ-BUNDLE-001` codes. |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/library/catalog.zig` (`applyPatch`) — the transactional, `RETURNING id`-graded partial update this widening extends. The race handling (zero rows ⇒ `UZ-CATALOG-001`) is the pattern; do not invent a second one.
- **Reference:** `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/AddLibraryDialog.tsx` — already spells the verb this spec adopts (`Create fleet library`) and already validates `owner/repo` against the shared `SOURCE_REF_PATTERN`. The admin surface is the one that diverged; it converges here.
- **Reference:** the `description` field's existing treatment — seeded from the bundle at first import, absent from the refetch conflict SET, therefore operator-owned. `name` acquires exactly this treatment in §3; no new mechanism is invented for it.

## Sections (implementation slices)

### §1 — The status cannot lie

A row that is `public` with a null `content_hash` currently renders as `PUBLISHED` because `catalogStatus()` consults `visibility` alone on that branch. The gallery and install queries both demand `content_hash IS NOT NULL`, so such a row is invisible and uninstallable — the badge promises the opposite of the behaviour. This slice gives that combination its own state, so the derivation is total over all four `(visibility × has-bundle)` combinations and the badge can no longer overclaim. The state is reachable only from data the API refuses to create (a hand-inserted row), but it exists in the wild and the surface must survive it honestly rather than assume it away.

**Implementation default:** the new state is presented as a fault, not a lifecycle step — its tone is the destructive/attention tone, not the `draft` amber, because it is a row that needs an operator, not a row awaiting one.

- **Dimension 1.1** — `catalogStatus()` returns the new state for `visibility=public` + `content_hash=null`, and is exhaustive over all four combinations → Test `test_catalog_status_public_without_bundle`
- **Dimension 1.2** — the badge for the new state carries its own label and help text and never renders the published "live in every workspace gallery" copy → Test `test_status_badge_never_claims_live_without_bundle`
- **Dimension 1.3** — `rowActions` for the new state offers `Unpublish` (to make it honest) and `Fetch bundle` (to make it true), and withholds `Publish` and `Delete` — the server refuses both for a public row → Test `test_row_actions_broken_state_recoverable`

### §2 — The operator owns the row's identity

`Edit` reaches only the description and the credential copy, so an operator who added a fleet from a mistyped repository must delete the row and re-add it, discarding the curated copy. `PATCH` widens to admit `name`, `source_repo`, and `source_ref`. The bundle in the object store was built from the *old* repository, so a change to either source field must invalidate it: `content_hash` goes null and `visibility` returns to `draft`, in one statement, so no interleaving can leave a public row pointing at a source it is not serving. This is not a new behaviour — every refetch already stages the row back to `draft` for exactly this reason.

`id` is not patchable and never becomes patchable: it is the primary key and workspace installs reference it as `platform_library_id`. Renaming it would orphan every install.

**Implementation default:** invalidation triggers on a *changed* value, not a *present* one — re-sending the identical repository must not withdraw a live fleet, so the comparison is against the stored row inside the transaction, not against the request body's presence.

- **Dimension 2.1** — `PATCH` accepts `name` and persists it; empty or over-cap names are refused → Test `test_patch_accepts_name`
- **Dimension 2.2** — `PATCH` accepts `source_repo` / `source_ref`, validated by the *existing* `parseOwnerRepo` + `validSegment` validators; a malformed source is refused with the registered bundle-invalid code → Test `test_patch_rejects_malformed_source`
- **Dimension 2.3** — changing either source field nulls `content_hash` and stages `visibility` to `draft` atomically → Test `test_patch_source_change_invalidates_bundle`
- **Dimension 2.4** — re-sending the *unchanged* source leaves `content_hash` and `visibility` untouched; a live fleet is not withdrawn by an idempotent PATCH → Test `test_patch_unchanged_source_is_noop`
- **Dimension 2.5** — `id` is absent from the patch body and unmovable; a body carrying one is ignored and the row keeps its slug → Test `test_patch_cannot_move_slug`

### §3 — A rename survives the next fetch

Once the operator owns `name`, the refetch upsert must stop overwriting it — otherwise the next `Fetch update` silently reverts the rename, which is worse than not offering the field. `name` leaves `INSERT_PLATFORM`'s `ON CONFLICT … DO UPDATE SET` list, joining `description` and `required_credentials_reasons`, which are already excluded for precisely this reason. A first import still takes the name from the bundle's frontmatter; only the *conflict* path preserves the operator's.

- **Dimension 3.1** — a refetch preserves an operator-renamed `name`; a first import still seeds it from the bundle → Test `test_refetch_preserves_operator_name`

### §4 — The credential copy tracks the declared set

The reason map is keyed by credential name and the declared credential set moves when the repository moves. Two leaks follow, and both only exist because §2 makes the source editable: a credential the new bundle no longer declares leaves a **stale reason key** that the dialog never renders but faithfully round-trips on every save, and a newly declared credential arrives with **no reason**, so the install gate silently falls back to generic copy and a user is asked for a token with no explanation. The refetch prunes the map to the credentials actually declared; the dialog names the ones still lacking copy.

**Implementation default:** pruning happens in the refetch statement, not in the handler — the map and the declared set are written by the same statement, so the intersection is enforced where it cannot drift.

- **Dimension 4.1** — a refetch drops reason keys absent from the new bundle's declared credentials and preserves the rest → Test `test_refetch_prunes_stale_reason_keys`
- **Dimension 4.2** — the edit dialog marks every declared credential that has no reason text, so the operator can see what the install gate will not explain → Test `test_edit_dialog_flags_unexplained_credentials`

### §5 — The surface says what it means

Three copy and affordance faults on the admin page. The button reads **Add fleet** for an action that adds neither a fleet (a fleet exists only once a workspace installs one) nor, on this page, anything but a library — while the tenant surface already spells it `Create fleet library`. The repository is inert text an operator cannot click through to verify. And the page title is plural where the entity is singular.

**Implementation default:** the repository links only when its value is `owner/repo` shaped — a row imported from a template or an upload carries a source that is not a GitHub slug, and must render as plain text rather than a broken link.

- **Dimension 5.1** — the repository cell links to the GitHub repository when the value is `owner/repo` shaped, and renders inert text when it is not → Test `test_repository_cell_links_only_when_slug_shaped`
- **Dimension 5.2** — the admin button and its dialog read `Create fleet library`, matching the tenant surface → Test `test_admin_button_reads_create_fleet_library`

## Interfaces

```
PATCH /v1/admin/fleet-libraries/{id}          scope: platform-library:write
  body (all fields optional; absent ⇒ untouched):
    { name?: string,
      description?: string,
      source_repo?: string,        // "owner/repo"
      source_ref?: string,         // branch or tag
      required_credentials_reasons?: { [credential: string]: string },
      published?: boolean }
  200 → the updated CatalogEntry (unchanged shape)
  400 UZ-BUNDLE-001   malformed source_repo / source_ref
  404 UZ-CATALOG-001  no entry with that id (also: the row raced away mid-PATCH)
  409 UZ-CATALOG-002  publish attempted on a row with no bundle (current_state: "no_bundle")

  `id` is a path parameter and is never patchable.
  A CHANGED source_repo or source_ref sets content_hash = NULL and visibility = draft
  in the same statement. An unchanged value leaves both untouched.

Derived status (UI, never a wire field) — total over (visibility × has-bundle):
  public  + bundle  → published
  public  + none    → the new fault state   ← the hole this spec closes
  draft   + bundle  → draft
  draft   + none    → no_bundle
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Malformed source | `source_repo` is not `owner/repo`, or a segment fails `validSegment` (empty, `.`/`..`, over-length, bad charset) | 400 `UZ-BUNDLE-001`; row untouched; the dialog names the offending field. |
| Empty / over-cap name | `name` is `""` or exceeds the column's practical cap | 400 `ERR_INVALID_REQUEST`; row untouched. |
| Row vanished mid-PATCH | Concurrent DELETE between the read and the write | The write's `RETURNING id` yields zero rows ⇒ `CatalogRaced` ⇒ 404 `UZ-CATALOG-001`. |
| Concurrent refetch vs source edit | An operator edits the source while a refetch for the old source is in flight | Both writes are single statements inside one transaction; last-writer-wins on the row, and the invalidation cannot be half-applied (`content_hash` and `visibility` move together or not at all). |
| Publish with no bundle | An operator publishes a row whose bundle was never fetched (or was just invalidated by §2) | Existing 409 `UZ-CATALOG-002` with `current_state: "no_bundle"`; the SQL guard refuses independently of the handler. **Regression — must still hold.** |
| Delete a published row | An operator deletes without withdrawing | Existing 409 `UZ-CATALOG-003` with `current_state: "public"`. **Regression — must still hold.** |
| Source change withdraws a live fleet | The operator repoints a published row's repository | Intended: the row drops to `draft`, leaves every gallery, and new installs 404. **Existing installs are unaffected** — they hold their own pinned `content_hash`. The dialog warns before saving. |
| Non-slug source | A template/upload-sourced row carries a source that is not `owner/repo` | The repository cell renders inert text, never a link to a nonexistent GitHub page. |
| New bundle declares no credentials | A refetch whose bundle dropped every credential | The reason map prunes to `{}`; the dialog renders no reason fields. Not an error. |

## Invariants

1. **A row with no bundle never renders as published.** Enforced by `catalogStatus()` being total over the four `(visibility × has-bundle)` combinations and by the exhaustive `Record<CatalogStatus, StatusView>` — a new state that forgets its view fails the type check, not review.
2. **A public row always has a bundle.** Enforced in SQL, not the handler: `UPDATE_CATALOG_VISIBILITY`'s existing `AND ($2 <> $4 OR content_hash IS NOT NULL)` guard is retained verbatim.
3. **A row never advertises a source it is not serving.** Enforced by the single statement that writes the source: it sets `content_hash = NULL` and `visibility = draft` in the same `UPDATE`, so no interleaving observes a changed source beside a stale hash.
4. **The slug is immutable.** Enforced by `id` being a path parameter absent from `PatchBody`; `ignore_unknown_fields` discards any `id` a caller sends.
5. **Reason keys are a subset of declared credentials.** Enforced in the refetch statement, which writes the pruned map and the declared set together.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `platform_library_source_changed` | ops | An operator PATCHes a *changed* `source_repo` or `source_ref` — the act that withdraws a fleet from every workspace gallery | `entry_id`, `field` (repo/ref), `was_published` (bool), `outcome` (success/failure) | No repository contents, no credential names, no token material — the entry slug and the coarse outcome only | `test_source_change_emits_operator_event` |
| `platform_library_published` | ops | Unchanged — the existing publish/unpublish event | Unchanged | Unchanged | Existing coverage retained |

A source change is the one operator action here that silently removes a fleet from every tenant's gallery; it is the only new signal worth carrying. Editing copy is not instrumented — it changes nothing a tenant can observe until publish. No funnel changes, so no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_catalog_status_public_without_bundle` | `{visibility: "public", content_hash: null}` → the fault state, not `published`; all four combinations covered exhaustively. |
| 1.2 | unit | `test_status_badge_never_claims_live_without_bundle` | Rendering a public bundle-less row never emits the "live in every workspace gallery" help text. |
| 1.3 | unit | `test_row_actions_broken_state_recoverable` | The fault state offers Unpublish + Fetch, and offers neither Publish nor Delete. |
| 2.1 | unit | `test_patch_accepts_name` | `{name: "Reviewer"}` persists; `{name: ""}` → 400, row unchanged. |
| 2.2 | unit | `test_patch_rejects_malformed_source` | `"no-slash"`, `"a//b"`, `"../etc"`, a 200-char segment → 400 `UZ-BUNDLE-001`; row unchanged in every case. |
| 2.3 | integration | `test_patch_source_change_invalidates_bundle` | A published row with a hash, PATCHed to a new repo → `content_hash IS NULL` **and** `visibility = 'draft'`, read back from Postgres. |
| 2.4 | integration | `test_patch_unchanged_source_is_noop` | The same published row PATCHed with its *current* repo → hash and visibility both unchanged; the fleet stays live. |
| 2.5 | unit | `test_patch_cannot_move_slug` | A body carrying `id: "other"` → the row keeps its original slug; no second row appears. |
| 3.1 | integration | `test_refetch_preserves_operator_name` | Import → rename via PATCH → refetch the same repo → the operator's name survives; a *first* import still takes the bundle's name. |
| 4.1 | integration | `test_refetch_prunes_stale_reason_keys` | Reasons `{a, b}` + a new bundle declaring only `{b}` → the stored map is exactly `{b}`; `a` is gone. |
| 4.2 | unit | `test_edit_dialog_flags_unexplained_credentials` | A bundle declaring a credential with no reason text → the dialog marks it; one with copy is not marked. |
| 5.1 | unit | `test_repository_cell_links_only_when_slug_shaped` | `"agentsfleet/github-pr-reviewer"` → a link to the GitHub repository; a non-slug source → inert text, no anchor. |
| 5.2 | unit | `test_admin_button_reads_create_fleet_library` | The admin page's add affordance reads `Create fleet library`; the string "Add fleet" appears nowhere on the surface. |
| — | e2e | `test_e2e_operator_corrects_mistyped_repository` | The operator adds a fleet from a wrong repo, publishes, discovers it, edits the repository, watches the row fall to draft, refetches, republishes — the whole recovery path this spec exists to enable. |
| — | integration | `test_publish_without_bundle_still_refused` | **Regression**: `UZ-CATALOG-002` still fires — the widened PATCH did not weaken the publish guard. |
| — | integration | `test_delete_published_still_refused` | **Regression**: `UZ-CATALOG-003` still fires — the widened PATCH did not weaken the delete guard. |
| — | integration | `test_existing_install_survives_source_change` | **Regression**: a workspace that installed the fleet keeps running its pinned `content_hash` after the catalog row's source is repointed and invalidated. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No catalog row can render as published without a bundle (§1) | `cd ui/packages/app && bun test tests/ app/ -t "status"` | exit 0 | P0 | |
| R2 | A source change invalidates the bundle and withdraws the row (§2) | `make test-integration` | exit 0 | P0 | |
| R3 | An operator rename survives a refetch (§3) | `make test-integration` | exit 0 | P0 | |
| R4 | The publish-needs-a-bundle and delete-needs-withdrawal guards still hold (regression) | `make test-integration` | exit 0 | P0 | |
| R5 | The admin surface says `Create fleet library` and nowhere says `Add fleet` (§5) | `git grep -rn "Add fleet" -- ui/ \| wc -l` | `0` | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (HTTP + schema touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the operator's recovery path | `cd ui/packages/app && bun run test:e2e -- platform-library-onboarding` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep — the old title constant is gone | `git grep -rn "FLEET_LIBRARIES_TITLE" -- ui/ \| wc -l` | `0` | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `FLEET_LIBRARIES_TITLE` | `git grep -rn "FLEET_LIBRARIES_TITLE" -- ui/` | 0 matches |
| `ADD_FLEET` | `git grep -rn "ADD_FLEET\b" -- ui/` | 0 matches |

## Out of Scope

- **Upgrading an existing install to a newer bundle.** A workspace's fleet is pinned to the `content_hash` it installed and never drifts. There is no upgrade path today; this spec does not add one. It is a real gap and wants its own milestone.
- **Reclaiming orphaned bundle objects.** A refetch writes a new content-addressed object and abandons the old one; nothing deletes it. A storage-reaper is separate work.
- **Editing the slug.** Permanently rejected, not deferred: workspace installs reference it as `platform_library_id`.
- **Pinning a ref at import.** `POST` still hardcodes `main`; only `PATCH` learns `source_ref`. Teaching the import path to accept a ref is a coherent follow-on but is not needed to correct a mistyped repository.
- **Mocking the object store for local development.** With `R2_*` unset, importing a bundle carrying support files returns 503 `UZ-BUNDLE-005`. Pre-existing; unchanged here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A platform operator notices the reviewer fleet points at `agentsfleet/github-pr-reviewr`, opens Edit, fixes one character, and watches the row drop to draft with an honest "no bundle" badge. They hit Fetch bundle, then Publish. The curated install-gate copy they wrote weeks ago is still there.
2. **Preserved user behaviour** — Add, fetch, publish, unpublish, delete, and the description/credential-reason editing all keep working exactly as they do today. Every existing workspace install keeps running its pinned bundle regardless of what the operator does to the catalog row. The publish-needs-a-bundle and delete-needs-withdrawal guards are untouched.
3. **Optimal-way check** — The most direct shape would let the operator fix the repository *and* refetch in one action, with no window where the fleet is withdrawn. We take the two-step (invalidate, then fetch) because doing the network fetch inside a PATCH puts a fallible GitHub call inside a database transaction and invents a new class of half-applied failure. The window is acceptable: the row is already broken when the repository is wrong.
4. **Rebuild-vs-iterate** — Iterate. The catalog's field-ownership model (bundle owns identity, operator owns copy) is sound; this spec moves three fields across that line and makes the derived status total. Nothing about the lifecycle needs rebuilding.
5. **What we build** — A fourth derived status; three new PATCH fields with source-invalidation; `name` removed from the refetch overwrite; reason-map pruning; a repository link; corrected button copy.
6. **What we do NOT build** — Bundle upgrade for existing installs (needs its own milestone). Orphan-object reclamation (storage concern). Slug editing (would orphan installs). Ref-pinning at import (not needed for the recovery path). A refetch-inside-PATCH (see item 3).
7. **Fit with existing features** — Compounds with the install gate: the credential reasons the operator curates here are what a user reads when a fleet asks for their token, so keeping that map correct across a repository change directly protects the install experience. The one feature it must not destabilize is **install** — the gallery and install queries are the contract this status fix is being made *honest against*, and their behaviour does not change.
8. **Surface order** — UI-first, justified: this is a platform-operator dashboard surface with no CLI equivalent today; the API widening exists to serve it.
9. **Dashboard restraint** — The un-explained-credential marker (4.2) states a fact the row already carries; it makes no quality claim and offers no control. The new status is a fault indicator, not a feature.
10. **Confused-user next step** — An operator who lands on the fault state reads a badge that names the problem ("no bundle has been fetched") and sees exactly two affordances: Fetch bundle, or Unpublish. Both are self-serve, and both resolve the state.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five Sections split by the *fault each one closes*, not by layer — status honesty (§1), identity ownership (§2), rename durability (§3), credential-copy drift (§4), surface copy (§5). §3 and §4 exist only because §2 does: making the source editable is what makes a rename revertible and a reason key stale. Sequencing them behind §2 keeps that causality legible to the implementing agent, and each is independently testable.
- **Alternatives considered:** (a) **Refetch inside the PATCH** — no withdrawal window, but it puts a fallible network call inside a transaction and invents a half-applied failure class the codebase has nowhere else. Rejected. (b) **Copy-and-ref only** — do not make `name`/`source_repo` editable; repoint via the existing `replace: true` add flow. Smallest possible diff and breaks no invariant, but it leaves the actual complaint (a mistyped repository is uncorrectable without discarding curated copy) unsolved. Rejected. (c) **Status fix alone** — honest badge, no editing. Fixes the lie but not the trap. Rejected as half the job.
- **Patch-vs-refactor verdict:** this is a **patch** because the field-ownership model it extends already exists and already works — `description` has been operator-owned, refetch-durable, and excluded from the conflict SET since M128. `name` acquires the identical treatment through the identical mechanism. No new abstraction is introduced; one derivation becomes total and one write path widens.

## Discovery (consult log)

- **Consults** — Indy chose the invalidate-and-unpublish model over refetch-inside-PATCH in session (Jul 13, 2026), reasoning that a mistyped repository link is the common case and a fleet briefly leaving the gallery beats one silently serving the wrong repository's bytes. Indy also flagged the credential-textbox question that surfaced the stale-reason-key leak (§4) — a bug that exists only because §2 makes the source editable.
- **Metrics review** — one new operator event (`platform_library_source_changed`); no funnel change, so no analytics/funnel playbook update is required.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: pending.
- **Deferrals** — none.
