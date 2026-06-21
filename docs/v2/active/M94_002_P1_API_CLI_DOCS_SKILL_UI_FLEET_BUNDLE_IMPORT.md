# Milestone 94 Workstream 002 (M94_002): Fleet Bundle import uses Fleet creation

**Prototype:** v2.0.0
**Milestone:** Milestone 94 (M94)
**Workstream:** 002
**Date:** Jun 19, 2026
**Status:** IN_PROGRESS
**Priority:** Priority 1 (P1) — fixes the first-run install path for a customer-facing dashboard flow.
**Categories:** API, Command-Line Interface (CLI), Documentation (DOCS), Skill Bundle (SKILL), User Interface (UI)
**Batch:** Batch 1 (B1) — bundle import and install flow.
**Branch:** `fix/app-ui-polish-current`
**Test Baseline:** unit=1974 integration=192
**Depends on:** None.
**Provenance:** fleet-generated (pre-spec, current codebase review)
**Canonical architecture:** `docs/architecture/user_flow.md` §8.2 and `docs/architecture/data_flow.md` install/execute paths define today's install flow. This work makes Fleet the customer runtime noun, Fleet Bundle the source package noun, and Runner the infrastructure noun. `/fleets`, `core.fleets`, and `fleet_id` are canonical.

This spec uses Pull Request (PR), Cloudflare R2 object storage (R2), Universally Unique Identifier version 7 (UUIDv7), and Hash-based Message Authentication Code (HMAC) below.

---

## Implementing fleet — read these first

1. `cli/src/lib/load-skill-from-path.ts` and `cli/src/commands/fleet_install.ts` — current local bundle loader and existing CLI install path.
2. `src/agentsfleetd/http/handlers/fleets/create.zig` — current create handler reused by direct Markdown install and bundle-backed Fleet creation.
3. `src/agentsfleetd/fleet/service.zig` and the execution-policy struct file — runner lease and execution policy shape.
4. `docs/architecture/user_flow.md`, `docs/architecture/data_flow.md`, and `docs/architecture/capabilities.md` — canonical install, trigger, runner, and capability boundaries.
5. `docs/REST_API_DESIGN_GUIDELINES.md` and `docs/SCHEMA_CONVENTIONS.md` — Representational State Transfer (REST) API and schema rules for new routes/tables.

## PR Intent & comprehension handshake

