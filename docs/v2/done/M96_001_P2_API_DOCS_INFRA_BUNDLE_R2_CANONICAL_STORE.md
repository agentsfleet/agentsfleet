<!--
Specification (SPEC) AUTHORING RULES (load-bearing - do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority 0 (P0), Priority 1 (P1), Priority 2 (P2), and Priority 3 (P3) are the only sizing signals; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins - delete the section.
- Enforced by Specification Template Gate (SPEC TEMPLATE GATE) in `dispatch/write_spec.md` and `audits/spec-template.sh`,
  which also assert the determinism-critical sections below are present and filled.
-->

# Milestone 96 Workstream 001 (M96_001): Bundle support files use R2 as canonical storage

**Prototype:** v2.0.0
**Milestone:** Milestone 96 (M96)
**Workstream:** 001
**Date:** Jun 22, 2026
**Status:** DEFERRED - superseded by M103_001 (Two-tier Fleet template catalog with R2-canonical admin onboarding), which absorbs this spec's R2-only content decision. Never started; retired as a design record. Reactivation condition: none - the work lands under M103_001.
**Priority:** Priority 2 (P2) - removes duplicate artifact storage from the bundle install path without changing the install user journey.
**Categories:** API, Documentation (DOCS), Infrastructure (INFRA)
**Batch:** Batch 1 (B1) - bundle storage consolidation.
**Branch:** added when work begins
**Depends on:** Milestone 94 Workstream 002 (M94_002) - Fleet Bundle import, runner materialization, and dashboard install preview already exist.
**Provenance:** agent-generated (Indy design chat, Jun 22, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` and `docs/architecture/data_flow.md` - bundle/fleet split, import snapshot storage, and runner materialization path.

This spec uses Cloudflare R2 object storage (R2), Pull Request (PR), Command-Line Interface (CLI), User Interface (UI), Secure Hash Algorithm 256-bit (SHA-256), Representational State Transfer (REST), and Universally Unique Identifier version 7 (UUIDv7) below.

---

## Implementing fleet - read these first

1. `docs/architecture/fleet_bundles.md` - canonical bundle/fleet split, current R2 plus Postgres redundancy, and runtime read path.
2. `src/agentsfleetd/fleet_bundle/importer.zig`, `github_source.zig`, and `store.zig` - current content hash, canonical tar, manifest, and database insert/fetch paths.
3. `src/agentsfleetd/http/handlers/fleet_bundles/imports.zig`, `get.zig`, and `src/agentsfleetd/http/handlers/runner/bundles.zig` - public import/detail responses and runner-plane R2 proxy.
4. `src/runner/bundle_extract.zig` and `src/agentsfleetd/fleet/service.zig` - lease manifest delivery, runner cache, and support-file extraction.
5. `docs/SCHEMA_CONVENTIONS.md`, `docs/REST_API_DESIGN_GUIDELINES.md`, and `dispatch/write_zig.md` - schema, REST, and Zig rules for this diff.

## PR Intent & comprehension handshake

- **PR title (eventual):** Make R2 the canonical Fleet Bundle content store
- **Intent:** Keep GitHub visible as the source users choose, but store installed bundle support-file bytes in exactly one internal artifact store so runtime execution is stable, deduped, and not tied to a live GitHub branch.
- **Handshake (fleet fills during planning, before edits):** the implementing fleet restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: ...`). A mismatch between this restatement and the Intent above stops edits until reconciled.

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** - John installs the `github-pr-reviewer` Fleet from a GitHub repository, sees the same GitHub source and credential preview, receives a Fleet webhook URL, and later a PR review run uses the installed snapshot without John ever seeing or supplying an R2 path.
2. **Preserved user behaviour** - Template cards, public GitHub import, paste install, local CLI install, credential preview, direct Fleet creation, runner bundle cache, and live Fleet `SKILL.md` / `TRIGGER.md` edits keep working.
3. **Optimal-way check** - The direct shape is GitHub for public visibility and authoring, R2 for immutable installed bytes, and Postgres for searchable metadata plus source provenance. The gap is that the source repository still does not auto-refresh an installed Fleet; that is acceptable because resync is not part of this decision.
4. **Rebuild-vs-iterate** - A larger import rewrite is not justified. The existing M94_002 architecture already has content-addressed R2 snapshots and runner materialization; this work removes the duplicated Postgres content copy and hides R2 internals from the user surface.
5. **What we build** - Metadata-only Postgres bundle rows, R2-only support-file bytes, public responses that expose source/provenance and support summaries but no R2 object key, runner-only download by `content_hash`, and architecture docs that explain why GitHub visibility and R2 snapshots both exist.
6. **What we do not build** - GitHub-to-bundle resync; Memory Milestone 1 (M1); action-broker sandbox hardening; upload archive support; native GitHub review tools; marketplace ranking or quality claims.
7. **Fit with existing features** - Compounds with Fleet Bundle import and runner materialization. It must not destabilize direct Markdown Fleets or patched Fleet instructions, because the runtime still reads live `SKILL.md` / `TRIGGER.md` from `core.fleets`.
8. **Surface order** - API first. The UI should keep the same install flow and only stop exposing any R2 path-like field if one leaks through current types. CLI behaviour stays unchanged unless public response types force a renderer update.
9. **Dashboard restraint** - The dashboard shows GitHub source, template identity, missing credentials, support-file names/sizes, Fleet name, and webhook URL. It does not show R2 bucket keys, storage paths, or object-store controls.
10. **Confused-user next step** - The docs answer "where did my GitHub repo go?" with: GitHub remains the public source; `agentsfleet` stores an immutable installed snapshot internally; edit `SKILL.md` / `TRIGGER.md` through Fleet update, and re-import to change support files.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** - Rule `NDC` (No Dead Code), `NLR` (No Legacy Retained), `UFS` (Unified Form for Symbols), `ORP` (orphan sweep), `FLL` (File & Function Length Limits), `PSR` (standard parsers), `ECL` (distinct error classes), `VLT` (secrets in vault), `STS` (no static strings in SQL schema), `NSQ` (schema-qualified SQL), `MIG` (migration index assertions), `ITF` (real integration fixtures), `DRAIN` (Postgres drain-before-deinit), `XCC` (cross-compile), `TST-NAM` (milestone-free tests), `ERR` (error registry), `LOG` (logging discipline), and `PRI` (prompt-injection resistance).
- **`dispatch/write_zig.md`** - applies to importer, store, HTTP handlers, runner bundle proxy, and runner materialization tests.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** - applies if import/detail response schemas or OpenAPI bundle schemas change.
- **`docs/SCHEMA_CONVENTIONS.md`** - applies to `schema/027_core_fleet_bundles.sql`, `schema/embed.zig`, and migration array assertions.
- **`dispatch/write_ts_adhere_bun.md`** - applies only if UI or CLI TypeScript types/renderers need updates after the public response shape is cleaned.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Read `dispatch/write_zig.md`; keep importer/store/handler changes split; run both Linux cross-compiles. |
| Public Surface (PUB) / Struct-Shape | yes | New or changed public Zig structs get a shape verdict; avoid `pub` unless another file imports it. |
| File & Function Length | yes | Split manifest construction, R2 storage, response shaping, and tests before any file approaches the cap. |
| UFS | yes | Keep source kinds, object key parts, response field names, support manifest keys, and error labels as named constants. |
| UI Substitution / DESIGN TOKEN | conditional | Fires only if dashboard code changes; use existing install components and design-system primitives. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | Add registry entries for changed failures, drain queries through existing patterns, and keep schema edits single-concern. |

## Overview

**Goal (testable):** Importing `github-pr-reviewer` stores support-file bytes only in R2, persists only metadata and a `content_hash` in Postgres, and still installs and runs the Fleet without exposing an R2 path to the user.

**Problem:** The current bundle model stores support-file content twice: the canonical tar in R2 and the full support-file bytes inline in Postgres `support_files_json`. That duplicates state, makes the database a hidden artifact store, and confuses the product story: users choose GitHub, but runtime should run from an internal installed snapshot.

**Solution summary:** Make R2 the sole content store for bundle support-file bytes. Postgres keeps bundle metadata, source provenance, requirements, support-file manifest, and `content_hash`. Public API/UI surfaces return `bundle_id`, requirements, source fields, support summaries, and content identity, not R2 object keys. The runner continues to download support files through the daemon proxy by `content_hash`.

## Prior-Art / Reference Implementations

- **Bundle import** - mirror `src/agentsfleetd/http/handlers/fleet_bundles/imports.zig`: validate/fetch first, put R2 snapshot before inserting metadata, return a requirement preview.
- **Canonical tar** - mirror `src/agentsfleetd/fleet_bundle/github_source.zig`: validated files are re-packed into an agentsfleet tar, not a GitHub archive passthrough.
- **Runner materialization** - preserve `src/runner/bundle_extract.zig`: cache by immutable `content_hash`, skip tar `SKILL.md` / `TRIGGER.md`, extract support files into each sandbox workspace.
- **Schema** - mirror `schema/027_core_fleet_bundles.sql` and `docs/SCHEMA_CONVENTIONS.md`: UUIDv7 identifiers, app-set timestamps, no SQL enum checks/default strings.
- **REST API** - keep existing `/v1/workspaces/{workspace_id}/fleets/bundles/snapshots` resource shape; if response fields change, update OpenAPI under the existing bundle tag.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M96_001_P2_API_DOCS_INFRA_BUNDLE_R2_CANONICAL_STORE.md` | CREATE | Track the R2 canonical-store decision and implementation proof. |
| `docs/architecture/fleet_bundles.md` | EDIT | Replace the open redundancy note with the decided R2-canonical storage model. |
| `docs/architecture/data_flow.md` | EDIT | Update install flow so Postgres metadata and R2 content roles are unambiguous. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Clarify user-visible GitHub source vs internal installed snapshot without adding resync. |
| `schema/027_core_fleet_bundles.sql` | EDIT | Replace full support-file content storage with support-file manifest metadata. |
| `schema/embed.zig` and `src/agentsfleetd/cmd/common.zig` | EDIT | Keep migration embedding and index assertions aligned if the schema file changes. |
| `src/agentsfleetd/fleet_bundle/importer.zig` | EDIT | Produce a support-file manifest and content hash without serializing support bytes into Postgres metadata. |
| `src/agentsfleetd/fleet_bundle/store.zig` | EDIT | Persist and fetch metadata-only bundle rows. |
| `src/agentsfleetd/http/handlers/fleet_bundles/imports.zig` | EDIT | Store R2 snapshot before metadata and return public preview fields without R2 object keys. |
| `src/agentsfleetd/http/handlers/fleet_bundles/get.zig` | EDIT | Read support summaries from metadata, not full file content. |
| `src/agentsfleetd/http/handlers/runner/bundles.zig` | EDIT | Preserve runner-only R2 proxy by `content_hash`; remove any Postgres content fallback if present. |
| `src/agentsfleetd/http/handlers/fleets/create*.zig` | EDIT | Ensure Fleet creation uses bundle metadata and `bundle_content_hash`, not embedded support content. |
| `src/runner/bundle_extract.zig` | EDIT | Preserve cache/materialization behaviour and add regression coverage if no code change is needed. |
| `public/openapi/**` | EDIT | Remove public `snapshot_key` exposure if present; keep content identity and support summaries documented. |
| `ui/packages/app/**` and `agentsfleet/**` | EDIT | Update generated/client types or renderers only if the public response no longer carries `snapshot_key`. |
| `src/agentsfleetd/**/*test.zig`, `ui/packages/app/tests/**`, `agentsfleet/test/**` | EDIT/CREATE | Cover metadata-only import, public response shape, runner materialization, and unchanged install flows. |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Targeted storage consolidation: R2 owns support-file bytes; Postgres owns metadata, source provenance, requirements, support summaries, and content identity. GitHub remains the source users understand, not runtime storage.
- **Alternatives considered:** Postgres-only storage was rejected because bundle support files are immutable artifacts, content-addressed, runner-cached, and likely to grow beyond today's cap. Keeping both stores was rejected because duplicate content creates drift and makes failure recovery ambiguous. Resync was rejected for this work because it changes freshness semantics rather than storage ownership.
- **Patch-vs-refactor verdict:** this is a targeted refactor because it changes storage ownership and API shape around one existing install path, while preserving Fleet creation, runner leasing, and dashboard behaviour.

## Sections (implementation slices)

### Section 1 - Metadata-only bundle rows

Postgres stops storing support-file bytes while retaining enough metadata for preview, list/detail, idempotency, and audit.

- **Dimension 1.1** - Import produces `content_hash` from `SKILL.md`, optional `TRIGGER.md`, support file paths, and support file bytes exactly as today -> Test `test_content_hash_stable_across_metadata_only_storage`
- **Dimension 1.2** - Bundle row stores support-file manifest metadata, not support-file content -> Test `test_bundle_store_excludes_support_file_content`
- **Dimension 1.3** - Duplicate import of identical content reuses the same workspace bundle identity or fetches the existing row without mutating R2 content -> Test `test_bundle_import_dedupes_by_workspace_and_content_hash`

### Section 2 - R2 is the only support-file content store

R2 holds the canonical agentsfleet tar for support-file bundles; skill-only bundles remain valid without an object.

- **Dimension 2.1** - Import with support files writes the canonical tar to `fleet-bundles/sha256/{content_hash}.tar` before metadata commit -> Test `test_import_writes_r2_snapshot_before_metadata`
- **Dimension 2.2** - Import without support files does not require R2 and still installs a Fleet -> Test `test_skill_only_bundle_import_does_not_require_r2`
- **Dimension 2.3** - Runner bundle proxy serves only from R2 by validated `content_hash` and fails closed when R2 is unavailable or object content is missing -> Test `test_runner_bundle_proxy_uses_r2_only`

### Section 3 - Public source/provenance surface stays user-shaped

Users see GitHub source, bundle identity, requirements, and support summaries; they do not provide or manage R2 paths.

- **Dimension 3.1** - Import/detail API responses omit `snapshot_key` and expose `content_hash` plus support summaries -> Test `test_bundle_snapshot_response_hides_r2_key`
- **Dimension 3.2** - Dashboard install preview keeps existing template/GitHub flow and never renders an R2 path-like value -> Test `test_install_preview_hides_object_storage_keys`
- **Dimension 3.3** - CLI install and template flows continue to show actionable Fleet output: `fleet_id`, missing credentials, and webhook URLs -> Test `test_install_outputs_remain_fleet_facing`

### Section 4 - Runtime remains snapshot-based, not GitHub-based

The installed Fleet runs from the internal snapshot and live Fleet instructions, never from a live GitHub repository.

- **Dimension 4.1** - Lease for a bundle-backed Fleet carries `bundle.content_hash`; direct Markdown Fleets remain unchanged -> Test `test_lease_bundle_manifest_unchanged_for_bundle_fleets`
- **Dimension 4.2** - Runner cache key remains `{workspace_base}/.bundle-cache/{content_hash}.tar` and is reused across runs -> Test `test_runner_cache_uses_content_hash`
- **Dimension 4.3** - Patched Fleet `SKILL.md` / `TRIGGER.md` still override tar copies while support files come from the immutable bundle -> Test `test_live_fleet_markdown_remains_authoritative`

### Section 5 - Documentation and orphan cleanup

Architecture docs become the source of truth for "GitHub visibility, R2 runtime snapshot" and stale storage names are swept.

- **Dimension 5.1** - Architecture docs explain content hash, user-visible GitHub source, internal R2 snapshot, and no user-provided R2 path -> Test `test_architecture_docs_define_r2_canonical_store`
- **Dimension 5.2** - Removed `support_files_json` content semantics have no production references outside historical specs -> Test `test_support_files_content_storage_orphan_sweep`
- **Dimension 5.3** - Resync remains explicitly out of scope and no route, CLI verb, or UI affordance for resync is added -> Test `test_no_resync_surface_added`

## Interfaces

Public bundle snapshot responses:

- `POST /v1/workspaces/{workspace_id}/fleets/bundles/snapshots` returns `bundle_id`, `name`, `source_kind`, `source_ref`, `validation_status`, `content_hash`, `requirements`, and `support_files` summaries.
- `GET /v1/workspaces/{workspace_id}/fleets/bundles/snapshots/{bundle_id}` returns the same public metadata shape.
- Public responses do not return `snapshot_key` or any R2 bucket/key/path field.

Internal storage:

- `core.fleet_bundles` stores bundle identity, workspace, source fields, `content_hash`, internal `snapshot_key` if still needed by server code, `skill_markdown`, `trigger_markdown`, support-file manifest metadata, requirements, status, and timestamps.
- Support-file manifest metadata is path/size/hash style data only; full support-file bytes are not present in Postgres.
- R2 key derivation remains server-owned: `fleet-bundles/sha256/{content_hash}.tar`.

Runner plane:

- `GET /v1/runners/me/bundles/{content_hash}` remains the only runner bundle download route.
- The daemon rebuilds the R2 key from the validated hash and streams the canonical tar to an authenticated runner.
- Direct Markdown Fleets and skill-only bundles may have no stored tar; runner materialization treats that as "no support files" without failing the run.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| R2 unavailable during support-file import | Object store client missing or put fails | Import returns storage-unavailable error and no bundle metadata row is committed. |
| R2 object missing at run time | Metadata references a `content_hash` with no stored tar | Runner reports startup failure; event log records bundle materialization failure without leaking secrets. |
| Public response still exposes R2 key | API or generated client keeps `snapshot_key` public | Tests fail; UI must not render object storage internals. |
| Manifest parse failure | Stored support manifest is malformed | Detail endpoint returns internal error and tests prove importer cannot write malformed manifests. |
| Duplicate import race | Same workspace imports identical content concurrently | Unique `(workspace_id, content_hash)` wins; caller receives one valid `bundle_id`; R2 put remains idempotent. |
| Skill-only bundle with no R2 object | Bundle has no support files | Import and install succeed; runner sees no support files and proceeds. |
| Direct Markdown install regression | Non-bundle Fleet path accidentally expects bundle metadata | Regression tests fail; direct install must never require R2. |
| GitHub source changes after install | Source repo branch moves | Existing Fleet run remains unchanged; re-import is required for support-file changes. |

## Invariants

1. Support-file bytes are stored in R2 only - enforced by schema shape, store tests, and a production grep for removed content fields.
2. Public users never provide or receive an R2 path - enforced by OpenAPI/client tests and dashboard rendering tests.
3. `content_hash` remains content-derived, not source-derived - enforced by hashing tests that import identical bytes through different source kinds.
4. R2 writes happen before Postgres metadata commits for support-file bundles - enforced by failure-injection integration tests.
5. Runner materialization never reads support-file bytes from Postgres - enforced by runner proxy tests and orphan sweep.
6. Direct Markdown and skill-only bundle installs do not require R2 - enforced by regression tests.
7. GitHub is import-time source visibility, not runtime storage - enforced by scenario docs and lease/materialization tests.
8. Secrets never enter R2, support manifests, public responses, or logs - enforced by importer secret-shape rejection and redaction tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | not applicable | not applicable | not applicable | not applicable | not applicable |

Metrics review: no analytics or funnel event changes. This milestone changes only the storage backend for fleet bundle support-file bytes; no product event names or properties change.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs -> expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_content_hash_stable_across_metadata_only_storage` | Same bundle bytes across source kinds produce the same 64-character SHA-256 hex digest. |
| 1.2 | integration | `test_bundle_store_excludes_support_file_content` | A bundle with support file content stores path/size/hash metadata in Postgres and no support file body text. |
| 1.3 | integration | `test_bundle_import_dedupes_by_workspace_and_content_hash` | Importing identical bytes twice returns one workspace/content identity and does not create conflicting metadata. |
| 2.1 | integration | `test_import_writes_r2_snapshot_before_metadata` | Injected R2 put failure returns storage error and leaves no metadata row. |
| 2.2 | integration | `test_skill_only_bundle_import_does_not_require_r2` | With no support files and no R2 client, import and Fleet creation succeed. |
| 2.3 | unit | `test_runner_bundle_proxy_uses_r2_only` | Valid content hash fetches R2; invalid hash is rejected; missing object yields bundle-not-found error. |
| 3.1 | integration | `test_bundle_snapshot_response_hides_r2_key` | Import/detail JSON has `content_hash` and support summaries but no `snapshot_key` or R2 path field. |
| 3.2 | UI | `test_install_preview_hides_object_storage_keys` | Rendered install preview shows GitHub/source/support metadata and no object-store key text. |
| 3.3 | CLI | `test_install_outputs_remain_fleet_facing` | CLI install output still names Fleet identifier, missing credentials, and webhook URLs; no R2 path appears. |
| 4.1 | integration | `test_lease_bundle_manifest_unchanged_for_bundle_fleets` | Bundle-backed Fleet lease includes `content_hash`; direct Markdown Fleet lease has no bundle manifest. |
| 4.2 | unit | `test_runner_cache_uses_content_hash` | Runner cache path is derived from `content_hash`; repeated materialization reuses cached tar. |
| 4.3 | integration | `test_live_fleet_markdown_remains_authoritative` | Patched Fleet instructions ride the lease while support files are extracted from the immutable bundle. |
| 5.1 | unit | `test_architecture_docs_define_r2_canonical_store` | Architecture docs contain the decided GitHub-source/R2-snapshot/Postgres-metadata model. |
| 5.2 | unit | `test_support_files_content_storage_orphan_sweep` | Production grep has no full-content Postgres storage references outside historical specs. |
| 5.3 | unit | `test_no_resync_surface_added` | No new route, CLI command, or UI label for resync exists in the diff. |

Regression tests: existing dashboard template install, public GitHub import, paste install, CLI local install, and runner materialization tests remain green.

Idempotency/replay tests: duplicate imports of identical content converge on one `(workspace_id, content_hash)` row; retry after R2 failure creates no partial metadata row.

## Acceptance Criteria

- [ ] Support-file content is absent from Postgres bundle metadata - verify: `make test-unit-agentsfleetd` and production grep for removed content fields.
- [ ] R2 stores the canonical support-file tar for bundles with support files - verify: `make test-integration`.
- [ ] Public API responses hide R2 object keys and keep `content_hash` plus support summaries - verify: `make check-openapi && make test-unit-agentsfleetd`.
- [ ] Runner materialization still downloads by `content_hash` and extracts support files before execution - verify: `make test-unit-agentsfleetd && make test-integration`.
- [ ] Dashboard and CLI install surfaces show Fleet-facing output and no R2 path - verify: targeted UI/CLI tests plus grep for `snapshot_key` rendering.
- [ ] Direct Markdown and skill-only bundle installs do not require R2 - verify: integration regression tests.
- [ ] No resync surface is added - verify: grep routes, CLI commands, and UI labels for resync additions in the diff.
- [ ] Repository gates pass - verify: `make lint && make test && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect`.

## Eval Commands (post-implementation)

```bash
make test-unit-agentsfleetd
make test-integration
make check-openapi
make check-pg-drain
make lint
make test
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux
gitleaks detect
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
rg -n "resync" src agentsfleet ui public docs/architecture
```

## Dead Code Sweep

**1. Orphaned files - deleted from disk and git.**

No files are planned for deletion.

**2. Orphaned references - zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `support_files_json` as full content storage | `rg -n "support_files_json|support file content|full support-file" src schema public ui agentsfleet docs/architecture` | No production semantics saying Postgres stores support-file bodies. |
| Public `snapshot_key` response field | `rg -n "snapshot_key" public ui agentsfleet src/agentsfleetd/http/handlers/fleet_bundles` | Internal server/storage references only; no public response schema or UI rendering. |
| Resync surface | `rg -n "resync" src agentsfleet ui public docs/architecture` | Historical/design mentions only; no route, command, or UI action added by this work. |

## Discovery (consult log)

- Architecture consult, Jun 22, 2026: grounded in `docs/architecture/fleet_bundles.md` and `docs/architecture/data_flow.md`. Decision captured here: GitHub remains the user-visible source; R2 becomes the internal canonical store for installed support-file bytes; Postgres stores metadata and content identity only.
- Deferral quote for resync scope: > Indy (2026-06-22 22:30): "There two design decision currently M1 Memory and Cloudflare R2 is redundant (i am not focussed on resync)" - context: GitHub-to-bundle resync stays out of this R2 redundancy spec.

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this Test Specification, including metadata-only storage and failure injection. | Clean; final coverage note in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review against this spec, architecture docs, REST guide, Zig rules, Failure Modes, and Invariants. | Clean or every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Reviews the open Pull Request (PR) diff for response-shape drift, schema orphaning, and generated-client mismatch. | Comments addressed before human review. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-agentsfleetd` | pending | |
| Integration tests | `make test-integration` | pending | |
| OpenAPI | `make check-openapi` | pending | |
| Postgres drain | `make check-pg-drain` | pending | |
| Lint | `make lint` | pending | |
| Test suite | `make test` | pending | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | pending | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | pending | |
| Gitleaks | `gitleaks detect` | pending | |
| Orphan sweep | `rg -n "support_files_json|snapshot_key|resync" src schema public ui agentsfleet docs/architecture` | pending | |

## Out of Scope

- GitHub-to-bundle resync, auto-refresh, or a CLI resync command.
- Memory M1 and cross-fleet compounding memory.
- Action broker, no-network sandbox hardening, and GitHub review action schemas.
- Upload archive support in the dashboard picker.
- Marketplace quality ranking, install-count sorting, or run-success claims.
