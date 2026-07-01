<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M103_001: Two-tier Fleet template catalog with R2-canonical admin onboarding

**Prototype:** v2.0.0
**Milestone:** M103
**Workstream:** 001
**Date:** Jun 29, 2026
**Status:** DONE
**Priority:** P1 — operator- and user-facing: admins onboard templates, users consume installed bytes without uploading.
**Categories:** API, DOCS, INFRA
**Batch:** B1 — template catalog consolidation.
**Branch:** feat/m103-two-tier-template-catalog
**Test Baseline:** unit=2190 integration=210
**Depends on:** M104_001 (scope-based authorization — platform route gates on `platform-template:write`, tenant route on `template:write`; M104_001's provisional `template:admin` is renamed to `platform_template_write` here); M94_002 (Fleet Bundle import, runner materialization, install preview already exist). Supersedes M96_001 (R2-canonical store) — its R2-only content decision is absorbed here; M96_001 is already in `done/` as DEFERRED.
**Provenance:** agent-generated (Indy design chat, Jun 29, 2026)

> **Provenance is load-bearing.** LLM-drafted — cross-check every claim against the codebase before EXECUTE; the design decisions below were Indy-approved in-session but the file pointers must be re-verified.

**Canonical architecture:** `docs/architecture/fleet_bundles.md` and `docs/architecture/data_flow.md` — bundle/fleet split, import snapshot storage, runner materialization path.

This spec uses Cloudflare R2 object storage (R2), Pull Request (PR), Command-Line Interface (CLI), User Interface (UI), Representational State Transfer (REST), Secure Hash Algorithm 256-bit (SHA-256), Universally Unique Identifier version 7 (UUIDv7), and Foreign Key (FK) below.

---

## Implementing agent — read these first

1. `schema/028_fleet_bundle_templates.sql` — the curated platform catalog (slug-keyed, `visibility`, the gallery shop-window). Its header records the eng-review FINAL decisions this spec partially reverses; read it before editing.
2. `src/agentsfleetd/fleet_bundle/{importer.zig,github_source.zig,store.zig}` — content-hash derivation, canonical tar packing, and the bundle row insert/fetch the onboarding paths reuse.
3. `src/agentsfleetd/auth/{principal.zig,scopes.zig,middleware/require_scope.zig}` (delivered by M104_001) — `principal.scopes` and the `requireScope` gate the two onboarding routes use; the platform route gates on `platform-template:write` (replacing M104_001's provisional `template:admin`), the tenant route on `template:write`.
4. `src/agentsfleetd/http/handlers/fleet_bundles/{imports.zig,get.zig,resolve.zig}` and `runner/bundles.zig` — current import/detail responses and the runner-plane R2 proxy by content hash.
5. `docs/SCHEMA_CONVENTIONS.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, `dispatch/write_zig.md` — schema, REST, and Zig rules for this diff.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Add two-tier Fleet template catalog with R2-canonical onboarding
- **Intent:** Platform admins and tenant admins onboard Fleet templates once; every user installs them straight from the internal R2 snapshot without ever uploading bytes, so runtime is stable, deduped, and not tied to a live GitHub branch.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists `ASSUMPTIONS I'M MAKING: …`. A mismatch with the Intent above stops edits until reconciled.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — A tenant admin onboards `github-pr-reviewer` from a GitHub repository; minutes later a teammate (a plain user) opens the gallery, sees it beside the platform templates, installs it, and gets a Fleet webhook URL — never touching an upload field or an R2 path.
2. **Preserved user behaviour** — Platform gallery, credential preview, runner bundle cache, live `SKILL.md`/`TRIGGER.md` editing of an installed Fleet, and direct Fleet runtime all keep working. GitHub-at-install and paste-`SKILL.md` create are removed: onboard a template (platform or tenant) first, then install it.
3. **Optimal-way check** — Admins onboard via a GitHub source-ref; bytes land in R2 once keyed by content hash; the gallery unions platform + own-tenant rows; users install by reference. The gap (no archive upload, no resync) is acceptable: neither changes storage ownership.
4. **Rebuild-vs-iterate** — No larger rewrite. M94_002 already content-addresses R2 snapshots and materializes them on runners; this adds the tenant tier, the admin write paths, and drops the duplicated Postgres content copy.
5. **What we build** — Tenant template table, two scope-gated onboarding routes, R2-only bytes with metadata-only Postgres rows, a visibility-unioned gallery (carrying description + credential-reason copy), a two-tier (platform | tenant) template install surface, reconciled architecture docs.
6. **What we do NOT build** — Archive upload; resync; paste-`SKILL.md` create; Memory Milestone 1; action-broker hardening; marketplace ranking.
7. **Fit with existing features** — Compounds with Fleet Bundle import and runner materialization; must not destabilize direct Markdown Fleets or live-edited instructions (runtime still reads live `SKILL.md`/`TRIGGER.md` from `core.fleets`).
8. **Surface order** — API first; UI keeps the install flow and stops exposing any R2 path-like field; CLI changes only if response types force a renderer update.
9. **Dashboard restraint** — The gallery shows template identity, source, requirements, and support-file names/sizes — never R2 keys, paths, or object-store controls.
10. **Confused-user next step** — Docs answer "where did my GitHub repo go?": GitHub stays the public source; `agentsfleet` keeps an immutable internal snapshot; edit `SKILL.md`/`TRIGGER.md` via Fleet update, re-onboard to change support files.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — `NDC` (No Dead Code), `NLR` (touch-it-fix-it), `NLG` (no new legacy framing), `UFS` (unified symbols), `ORP` (orphan sweep), `FLL` (file/function length), `PSR` (standard parsers), `ECL` (error classes), `VLT` (secrets in vault), `STS` (no static strings in SQL schema), `NSQ` (schema-qualified SQL), `MIG` (migration index assertions), `ITF` (real integration fixtures), `DRAIN` (Postgres drain-before-deinit), `XCC` (cross-compile), `TST-NAM` (milestone-free test names), `ERR` (error registry), `LOG` (logging discipline), `PRI` (prompt-injection resistance).
- **`dispatch/write_zig.md`** — importer, store, onboarding handlers, runner proxy, materialization tests.
- **`dispatch/write_auth.md`** + **`docs/AUTH.md`** — the two onboarding routes are auth-gated. Per M104_001 (scope-based authz), the platform route gates on the `platform-template:write` capability scope (replacing M104_001's provisional `template:admin` — M104_001 anticipated this reconciliation at M103 time); the tenant route gates on `template:write` plus the unchanged workspace-ownership check. The two scopes are independent (no hierarchy). AUTH.md (rewritten by M104_001) governs and is updated for the rename.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — new onboarding routes and any import/detail response schema or OpenAPI change.
- **`docs/SCHEMA_CONVENTIONS.md`** — new tenant table, `schema/027`/`028` edits, `schema/embed.zig`, migration array assertions.
- **`dispatch/write_ts_adhere_bun.md`** — only if UI/CLI types or renderers change after the public response shape settles.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Read `dispatch/write_zig.md`; split importer/store/handler edits; cross-compile both Linux targets. |
| PUB / Struct-Shape | yes | Shape verdict on new public structs (tenant template row, onboarding request/response); avoid `pub` unless imported. |
| File & Function Length | yes | Split onboarding validation, R2 storage, response shaping, and tests before any file nears the cap. |
| UFS | yes | Source kinds, visibility values, object-key parts, response field and manifest keys, error labels as named constants. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | Registry entries for new failures; drain queries via existing patterns; schema edits single-concern. |
| UI Substitution / DESIGN TOKEN | conditional | Fires only if gallery/install dashboard code changes; reuse install components and design-system primitives. |

---

## Overview

**Goal (testable):** A platform admin and a tenant admin each onboard a template by GitHub source-ref; the bytes land in R2 once under `fleet-bundles/sha256/{content_hash}.tar`; the gallery returns installable (onboarded) platform templates to everyone and a tenant's templates only to that workspace; a plain user installs either and runs the Fleet — and no Postgres row holds support-file bytes and no public response holds an R2 key.

**Problem:** Templates today are a single curated, migration-seeded global catalog with no tenant-owned tier, and bundle support-file bytes are stored twice — the canonical tar in R2 and full bytes inline in Postgres `support_files_json`. There is no admin onboarding path, the duplicate storage drifts, and the database is a hidden artifact store.

**Solution summary:** Add a tenant-scoped template tier beside the platform catalog, give each tier an scope-gated runtime onboarding route that writes the canonical tar to R2 and persists metadata only, restrict the install surface to the two template tiers, and make the gallery union both tiers by visibility. R2 becomes the sole content store; Postgres holds metadata, a support manifest, and the content hash.

---

## Prior-Art / Reference Implementations

- **Onboarding + hash** — mirror `fleet_bundles/imports.zig` (validate/fetch, R2-put before metadata, return preview) and `fleet_bundle/{github_source,importer}.zig` (validated files re-packed into an agentsfleet tar; content hash over `SKILL.md`, optional `TRIGGER.md`, support paths, support bytes).
- **Tenant-scoped table** — mirror `schema/027_core_fleet_bundles.sql`: UUIDv7 id, `workspace_id UUID NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE`, app-set timestamps, no SQL enum checks or default strings. Platform tier mirrors `schema/028_fleet_bundle_templates.sql`.
- **Authorization** — mirror `src/agentsfleetd/auth/middleware/require_scope.zig` (M104_001) for both routes' `template:write` scope gate; mirror `common_authz.zig::authorizeWorkspace` for the tenant route's ownership check.
- **Runner read** — preserve `runner/bundles.zig` + `src/runner/bundle_extract.zig`: download and cache by content hash.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M103_001_*.md` (CREATE) · `docs/v2/done/M96_001_*.md` (MOVE) | — | This spec; retire superseded R2-canonical spec as DEFERRED. |
| `docs/architecture/{fleet_bundles,data_flow}.md`, `scenarios/gh-pr-reviewer.md` | EDIT | R2-canonical + two-tier model; onboard-vs-install storage roles; admin-onboard-vs-user-install, no resync. |
| `schema/029_core_tenant_fleet_bundle_templates.sql` | CREATE | Tenant tier: workspace-scoped, runtime-mutable, content-hash + manifest; `description` column for gallery parity (Dimension 5.4). |
| `schema/027_core_fleet_bundles.sql` | EDIT | Full support-file content → manifest metadata. |
| `schema/028_fleet_bundle_templates.sql` | EDIT | Content-hash + manifest columns; grant `api_runtime` INSERT/UPDATE (consistent with `core.fleet_bundles`); writes gated in-handler by the `platform-template:write` scope. |
| `src/agentsfleetd/auth/scopes.zig`, `scopes_test.zig`, `http/test_scope_tokens.zig` | EDIT | Rename `template_admin` → `platform_template_write` (wire `platform-template:write`); remove hierarchy entry (independent of `template_write`); update test fixtures. M104_001 anticipated this reconciliation. |
| `scripts/mint-scope-personas.mjs`, `docs/AUTH.md`, `playbooks/founding/03_priming_infra/001_playbook.md` | EDIT | Replace `template:admin` wire string with `platform-template:write` in scope minting, auth docs, and priming playbook. |
| `schema/embed.zig`, `src/agentsfleetd/cmd/common.zig` | EDIT | Migration embedding + index assertions stay aligned. |
| `src/agentsfleetd/fleet_bundle/{importer,store}.zig` | EDIT | Manifest + content hash, no support bytes in Postgres; metadata-only rows. |
| `src/agentsfleetd/http/handlers/fleet_bundles/{imports,get,resolve}.zig` | EDIT | R2-before-metadata; previews without R2 key; onboarding source resolution. |
| `src/agentsfleetd/http/handlers/templates/**` | CREATE | Two onboarding routes (platform + tenant) and the gallery union read; gallery entries carry `description` + `required_credentials_reasons` (Dimension 5.4). |
| `src/agentsfleetd/http/handlers/runner/bundles.zig` | EDIT | Runner-only R2 proxy by content hash; drop Postgres content fallback. |
| `src/agentsfleetd/http/handlers/fleets/create*.zig` | EDIT | Restrict install to the two template tiers (platform \| tenant); remove paste-`SKILL.md` and github-import create. |
| `src/runner/bundle_extract.zig` | EDIT | Preserve cache/materialization; add regression coverage. |
| `public/openapi/**` | EDIT | Document onboarding routes; remove public `snapshot_key`; keep identity + summaries. |
| `ui/packages/app/**`, `agentsfleet/**` | EDIT | Dashboard install flow rewritten to a template-only gallery pick (`POST /fleets {platform_template_id\|tenant_template_id}` keyed off `visibility`); `lib/api/fleet-bundles.ts` → `fleet-templates.ts` against the workspace gallery; paste/github affordances removed; cards render `description`. CLI `install --from` removed — `install --template <id>` resolves the workspace gallery and installs by tier; `fleet update --from` (live-edit) retained. |
| `src/agentsfleetd/**/*test.zig`, `ui/packages/app/tests/**`, `agentsfleet/test/**` | EDIT/CREATE | Cover onboarding, visibility, metadata-only storage, restricted install, materialization. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Two tables (platform + tenant) with two scope-gated onboarding routes and a unioned gallery read; R2 owns support-file bytes; Postgres owns metadata, source provenance, requirements, manifest, content identity, and visibility scope.
- **Alternatives considered:** One shared table with a nullable `workspace_id` + `visibility` predicate — rejected: a nullable FK can't express a real cascade, and a single write grant lets a tenant path touch global rows. Keeping templates migration-only — rejected: the product needs runtime admin onboarding. Archive-upload onboarding — deferred: it adds a multipart attack surface without changing storage ownership.
- **Patch-vs-refactor verdict:** a **targeted refactor** — it adds a tier and write paths and changes storage ownership around one install path, while preserving Fleet creation, runner leasing, and the platform gallery. The `028` "SELECT-only / migration-curated" note updates: the platform catalog becomes an app-written table like `core.fleet_bundles` (grant `api_runtime` INSERT/UPDATE), with writes gated **in the handler** by the `template:write` scope (M104_001). Seed rows stay as bootstrap.

---

## Sections (implementation slices)

### §1 — Tenant template tier (metadata-only, workspace-scoped)

A new workspace-owned template table beside the platform catalog, carrying enough metadata for gallery, install, idempotency, and audit — no support-file bytes. **Implementation default:** UUIDv7 id and `ON DELETE CASCADE` on `workspace_id`, mirroring `core.fleet_bundles`.

- **Dimension 1.1** — Tenant template row stores identity, `workspace_id`, source provenance, `content_hash`, support manifest, requirements, and visibility — and no support-file body text → Test `test_tenant_template_row_excludes_support_content`
- **Dimension 1.2** — `content_hash` derives from `SKILL.md`, optional `TRIGGER.md`, support paths, and support bytes, byte-identical to the bundle importer → Test `test_content_hash_stable_across_tiers`
- **Dimension 1.3** — Re-onboarding identical bytes into the same workspace converges on one `(workspace_id, content_hash)` row without mutating R2 → Test `test_tenant_onboard_dedupes_by_workspace_and_hash`

### §2 — Two scope-gated onboarding routes

Platform and tenant admins onboard by GitHub source-ref; each route validates, fetches, writes the R2 snapshot, then commits its metadata row. **Implementation default:** reuse `github_source` + the importer; no archive upload. The platform route gates on the `platform-template:write` capability scope (M104_001's `template:admin` renamed — the reconciliation M104_001 anticipated); the tenant route gates on `template:write` plus ownership of the target workspace. The two scopes are independent (no hierarchy).

- **Dimension 2.1** — Platform onboarding requires the `platform-template:write` scope; a principal lacking it is rejected and writes nothing → Test `test_platform_onboard_requires_platform_template_write`
- **Dimension 2.2** — Tenant onboarding requires the `template:write` scope plus ownership of the target workspace, and writes only its own `workspace_id`; a principal without scope or ownership is rejected → Test `test_tenant_onboard_requires_scope_and_ownership`
- **Dimension 2.3** — Onboarding writes `fleet-bundles/sha256/{content_hash}.tar` to R2 before any metadata commit; an injected R2 put failure leaves no row → Test `test_onboard_writes_r2_before_metadata`
- **Dimension 2.4** — Skill-only template (no support files) onboards without requiring an R2 object → Test `test_skill_only_template_onboard_needs_no_r2`

### §3 — R2 is the only support-file content store

R2 holds the canonical agentsfleet tar; Postgres bundle and template rows hold manifest metadata only; the runner downloads by content hash. Absorbs M96_001.

- **Dimension 3.1** — `core.fleet_bundles` stores support-file path/size/hash manifest, not bytes → Test `test_bundle_store_excludes_support_file_content`
- **Dimension 3.2** — Runner bundle proxy serves only from R2 by validated `content_hash`, failing closed when the object is missing → Test `test_runner_bundle_proxy_uses_r2_only`
- **Dimension 3.3** — Public import/detail and gallery responses omit `snapshot_key` and expose `content_hash` plus support summaries → Test `test_public_responses_hide_r2_key`

### §4 — Install surface = the two template tiers

Creating a Fleet accepts exactly one onboarded template tier — a platform template (slug id) or this workspace's tenant template (UUIDv7). The legacy github-import-at-install, the per-workspace `bundle_id`, and raw-`SKILL.md` paste create are all removed (onboard a template first, then install it); editing an installed Fleet's live instructions is untouched.

- **Dimension 4.1** — Install accepts exactly one of `{platform_template_id, tenant_template_id}` and rejects a raw-`SKILL.md` paste, a legacy `bundle_id`, or a github-import create payload → Test `test_install_rejects_manual_skill_create`
- **Dimension 4.2** — Installing a tenant template from another workspace is rejected; a user installs only platform templates or their own workspace's → Test `test_install_enforces_template_visibility`
- **Dimension 4.3** — Editing an already-installed Fleet's live `SKILL.md`/`TRIGGER.md` still overrides tar copies; support files come from the immutable snapshot → Test `test_live_fleet_markdown_remains_authoritative`

### §5 — Gallery read model (visibility union)

The gallery returns the union of installable (onboarded) platform templates and the requesting workspace's own templates, and nothing from other workspaces. A migration-seeded platform row is metadata-only (no `content_hash`) until onboarded; the gallery hides it until then, mirroring `fetchPlatformInstall` so no entry can dead-end at install.

- **Dimension 5.1** — Gallery for workspace W returns all installable (onboarded) platform templates plus W's tenant templates → Test `test_gallery_unions_platform_and_own_tenant`
- **Dimension 5.2** — Gallery for W never returns another workspace's tenant templates → Test `test_gallery_isolates_tenant_templates`
- **Dimension 5.3** — Gallery entries carry identity, source, requirements, and support summaries — no R2 path-like field → Test `test_gallery_entries_hide_object_keys`
- **Dimension 5.4** — Gallery entries carry `description` and a `required_credentials_reasons` object so the dashboard renders template descriptions and purpose-driven credential copy; platform rows surface their curated columns (onboarding a seed preserves its curated reasons via the `ON CONFLICT` UPSERT), tenant rows surface the onboarded SKILL description and an empty reasons object → Test `test_gallery_entries_carry_description_and_reasons`

### §6 — Documentation and orphan cleanup

Architecture docs become the source of truth for the two-tier, R2-canonical model; stale full-content storage names are swept; M96_001 retires.

- **Dimension 6.1** — Architecture docs define content hash, user-visible GitHub source, internal R2 snapshot, two-tier visibility, and no user-provided R2 path → Test `test_architecture_docs_define_two_tier_r2_model`
- **Dimension 6.2** — Removed `support_files_json` full-content semantics have no production references outside historical specs → Test `test_support_files_content_orphan_sweep`
- **Dimension 6.3** — No resync and no archive-upload route, CLI verb, or UI affordance is added → Test `test_no_resync_or_upload_surface_added`

---

## Metrics & Observability

M103 adds no net-new analytics event: the dashboard install flow reuses the existing `fleet_created` product event, fired once after a template-backed fleet is created (either tier). The two onboarding routes and the gallery read are operator/admin surfaces observed through the service's structured request logs (RULE LOG) — request id, route, workspace/template id, content hash, and outcome — not product analytics. The R2 write rides the existing object-store client logs. The privacy bound is the same one the gallery enforces at Dimension 5.3: no SKILL/TRIGGER content, credential material, or R2 object key reaches an analytics property or a client-facing field.

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet_created` | product | A template-backed fleet is created (platform or tenant tier), after the install server action returns ok | `fleet_id` only | No SKILL/TRIGGER bytes, no credential names or token material — only the resource id | `test_install_lands_in_steer` (asserts `captureProductEvent` fires with the fleet id) |
| onboarding request logs | ops | A platform or tenant onboarding route completes (2xx or error) | request id, route, workspace id, template id, content hash, status | No SKILL/TRIGGER content and no R2 object key in any client-facing field | `onboard_integration_test.zig` (status + persistence assertions) |
| gallery request logs | ops | A workspace gallery read completes | request id, workspace id, row count, status | No object-store key surfaces (Dimension 5.3) | `test_gallery_entries_hide_object_keys` |

---

## Interfaces

```
Onboard (body {source_kind:"github", source_ref:"owner/repo"}; 201 returns id, name,
  visibility, content_hash, requirements, support_files[] — never snapshot_key/R2 path):
  POST /v1/admin/fleet-templates                      requires scope platform-template:write (global)  → visibility "platform"
  POST /v1/workspaces/{workspace_id}/fleet-templates  requires scope template:write + workspace ownership → visibility "tenant"
Gallery (user read):
  GET /v1/workspaces/{workspace_id}/fleet-templates → platform templates ∪ caller-workspace tenant templates;
    each entry: id, name, description, visibility, source_ref, requirements,
    required_credentials_reasons{}, support summaries — never snapshot_key/R2 path.
Install (Fleet create) — source is exactly one of:
  {platform_template_id} | {tenant_template_id}; raw SKILL.md paste, legacy bundle_id, and github-import create all rejected.
Runner (unchanged): GET /v1/runners/me/bundles/{content_hash} → daemon rebuilds R2 key, streams tar.
Storage: core.tenant_fleet_bundle_templates = UUIDv7 id, workspace_id FK (CASCADE), source fields,
  content_hash, support manifest (path/size/hash only), requirements, visibility, timestamps.
  R2 key server-owned: fleet-bundles/sha256/{content_hash}.tar.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Onboard without scope | Principal lacks `platform-template:write` (platform route) or `template:write` (tenant route, or lacks workspace ownership) | 403; no R2 put, no metadata row. |
| R2 unavailable during onboard | Object store client missing or put fails | Storage-unavailable error; no metadata row committed. |
| R2 object missing at run time | Metadata references a `content_hash` with no stored tar | Runner reports startup failure; event log records materialization failure, no secret leak. |
| Cross-tenant install | User installs another workspace's tenant template | 404/403; visibility check rejects; no Fleet created. |
| Manual `SKILL.md` create | Client sends a raw-markdown, `bundle_id`, or github-import create payload | 400 invalid-source; install accepts only the two template tiers. |
| Manifest parse failure | Stored manifest malformed | Detail endpoint returns internal error; tests prove the importer cannot write a malformed manifest. |
| Duplicate onboard race | Same workspace onboards identical content concurrently | Unique `(workspace_id, content_hash)` wins; one `id` returned; R2 put idempotent. |
| Public response leaks R2 key | Handler or generated client keeps `snapshot_key` public | Tests fail; UI must not render object-store internals. |
| GitHub source changes after onboard | Source branch moves | Existing template/Fleet unchanged; re-onboard required for support-file changes. |

---

## Invariants

1. Support-file bytes live in R2 only — enforced by schema shape, store tests, and a production grep for removed content fields.
2. Tenant onboarding requires the `template:write` scope plus ownership of the target workspace, and writes only its own `workspace_id` and only the tenant table — enforced by scope + ownership middleware tests.
3. Platform onboarding requires the `platform-template:write` scope and writes only the platform table — enforced by scope-gate tests; the tenant write path has no grant on the platform table.
4. `content_hash` is content-derived, byte-identical across tiers — enforced by hashing tests importing identical bytes through platform and tenant routes.
5. R2 writes happen before Postgres metadata commits — enforced by failure-injection integration tests.
6. The gallery for workspace W returns no other workspace's tenant templates — enforced by visibility query tests.
7. Public users never provide or receive an R2 path — enforced by OpenAPI/client tests and dashboard rendering tests.
8. Install accepts exactly the two template tiers (platform | tenant); raw-`SKILL.md` paste, legacy `bundle_id`, and github-import create are all impossible — enforced by handler rejection tests.
9. Secrets never enter R2, manifests, public responses, or logs — enforced by importer secret-shape rejection and redaction tests.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_tenant_template_row_excludes_support_content` | Onboard with support files stores path/size/hash, no body text in Postgres. |
| 1.2 | unit | `test_content_hash_stable_across_tiers` | Same bytes via platform and tenant routes yield the same 64-char SHA-256 hex. |
| 1.3 | integration | `test_tenant_onboard_dedupes_by_workspace_and_hash` | Identical bytes onboarded twice into W return one `(workspace_id, content_hash)` row. |
| 2.1 | integration | `test_platform_onboard_requires_platform_template_write` | Principal without `platform-template:write` → 403, no row, no R2 put. |
| 2.2 | integration | `test_tenant_onboard_requires_scope_and_ownership` | Missing scope or non-owned workspace → 403; owner with scope writes only its own `workspace_id`. |
| 2.3 | integration | `test_onboard_writes_r2_before_metadata` | Injected R2 put failure → storage error and no metadata row. |
| 2.4 | integration | `test_skill_only_template_onboard_needs_no_r2` | No support files, no R2 client → onboard and install still succeed. |
| 3.1 | integration | `test_bundle_store_excludes_support_file_content` | Bundle row stores manifest only; no support body text. |
| 3.2 | unit | `test_runner_bundle_proxy_uses_r2_only` | Valid hash fetches R2; invalid rejected; missing object → bundle-not-found. |
| 3.3 | integration | `test_public_responses_hide_r2_key` | Import/detail/gallery JSON has `content_hash` + summaries, no `snapshot_key`. |
| 4.1 | integration | `test_install_rejects_manual_skill_create` | Raw-`SKILL.md`/`bundle_id`/github-import create payload → 400; exactly one of the two template tiers accepted. |
| 4.2 | integration | `test_install_enforces_template_visibility` | Install of another workspace's tenant template → rejected. |
| 4.3 | integration | `test_live_fleet_markdown_remains_authoritative` | Patched instructions ride the lease; support files from the immutable snapshot. |
| 5.1 | integration | `test_gallery_unions_platform_and_own_tenant` | Gallery for W returns all installable (onboarded) platform + W's tenant templates; un-onboarded seeds stay hidden. |
| 5.2 | integration | `test_gallery_isolates_tenant_templates` | Gallery for W excludes another workspace's tenant templates. |
| 5.3 | unit | `test_gallery_entries_hide_object_keys` | Gallery entries carry summaries, no R2 path-like field. |
| 5.4 | integration | `test_gallery_entries_carry_description_and_reasons` | Onboard platform + tenant templates → gallery entries expose `description` and a `required_credentials_reasons` object (platform from columns; tenant description from SKILL, reasons `{}`). |
| 6.1 | unit | `test_architecture_docs_define_two_tier_r2_model` | Architecture docs contain the two-tier, GitHub-source/R2-snapshot/Postgres-metadata model. |
| 6.2 | unit | `test_support_files_content_orphan_sweep` | Production grep finds no full-content Postgres storage outside historical specs. |
| 6.3 | unit | `test_no_resync_or_upload_surface_added` | No resync or archive-upload route, CLI verb, or UI label in the diff. |

Regression: existing platform gallery, template onboarding, CLI local install, and runner materialization tests stay green.
Idempotency/replay: duplicate onboards converge on one `(workspace_id, content_hash)` row; retry after R2 failure leaves no partial metadata.

---

## Acceptance Criteria

- [ ] Tenant template tier exists, workspace-scoped, metadata-only — verify: `make test-integration` and production grep for content fields.
- [ ] Both onboarding routes enforce the `template:write` scope and write R2 before metadata — verify: `make test-integration`.
- [ ] Gallery unions platform + own-tenant templates and isolates other tenants — verify: `make test-integration`.
- [ ] Install accepts exactly the two template tiers; raw-`SKILL.md`/`bundle_id`/github-import create is rejected; live-edit unaffected — verify: `make test-integration`.
- [ ] Public responses hide R2 keys and keep `content_hash` + summaries — verify: `make check-openapi && make test-unit-agentsfleetd`.
- [ ] Runner downloads by `content_hash` and extracts support files — verify: `make test-unit-agentsfleetd && make test-integration`.
- [ ] No resync or archive-upload surface added — verify: grep routes, CLI, UI labels.
- [ ] Repository gates pass — verify: `make lint && make test && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect`.

---

## Eval Commands (post-implementation)

```bash
make test-unit-agentsfleetd && make test-integration && make check-openapi && make check-pg-drain && make lint
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect 2>&1 | tail -3
# 350-line gate (exempts .md):
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# Orphan sweep:
rg -n "support_files_json|snapshot_key|resync" src schema public ui agentsfleet docs/architecture | head
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

No files are deleted; M96_001 moves `pending/` → `done/` (record, not deletion).

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `support_files_json` as full content | `rg -n "support_files_json\|support file content\|full support-file" src schema public ui agentsfleet docs/architecture` | No production semantics saying Postgres stores support-file bodies. |
| Public `snapshot_key` response field | `rg -n "snapshot_key" public ui agentsfleet src/agentsfleetd/http/handlers` | Internal server/storage references only. |
| Manual `SKILL.md` create path | `rg -n "skill_markdown" src/agentsfleetd/http/handlers/fleets` | No create-by-paste authoring entry; live-edit untouched. |
| Resync / archive-upload surface | `rg -n "resync\|upload_archive\|archive_upload" src agentsfleet ui public docs/architecture` | Historical/design mentions only; nothing added by this work. |

---

## Discovery (consult log)

- Architecture consult, Jun 29, 2026: grounded in `docs/architecture/fleet_bundles.md` and `docs/architecture/data_flow.md`. Decision: GitHub remains the user-visible source; R2 is the internal canonical store; Postgres holds metadata only; templates split into a migration-bootstrapped platform tier and a runtime-onboarded tenant tier.
- Design decision (Indy, Jun 29, 2026): two tables, not one — tenant tier carries a `workspace_id` FK with cascade; platform tier keeps its slug-keyed shape. Both tiers onboard at runtime via separate scope-gated routes.
- Auth design (Indy, Jun 29, 2026): authorization moves from roles to explicit scopes (M104_001, a dependency of this spec). Platform route gates on `platform-template:write` (M104_001's provisional `template:admin` renamed — the reconciliation M104_001 anticipated at M103 time); tenant route gates on `template:write` + workspace ownership. The two scopes are independent (no hierarchy). The earlier in-session `platform_catalog_writer` DB-role idea and the "reuse `platform_admin`/`RequireRole`" interim plan are both withdrawn — superseded by M104_001.
- `028` note update (no longer a "reversal needing ack"): the platform catalog stops being migration-only and becomes an app-written table like `core.fleet_bundles` (grant `api_runtime` INSERT/UPDATE), authorized in-handler by the `platform-template:write` scope. The `028` "SELECT-only" was an anomaly tied to migration-curation, not a security boundary; seed rows stay as bootstrap.
- Deferral quote (resync + archive upload): > Indy (2026-06-22 22:30): "There two design decision currently M1 Memory and Cloudflare R2 is redundant (i am not focussed on resync)" — context: GitHub-to-template resync and archive-upload onboarding stay out of this catalog spec.
- Install-surface reconciliation (Indy, 2026-06-30): the install surface drops from three sources to the two template tiers — github-import-at-install and the legacy `bundle_id` are removed (onboard a template, then install it). Slice E implemented this; §4 / Interfaces / Failure Modes / Invariant 8 / Dimension 4.1 are amended here to match the shipped two-tier surface.
  > Indy (2026-06-30): selected "Template-only is correct" — context: §4 still said "exactly three sources" (incl. github_import) but the shipped backend + OpenAPI (`fleets.yaml`) accept only `{platform_template_id | tenant_template_id}`; confirmed the Slice E removal was intended, so the spec is corrected rather than the backend reverted.
- Gallery-metadata parity (Indy, 2026-06-30): the gallery entry gains `description` + a `required_credentials_reasons` object so the dashboard keeps template descriptions and purpose-driven connect copy (Dimension 5.4). Platform rows surface their curated columns; tenant rows surface the onboarded SKILL description and an empty reasons object (the importer derives no per-credential reason copy). A `description` column is added to `core.tenant_fleet_bundle_templates` (migration 029, edited in place — branch-only, unreleased).
  > Indy (2026-06-30): selected "Treat as backend gap" — context: `FleetTemplateGalleryEntry` lacked the `description` + `required_credentials_reasons` fields the install UI renders; chose to add them to the gallery contract (OpenAPI + gallery.zig + tenant onboard) rather than degrade the dashboard.
- CLI install surface (Indy, 2026-06-30): `agentsfleet install --from <path>` (local-directory install) is REMOVED outright — `install` is template-only (`agentsfleet install --template <id>`, resolved in the workspace gallery and installed by tier). `agentsfleet fleet update <fleet_id> --from <path>` (live-edit of an installed fleet) is retained; onboarding a local directory as a template is dashboard/API only (no CLI onboard verb). The `~12` docs pages that documented `install --from` as the primary install path are revised across the board. A discovery midway — that `install --from` was the *primary* documented install path, not a secondary one — reversed an earlier "defer `--from`" lean.
  > Indy (2026-06-30): "remove --from, are we having install <template id or fleet_bundle_id?> now? just change it across" — context: confirmed install is by template id (not the removed `bundle_id`); directed removing `install --from` outright and propagating template-only install across the CLI and docs.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification, including scope gates, visibility, metadata-only storage, failure injection. | Clean; final coverage note in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, architecture docs, REST guide, Zig rules, Failure Modes, Invariants. | Clean or every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Reviews the open PR for response-shape drift, schema orphaning, generated-client mismatch. | Comments addressed before human review. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration tests (backend) | `make test-integration` | Full suite passed (DB + Redis) | ✅ |
| OpenAPI bundle + lint | `make check-openapi` | Bundle + Redocly + error-schema + URL-shape green | ✅ |
| Zig lint (fmt, ZLint, pg-drain, test-depth, line-limit, role/legacy guards) | `make lint-zig` | passed | ✅ |
| Schema gate | `make check-schema-gate` | passed | ✅ |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | built | ✅ |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | built | ✅ |
| UI typecheck + lint + vitest | `(ui/packages/app) tsc · oxlint · bun run test` | tsc 0 · lint clean · 1121 tests pass | ✅ |
| CLI typecheck + tests + coverage | `(cli) tsc · bun run test` | tsc 0 · 1252 pass · 100% line/fn coverage | ✅ |
| gitleaks | `gitleaks detect` | no leaks | ✅ |
| Orphan sweep | `rg -n "snapshot_key\|resync" src schema public ui agentsfleet` | only legit internal R2-key derivation; no `resync` | ✅ |
| Test delta (zig) | `make _lint_zig_test_depth` | unit 2190→2196 (+6); integration 210→215 (+5) | ✅ |

**Note — migration 029 edited in place.** The tenant `description` column (Dimension 5.4) was added by editing the unreleased migration 029 (introduced on this branch, not in `main`). Fresh databases — CI, and `main`/production after merge — apply the corrected 029 with the column. A persistent dev DB that already applied the *pre-edit* 029 must reset/re-migrate, because the version-gated runner (`migration_versions.zig`, version-only, no checksum) won't re-run 029. No additive 030 was used: a `030 ADD COLUMN … NOT NULL` would require a static-string `DEFAULT` (forbidden by the schema rules) to be safe against pre-existing rows.

---

## Out of Scope

- Archive/upload onboarding in the dashboard or API.
- GitHub-to-template resync, auto-refresh, or a CLI resync command.
- Memory M1 and cross-fleet compounding memory.
- Action broker, no-network sandbox hardening, and GitHub review action schemas.
- Marketplace quality ranking, install-count sorting, or run-success claims.