- **PR title (eventual):** Add Fleet Bundle import through Fleet creation
- **Intent:** Let a user start from a GitHub PR reviewer or Zoho Recruit outreach Fleet Bundle, satisfy required credentials, and create a Fleet through the existing install handler without adding a bundle-specific install endpoint.
- **Handshake (fleet fills during planning, before edits):** the implementing fleet restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: ...`). A mismatch between this restatement and the Intent above stops edits until reconciled.

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — In the dashboard, the user chooses a prebuilt Fleet Bundle, sees exactly which credentials are required, connects them, clicks create, and lands back on the Fleets page with the new Fleet ready to wake on its trigger or manual run.
2. **Preserved user behaviour** — Existing local CLI install from `SKILL.md` plus `TRIGGER.md`, dashboard paste install, existing credential APIs, and canonical `/fleets` routes keep working.
3. **Optimal-way check** — The direct shape is import once, validate once, snapshot once, then create the Fleet through the existing install handler. The acceptable gap is that external skill repository curation stays outside this PR while the app records two concrete scenarios.
4. **Rebuild-vs-iterate** — Fleet is the customer runtime noun, Fleet Bundle is the reusable source package, and Runner is the machine capacity. Agent-shaped names are removed from product-owned runtime/API/schema surfaces in this branch.
5. **What we build** — Bundle import/validation metadata, immutable R2 snapshot storage, optional trigger defaulting, install-time credential checks, canonical Fleet routes, runner materialization for supporting files, and a gallery-first dashboard source selector with template + GitHub import choices (Upload deferred — see Discovery 2026-06-20).
6. **What we do NOT build** — A bundle-specific install endpoint; private GitHub app setup; arbitrary dependency installs during every run; `/agents` routes or a `core.agents` view; edits to the external skills repository.
7. **Fit with existing features** — Uses the workspace credential vault and tenant model-provider setup without merging them into one confusing page flow.
8. **Surface order** — UI-first for end users, CLI preserved for developers. CLI also accepts `SKILL.md`-only folders once the server can generate a default manual/API trigger.
9. **Dashboard restraint** — Hide marketplace rankings, quality scores, and run-success claims until the product has counters that prove them.
10. **Confused-user next step** — The install preview names missing credentials and offers an inline create action or a direct link to the workspace credential page.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Rule `PSR` (standard parsers) for Markdown/frontmatter/archive parsing; `VLT` (secrets in vault) for credentials; `STS` (no static strings in SQL schema); `UFS` (shared literals as constants); `PRI` (no prompt injection from user content); `TGU` (tagged-union result shapes); `ECL` (distinct retry/fatal/input errors); `EMS` (standard error messages); `FLL` (file length); `XCC` (cross-compile); `ORP` (orphan sweep).
- **`dispatch/write_zig.md`** — applies to Zig handlers, route tables, parser helpers, runner lease policy, and materialization code.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — applies to new bundle import/list/preview routes and OpenAPI updates.
- **`docs/SCHEMA_CONVENTIONS.md`** — applies to bundle metadata tables and the `core.fleets` runtime table.
- **`dispatch/write_ts_adhere_bun.md`** — applies to dashboard install UI and CLI adjustments.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Zig gate | yes | Read `dispatch/write_zig.md`; cross-compile both Linux targets; keep functions small. |
| Public-surface gate | yes | New route/response structs get a shape verdict and tagged-union outcomes where they can fail. |
| File & Function Length | yes | Split bundle parsing, storage, install preview, and runner materialization into separate files before any file approaches the cap. |
| Unified Field Semantics | yes | Centralize bundle states, source kinds, supported file names, trigger defaults, tool names, and credential requirement keys. |
| UI Substitution / Design Token | yes | Use design-system primitives and theme tokens; no raw one-off controls for source selection or credential status. |
| Logging / Lifecycle / Error Registry / Schema | yes | Add named error entries for API failures, drain Postgres queries, and register schema files through the canonical migration array. |

## Overview

**Goal (testable):** Importing either the GitHub PR reviewer bundle or the Zoho Recruit outreach bundle produces a validated bundle snapshot that creates a Fleet and reaches the runner with declared credentials, policy, and supporting files.

**Problem:** The product currently asks users to paste `SKILL.md`/`TRIGGER.md` and calls the action "Install Fleet"; that is too low-level for a user who expects to pick or create a useful Fleet and connect the services it needs.

**Solution summary:** Add Fleet Bundle as the user-facing import/template layer. The server validates an uploaded or public GitHub bundle, stores searchable metadata in Postgres, stores the immutable archive in R2, previews required credentials/tools/network, then creates the runtime Fleet through the existing install handler. No bundle-specific install route is added.

## Prior-Art / Reference Implementations

- **CLI** — mirror `cli/src/commands/fleet_install.ts`: command reads a local bundle and calls `POST /v1/workspaces/{workspace_id}/fleets`; adjust only for optional `TRIGGER.md` and remote template selection.
- **API** — reuse `src/agentsfleetd/http/handlers/fleets/create.zig` for workspace authorization, request parsing, validation errors, direct Markdown install, and bundle-backed Fleet creation.
- **Credentials** — mirror `src/agentsfleetd/http/handlers/fleets/credentials.zig`: bundle-backed Fleet creation references credential names; secrets remain in the workspace vault.
- **Runner policy** — extend `src/agentsfleetd/fleet/service.zig`: it already returns instructions and execution policy but must also carry bundle files and fill declared tools/network.
- **Schema** — mirror `schema/021_fleet_runners.sql` through `schema/026_account_purge_gate_bypass.sql`: UUIDv7 primary keys, timestamps as `BIGINT`, no static SQL defaults/check enumerations.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M94_002_P1_API_CLI_DOCS_SKILL_UI_FLEET_BUNDLE_IMPORT.md` | CREATE | Tracks the scope and the two concrete bundle scenarios. |
| `docs/AUTH.md` | EDIT | Document bundle import auth and credential-reference boundaries. |
| `docs/architecture/README.md` | EDIT | Add Fleet Bundle to the architecture index and glossary. |
| `docs/architecture/high_level.md` | EDIT | Reconcile Markdown-defined fleets with bundle support files. |
| `docs/architecture/direction.md` | EDIT | Record Fleet Bundle as an import/template layer above the runtime fleet. |
| `docs/architecture/capabilities.md` | EDIT | Make `SKILL.md` required, `TRIGGER.md` optional for bundles, and support files non-authoritative. |
| `docs/architecture/user_flow.md` | EDIT | Document template/import/upload install flow and existing fleet install reuse. |
| `docs/architecture/data_flow.md` | EDIT | Document import snapshot, install, lease, and runner materialization flow. |
| `docs/architecture/runner_fleet.md` | EDIT | Document bundle manifest materialization and remove stale registry allow-list wording. |
| `docs/architecture/scenarios/README.md` | EDIT | Reference the two bundle fixtures as the scenario layer for this work. |
| `schema/027_core_fleet_bundles.sql` | CREATE | Store bundle metadata, source kind, visibility, content hash, parsed requirements, and validation status. |
| `schema/007_core_fleets.sql` | EDIT | Define the runtime table as `core.fleets` and add nullable bundle reference fields. |
| `schema/embed.zig` and `src/agentsfleetd/cmd/common.zig` | EDIT | Register the new schema file in the canonical migration list. |
| `src/agentsfleetd/http/routes.zig`, `src/agentsfleetd/http/route_matchers.zig`, `src/agentsfleetd/http/route_table.zig`, `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | Add Fleet Bundle import/list/preview routes and keep Fleet creation/list/detail routed through the existing handler family. |
| `src/agentsfleetd/http/handlers/fleet_bundles/*.zig` | CREATE | Validate, preview, import, and store bundle sources. |
| `src/agentsfleetd/http/handlers/fleets/create.zig` | EDIT | Accept optional `bundle_id`; generate default trigger when `TRIGGER.md` is absent; emit canonical Fleet response fields. |
| `src/agentsfleetd/fleet/service.zig` and the execution-policy struct file | EDIT | Include bundle materialization metadata and declared policy in runner leases. |
| `src/runner/**` | EDIT | Materialize bundle support files inside the sandbox workspace before execution. |
| `public/openapi.json` and `public/openapi/paths/*.yaml` | EDIT | Publish Fleet Bundle import/list/preview routes and the extended Fleet creation request. |
| `cli/src/lib/load-skill-from-path.ts` and `cli/src/commands/fleet_install.ts` | EDIT | Permit `SKILL.md`-only folders and preserve local install behavior. |
| `ui/packages/app/app/(dashboard)/fleets/new/**` and `ui/packages/app/lib/**` | EDIT | Replace paste-first install with gallery-first template/GitHub/paste flow, Fleet-facing copy, and credential preview. |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | First-run `FirstInstallCard` surfaces template cards with `Use template → /fleets/new?template=<id>`. |
| `src/agentsfleetd/fleet_bundle/github_source.zig` and `github_net.zig` | CREATE | Server-side `github.com`-only bundle fetcher: Server-Side Request Forgery (SSRF) host-allowlist guards, compressed/decompressed + file-count caps, decompression-bomb + tar traversal/symlink rejection, content hash, and canonical-tar re-pack for R2. |
| `ui/packages/app/lib/api/fleet-bundles.ts` | CREATE | Bundle snapshot/list API client — the dashboard posts `{source_kind, source_ref}` to the server-side fetcher; the app-side `lib/github` fetcher is shelved (Discovery 2026-06-20). |
| `src/agentsfleetd/**/*test.zig`, `cli/test/**`, `ui/packages/app/tests/**` | EDIT/CREATE | Cover import validation, optional trigger defaulting, runner materialization, and UI flow. |
| `samples/fixtures/fleet-bundles/**` | CREATE | Minimal in-repo fixtures for GitHub PR reviewer and Zoho Recruit outreach scenarios. |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Introduce Fleet Bundle as a source/import layer and make Fleet the customer runtime noun. Canonical surfaces use `/fleets`, `/fleets/bundles`, `core.fleets`, and `fleet_id`; `/agents` routes and `core.agents` are not carried forward.
- **Alternatives considered:** A new bundle-specific install route was rejected because it would duplicate Fleet creation and split install validation across two handlers. Using Fleet for runner infrastructure was rejected because runners are machine capacity, not the customer's created runtime.
- **Patch-vs-refactor verdict:** this is a targeted refactor around install source handling because paste-only install, remote import, credential preview, and runner file materialization share one user flow.

## Sections (implementation slices)

### §1 — Bundle source model and validation

Define the server-side Fleet Bundle snapshot model and validation boundary. Only `SKILL.md` is required; `TRIGGER.md`, `SOUL.md`, service playbooks, scripts, examples, and assets are optional support files.

- **Dimension 1.1** — Import validates public GitHub and upload sources for required `SKILL.md`, path traversal, file allow rules, size caps, frontmatter, and secret-shaped content → Test `test_fleet_bundle_import_rejects_invalid_archives`
- **Dimension 1.2** — Missing `TRIGGER.md` generates a default manual/API trigger at install time, not import time → Test `test_fleet_install_generates_default_trigger_for_skill_only_bundle`
- **Dimension 1.3** — Imported bundle snapshots are content-addressed, immutable, and stored with metadata in Postgres plus archive bytes in R2 → Test `test_fleet_bundle_import_persists_metadata_and_snapshot_ref`

### §2 — Two concrete scenario fixtures

Track the two end-user scenarios Indy will update in the external skills repository, with minimal in-repo fixtures for tests.

- **Dimension 2.1** — GitHub PR reviewer declares GitHub credential requirements, GitHub Pull Request webhook events, `api.github.com` network access, and review-comment capability → Test `test_github_pr_review_bundle_preview_lists_required_grants`
- **Dimension 2.2** — Zoho Recruit outreach declares Zoho credential requirements, optional mail credential, Zoho network access, and support files such as `ZOHO.md` → Test `test_zoho_recruit_bundle_preview_lists_required_grants`
- **Dimension 2.3** — Scenario fixtures remain fixtures; the external skills repository is not modified by this PR → Test `test_bundle_fixture_sources_are_not_runtime_defaults`

### §3 — Create Fleet through existing install handler

Extend the existing install handler so a validated bundle snapshot can feed the same creation path used by paste and CLI install while exposing Fleet-first routes and response language.

- **Dimension 3.1** — `POST /v1/workspaces/{workspace_id}/fleets` accepts direct Markdown bodies and `bundle_id` plus optional overrides through the same handler → Test `test_fleet_create_accepts_bundle_id_and_existing_markdown_body`
- **Dimension 3.2** — Install fails with a structured missing-credentials response when required workspace credentials are absent → Test `test_fleet_create_bundle_reports_missing_credentials`
- **Dimension 3.3** — A bundle install records source metadata on the created Fleet while preserving canonical `fleet_id`, event stream, and webhook response semantics → Test `test_fleet_create_bundle_preserves_fleet_response_shape`

### §4 — Runner materialization and policy enforcement

Ensure the runner receives files and policy, not just prompt text.

- **Dimension 4.1** — Lease response includes a signed bundle snapshot reference or prepared file manifest for the exact installed bundle version → Test `test_runner_lease_includes_bundle_manifest_for_bundle_agent`
- **Dimension 4.2** — Supporting files are materialized inside the sandbox workspace before execution and are not pasted into the model prompt wholesale → Test `test_runner_materializes_bundle_files_inside_workspace`
- **Dimension 4.3** — Declared tools, network policy, and credential map are populated from install metadata into `ExecutionPolicy` → Test `test_execution_policy_includes_bundle_tools_network_and_credentials`

### §5 — Dashboard and CLI experience

Make the install path obvious for end users while preserving developer workflows.

- **Dimension 5.1** — Dashboard install is gallery-first: prebuilt templates lead the page, with Import from GitHub and a Paste fallback in a secondary "or start from source" strip (Upload deferred per Discovery 2026-06-20) → Test `test_install_fleet_source_selector_renders_primary_choices`
- **Dimension 5.2** — Credential preview distinguishes workspace credentials from tenant model provider setup and links to the correct create flow → Test `test_install_preview_routes_missing_credentials_to_workspace_credentials`
- **Dimension 5.3** — CLI local install accepts `SKILL.md`-only bundles and prints a clear generated-trigger note → Test `test_cli_install_skill_only_bundle_uses_default_trigger`

## Interfaces

Preferred Fleet creation routes reuse the existing install handler:

- `POST /v1/workspaces/{workspace_id}/fleets` is the Fleet-first creation route.
- Extended request accepts either direct Markdown bodies or `bundle_id`.
- `trigger_markdown` becomes optional when `bundle_id` is present or when the client submits a `SKILL.md`-only local bundle.
- Response is Fleet-native: created `fleet_id`, `name`, `status`, and `webhook_urls`.

New bundle import/list/preview surfaces:

- `GET /v1/fleets/bundles` lists first-party template metadata and scenario identifiers.
- `POST /v1/workspaces/{workspace_id}/fleets/bundles/snapshots` imports a public GitHub repository/path or uploaded archive reference and returns validation status plus `bundle_id`.
- `GET /v1/workspaces/{workspace_id}/fleets/bundles/snapshots/{bundle_id}` returns parsed metadata, required credentials, required tools, network hosts, source kind, and immutable content hash.

Runner lease extension:

- `POST /v1/runners/me/leases` exists today.
- Lease response adds bundle manifest/download metadata only for Fleets installed from a bundle.
- `ExecutionPolicy` carries declared tools, network policy, credential map, provider, and context.

Credential and model surfaces:

- Workspace credentials continue through `GET/POST /v1/workspaces/{workspace_id}/credentials`.
- Tenant model provider setup continues through `GET/PUT/DELETE /v1/tenants/me/provider`.
- Bundle install preview must not imply model-provider credentials and workspace service credentials are the same thing.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing skill | Bundle lacks `SKILL.md` | Import returns 400 with `missing_skill` and no snapshot is stored. |
| Unsafe archive path | Archive contains absolute path or parent-directory traversal | Import returns 400 with `unsafe_path`; the server stores nothing. |
| Oversized bundle | Archive or parsed Markdown exceeds configured caps | Import returns 413 with the offending cap name. |
| Malformed frontmatter | Required metadata cannot be parsed with the standard parser | Import returns 400 with field-level errors. |
| Missing trigger | `TRIGGER.md` is absent | Import succeeds; install generates the default manual/API trigger. |
| Missing credential | Bundle requires `github`, `zoho`, or mail credential not present in workspace vault | Install preview and create return missing credential keys and do not create the Fleet. |
| GitHub fetch failure | Public URL is unavailable or not a supported GitHub source | Import returns retryable/fatal class depending on upstream result. |
| R2 unavailable | Snapshot storage fails after validation | Import returns 503 and no Postgres metadata is committed. |
| Runner bundle download failure | Lease references a snapshot that cannot be fetched/materialized | Runner reports a startup failure event; no user credential is logged. |
| Prompt injection in support file | Support file tries to override capability grants through prose | Import may store the file, but execution ignores undeclared tools, network, and secrets. |

## Invariants

1. `SKILL.md` is the only required bundle file — enforced by import validation and tests.
2. `TRIGGER.md` is optional — enforced by server-side default trigger generation and CLI tests.
3. No bundle-specific install endpoint exists — enforced by route table tests and OpenAPI diff review.
4. Bundle archives are immutable after import — enforced by content hash, R2 object key, and metadata status transitions.
5. Resolved secret values never enter bundle metadata, R2 archives, logs, or runner manifest — enforced by vault-only references and redaction tests.
6. Prose files cannot grant capabilities — enforced by `ExecutionPolicy` assembly from parsed metadata and workspace grants.
7. Installed Fleets do not depend on a live GitHub branch after import — enforced by stored content hash and scenario tests.
8. Canonical `/fleets` paths and `fleet_id` response fields remain stable — enforced by regression tests.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fleet_bundle_import_rejects_invalid_archives` | Archive missing `SKILL.md`, unsafe paths, and over-cap files return specific 400/413 errors with no metadata row. |
| 1.2 | integration | `test_fleet_install_generates_default_trigger_for_skill_only_bundle` | `SKILL.md`-only bundle creates a Fleet with manual/API trigger and empty tool list. |
| 1.3 | integration | `test_fleet_bundle_import_persists_metadata_and_snapshot_ref` | Valid upload stores Postgres metadata and an immutable R2 key derived from content hash. |
| 2.1 | unit | `test_github_pr_review_bundle_preview_lists_required_grants` | Fixture preview lists GitHub credential, PR webhook events, `api.github.com`, and review-comment action. |
| 2.2 | unit | `test_zoho_recruit_bundle_preview_lists_required_grants` | Fixture preview lists Zoho credential, optional mail credential, Zoho hosts, and support files. |
| 2.3 | unit | `test_bundle_fixture_sources_are_not_runtime_defaults` | Fixture paths are used only by tests and are not hard-coded into production template defaults. |
| 3.1 | integration | `test_fleet_create_accepts_bundle_id_and_existing_markdown_body` | Existing direct Markdown body and new `bundle_id` body both create Fleets through one handler. |
| 3.2 | integration | `test_fleet_create_bundle_reports_missing_credentials` | Missing `github`/`zoho` credentials return structured missing keys and no Fleet row. |
| 3.3 | integration | `test_fleet_create_bundle_preserves_fleet_response_shape` | Response includes `fleet_id`, `name`, `status`, and `webhook_urls` with existing semantics. |
| 4.1 | integration | `test_runner_lease_includes_bundle_manifest_for_bundle_agent` | Lease for a bundle-backed Fleet includes snapshot metadata; non-bundle Fleet leases are unchanged. |
| 4.2 | integration | `test_runner_materializes_bundle_files_inside_workspace` | Support files exist inside sandbox workspace before execution and are not injected into instructions. |
| 4.3 | unit | `test_execution_policy_includes_bundle_tools_network_and_credentials` | Execution policy includes declared tools, network hosts, and credential refs from install metadata. |
| 5.1 | UI | `test_install_fleet_source_selector_renders_primary_choices` | Dashboard starts with template, upload, and GitHub import options before manual paste fallback. |
| 5.2 | UI | `test_install_preview_routes_missing_credentials_to_workspace_credentials` | Missing service credential action routes to workspace credentials, not tenant model provider setup. |
| 5.3 | CLI | `test_cli_install_skill_only_bundle_uses_default_trigger` | CLI local folder with only `SKILL.md` succeeds and prints generated-trigger note. |

Regression tests: existing CLI install from a folder with both `SKILL.md` and `TRIGGER.md`, existing dashboard paste install, existing credential create/read/update/delete behaviour, and existing runner lease for non-bundle Fleets remain green.

Idempotency/replay tests: importing the same content twice returns the same content hash and either reuses or version-links metadata without mutating the immutable snapshot; retry after R2 failure does not create a partial bundle.

## Acceptance Criteria

- [ ] Canonical Fleet creation route uses the existing install handler; no bundle install endpoint is added — verify: `rg -n "fleets/bundles/.*/install|fleet_bundle.*install" src public docs`
- [ ] `POST /v1/workspaces/{workspace_id}/fleets` handles existing Markdown and new `bundle_id` request bodies — verify: `make test-unit-agentsfleetd`
- [ ] GitHub PR reviewer and Zoho Recruit outreach fixtures validate and preview required grants — verify: `make test-unit-agentsfleetd`
- [ ] Runner lease materializes support files and fills declared execution policy — verify: `make test-integration`
- [ ] Dashboard install flow exposes template/upload/GitHub import and correct credential next steps — verify: `bunx vitest ui/packages/app/tests/fleets-install-form.test.ts`
- [ ] CLI local install accepts `SKILL.md`-only folders — verify: `bun test cli/test`
- [ ] Schema/OpenAPI stay clean — verify: `make check-openapi && make check-pg-drain`
- [ ] Zig and repository gates pass — verify: `make lint && make test && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect`

## Eval Commands (post-implementation)

```bash
make test-unit-agentsfleetd
make test-integration
bunx vitest ui/packages/app/tests/fleets-install-form.test.ts
bun test cli/test
make check-openapi
make check-pg-drain
make lint
make test
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux
gitleaks detect
```

## Dead Code Sweep

No files are deleted by this spec. If implementation replaces paste-first install components, grep old component names and remove stale tests/imports in the same commit.

## Discovery (consult log)

- Jun 19, 2026 — Codebase review found no existing Fleet Bundle route or bundle-specific install endpoint. Existing install is `POST /v1/workspaces/{workspace_id}/fleets`.
- Jun 19, 2026 — Current CLI loader requires both `SKILL.md` and `TRIGGER.md`; dashboard currently generates a default manual trigger client-side.
- Jun 19, 2026 — Runner lease shape already has execution policy fields, but current service must be extended so declared tools/network and support files reach the runner.
- Jun 20, 2026 — Codex second-opinion review (high effort) confirmed: snapshot endpoint trusts client-supplied content (no server-side GitHub fetch today); app-side fetch is the right first increment (moving server-side later relocates only fetch/validate/assemble). [P1]s folded in: pin template GitHub sources to a commit SHA (not branch) so "prebuilt" cannot silently change between users; restrict fetch to `github.com` URLs (no arbitrary hosts/redirects, size+count caps before buffering, error taxonomy owned app-side to match the eventual API); app must handle the `missing_credentials` problem+json explicitly. Codex also flagged `create_fleet_bundle.zig:48` allows `bundle_id` + markdown overrides (not a pure immutable create) — our UI sends `{bundle_id}` only; tightening the API is a separate backend call for Indy. Snapshot import already dedupes by `(workspace_id, content_hash)` (`store.zig:68`).
- Jun 20, 2026 — Design: `/design-shotgun` image path unavailable (no `OPENAI_API_KEY` for the gstack designer); explored via hand-built HTML wireframes in the real design-system tokens instead (`~/.gstack/projects/agentsfleet-usezombie/designs/fleet-new-20260620/`). Layout direction chosen = Gallery-first.
- Jun 20, 2026 — Indy acks (verbatim): "we can descope Upload for now. But focus on prebuilt and git urls if that is easier?" — context: defer §5 Upload-bundle source; ship template + GitHub + paste. "Must be simple for users to start so D" — context: gallery-first layout chosen for `/fleets/new`. "We will have the template in github.com/agentsfleet/skills" — context: template bundles resolve from `agentsfleet/skills` at path=`<template-id>` pinned to a commit SHA (HEAD 775d748d at decision time). First-run Dashboard card surfaces the templates with `Use template → /fleets/new?template=<id>`.
- Jun 20, 2026 — Architecture re-cut to server-side import (Indy: "I feel we do 1 first and then with a github template"). Fetch = GitHub tarball API `GET /repos/{o}/{r}/tarball/{ref}` via `std.http.Client` + pure-Zig `std.compress.flate` + `std.tar`, filtered to the template subpath (gets subfolders / py / node / nested md in one request). **libgit2 clone rejected** (allyourcodebase/libgit2 0.16 exists, but would be agentsfleetd's first C dependency + heavy TLS/ssh2/zlib toolchain — overkill for a public snapshot-at-a-SHA). Storage = **Cloudflare R2 wired now** via the `z3` S3 client (`codeberg.org/fellowtraveler/z3`, MIT, Zig 0.16.0, zero-dep, R2-native `region=auto`+`endpoint`), wrapped behind `src/lib/s3/r2.zig` shared by the agentsfleetd (put) and runner (get) build graphs. The app-side `lib/github/fetch-bundle.ts` is shelved (server fetches now); the import request becomes `{source_kind, source_ref}`. Indy acks (verbatim): "GitHub tarball + std.tar/flate", "Wire R2 now (z3)".
- Jun 20, 2026 — **Credential gate (R2):** downstream creds required before end-to-end / deploy — `R2_ACCOUNT_ID` (→ `https://<account>.r2.cloudflarestorage.com`), `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `R2_BUCKET`; region fixed `auto`. Stored in vault, fed as env at deploy. Limits: `MAX_SOURCE_LEN`/`MAX_TRIGGER_LEN` → 200KB; support caps unchanged (32 / 64KB / 256KB) pending real template shapes.
- Jun 20, 2026 — `/plan-eng-review` decided template **repo layout + fetch** (Indy chose, overriding the monorepo recommendation): **one git repo per template** (`agentsfleet/skill-<id>`) instead of a single `agentsfleet/skills` monorepo with subfolders. Consequence: the repo's whole-repo tarball **is** the bundle — `source_path` is empty (root), no subfolder extraction. **Fetch = whole-repo tarball** (`GET /repos/agentsfleet/skill-<id>/tarball/<ref>` → `std.compress.flate` + `std.tar`, strip the single wrapper dir), behind a swappable fetcher seam; **libgit2 clone rejected** (would be the first C dep; wrong tool for a public cold-path snapshot). Key reframe: the GitHub fetch is **import-time only and R2-cached by content hash**, so whole-repo download is a rare one-time cost, not per-run. Accepted tradeoff (on record): N-repo sprawl as the catalog grows (fine at 3, revisit ~20). This **supersedes** the earlier `agentsfleet/skills` monorepo note above. The 028 primer + `template_catalog` now point at `agentsfleet/skill-<id>` repos; `source_ref` is `main` until each repo exists, then pinned to a commit SHA per repo (codex P1).
- Jun 20, 2026 — **FINAL launch decision (Indy): "I will stick with single repo for this launch."** Reverts to a **single monorepo `agentsfleet/skills`** with one subfolder per template — supersedes the repo-per-template note immediately above. The importer fetches the whole-repo tarball at `source_ref` and filters to `source_path` (the template subfolder) via `std.tar` (~20 lines). One pinned SHA covers all templates. Primer reset to `source_repo='agentsfleet/skills'`, `source_path=<id>`, `source_ref='main'` (placeholder; pin to the commit SHA once all three folders — github-pr-reviewer, zoho-sprint-daily-summarizer, security-reviewer — are committed; today only github-pr-reviewer exists as an 8-byte stub). In-repo fixtures live at `samples/fixtures/fleet-bundles/<id>/` and mirror the folder each template needs in the skills repo.
- Jun 20, 2026 — **TRULY FINAL (Indy clarified "single" = one repo *per fleet*, not a monorepo):** repo-per-template, repo named **`agentsfleet/<id>`** (repo name == template id, no `skill-` prefix; e.g. `agentsfleet/github-pr-reviewer`, public, HEAD `139c5a87` today but placeholder content). **Repo ROOT is the bundle** — `source_path` empty; the importer strips the single tarball wrapper dir, no subpath filter. Indy acks: "i will create single or individual repo per fleet", "just assume it will have the SKILL.md … i will fix it up later on the 3 repo", "use main?". So `source_ref='main'` for all three (pin to commit SHA in a follow-up once repos are finalized — codex P1). The three repos: `agentsfleet/github-pr-reviewer` (exists, to fix up), `agentsfleet/zoho-sprint-daily-summarizer` + `agentsfleet/security-reviewer` (Indy to create). This supersedes the monorepo note above. The `samples/fixtures/fleet-bundles/<id>/` content = what goes at each repo's ROOT. R2 vault item `cloudflare-r2` created by Indy.
- Jun 21, 2026 — **Server-side import handler landed (Option A).** `http/handlers/fleet_bundles/imports.zig` rewritten + new `resolve.zig`: the wire request is `{source_kind, source_ref, skill_markdown?, trigger_markdown?}`. `upload` (paste) carries SKILL + TRIGGER inline and rejects support files with 400 (attachments ride only fetched sources — Indy-confirmed Option A); `github`/`template` fetch server-side via `github_source.fetch` (SSRF + extraction guards), re-tar canonically, and store in R2 **only when the bundle has ≥1 support file** (so paste and fetched-no-support both import with R2 unset, `ctx.r2 = null`). Persist order is R2-put-then-Postgres so a storage failure leaves no metadata row. Import concurrency cap **K=12** (module-level atomic; 429 on overflow) bounds the fetch whose body read has no std deadline (`github_source` red-team finding #4). New error codes: UZ-BUNDLE-004 (502 fetch-failed), UZ-BUNDLE-005 (503 storage-unavailable), UZ-BUNDLE-006 (429 too-many-imports). Verified locally: `zig build`, both Linux cross-compiles, `make lint-zig`, `make harness-verify`, `make check-openapi`, `gitleaks` all green; sequential unit run 1288 passed / 0 failed (6 new `resolve` unit tests). Integration tests (`api_integration_test.zig`: paste persists + detail preview, upload-rejects-support → 400, `bundle_id` install, missing-creds → 424) are database-gated and not runnable locally (no `TEST_DATABASE_URL` / local Postgres) — they run in Continuous Integration (CI). Known gap: the github/template fetch → R2 happy path is network + R2-credential bound; the 503 storage path cannot be triggered via paste (paste rejects attachments before the R2 check), so it stays integration-only.

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this Test Specification. | Clean; iteration count recorded in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review against this spec, architecture docs, REST guide, Zig rules, Failure Modes, and Invariants. | Clean or every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Reviews the open PR against the immutable diff. | Comments addressed before human review. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-agentsfleetd` | Pending implementation. | |
| Integration tests | `make test-integration` | Pending implementation. | |
| UI tests | `bunx vitest ui/packages/app/tests/fleets-install-form.test.ts` | Pending implementation. | |
| CLI tests | `bun test cli/test` | Pending implementation. | |
| OpenAPI | `make check-openapi` | Pending implementation. | |
| Lint | `make lint` | Pending implementation. | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | Pending implementation. | |
| Gitleaks | `gitleaks detect` | Pending implementation. | |

## Out of Scope

- External skills repository content changes; this PR only tracks the two expected scenarios and test fixtures.
- Private GitHub authentication/import. Public GitHub source import is enough for this work.
- Runtime package installs such as `npm install` during every Fleet run.
- Separate bundle install endpoint or `/agents` route.
- Marketplace ranking, billing packaging, and public template publishing workflow.
- Upload-bundle source in the dashboard (deferred 2026-06-20, Indy-acked — see Discovery). Ship template + GitHub + paste first.
- App-side GitHub fetching in the dashboard. The 2026-06-20 architecture re-cut (see Discovery) moved the fetch server-side into `agentsfleetd` (`github_source.zig` / `github_net.zig`); the dashboard now posts `{source_kind, source_ref}` and the shelved app-side `lib/github` fetcher is not built.
- Populating the `agentsfleet/skills` repo with the `github-pr-reviewer` / `zoho-recruit-outreach` template folders (external skills-repo content; per §2 this PR only references them — a clicked template renders the import-error state until those folders exist).
