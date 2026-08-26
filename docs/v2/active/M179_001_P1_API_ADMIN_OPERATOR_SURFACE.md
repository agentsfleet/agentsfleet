<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M179_001: Admin and operator surface — platform planes serve from Rust

**Prototype:** v2.0.0
**Milestone:** M179
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing parity; the Zig daemon keeps serving production while this lands
**Categories:** API
**Batch:** B4 — runs concurrently with M178 after M177 (disjoint route groups; the shared files — `afd_api`, `afd_fleet`, `afd_state`, `rustd/Cargo.toml`, `make/test-integration.mk` — are append-only seams coordinated at merge)
**Branch:** feat/m179-admin-operator-surface
**Test Baseline:** `unit=5631 integration=1019` — reconstructed at CHORE(open) from the green GitHub Actions jobs on base `414805429`: Rust 914 + CLI 1624 + website 175 + app 2406 + design-system 512 unit tests; the live coverage lane reported 1019 tests. Local execution follows the cadence recorded in Discovery.
**Depends on:** M177_001 (runner rows, bundle serving, fleet services); M176_001 (auth, stores, shell, the `afd_state` crate this milestone extends)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Runner state + §Registering a runner; `docs/architecture/fleet_bundles.md`

---

## Overview

**Goal (testable):** the admin plane (`/v1/admin/fleet-libraries*`, `/v1/admin/platform-keys*`, `/v1/admin/models*`) and the operator plane (`/v1/fleets/bundles`, `/v1/fleets/runners*`, `/v1/fleets/streams`) serve from `agentsfleetd-rs` with scope-gate, response-shape, and bundle-import behaviour equal to the Zig daemon, graded by the route-inventory test and the integration subset (full-route OpenAPI coverage is M181's oracle).
**Problem:** the platform planes carry the highest-privilege scopes (platform keys, runner administration, library curation) and the bundle-import trust boundary; they must port with their gates intact, and they are the natural concurrent partner to M178 because the file sets and route groups are disjoint.
**Solution summary:** port the admin and operator handler groups onto the M176 shell, the fleet-library importer with its GitHub source, bundle import/validation with Cloudflare R2 storage (upload side of the M177 serving path), platform-key vault handling, and the runner administration views over `fleet.runners` + `fleet.runner_events`.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): admin + operator planes with import parity
- **Intent (one sentence):** platform operators can curate libraries, manage platform keys and models, import bundles, and administer runners against `agentsfleetd-rs` exactly as against the Zig daemon.
- **Handshake restatement:** `agentsfleetd-rs` will expose the existing privileged admin and operator behaviours with the same authorization decisions and wire shapes, while the implementation uses Rust-native ownership, typed errors, standard-library or crate primitives, and narrow traits at vendor/storage seams.
- **ASSUMPTIONS I'M MAKING:** parity grades observable behaviour rather than internal Zig structure; M179 owns a separate branch and Pull Request from M178; focused compile/test commands may run while completing a Dimension; repository-wide unit verification waits until all Sections are implemented; the live datastore lane runs at `orly gate pr`; OpenAPI and published docs stay unchanged unless source comparison proves existing documentation wrong.
- **PLAN impact:** create `afd_library`; extend `afd_api`, `afd_state`, and `afd_fleet`; add the Rust admin/operator integration subset; keep changes inside the Files Changed table.
- **PLAN verification:** unit tests accompany every Dimension; Rust format and Clippy run once per completed Section; `make test-unit-all` runs after all Sections; `make test-integration-rustd` runs through `orly gate pr`.
- **Quality ceiling:** thin HTTP adapters over typed services, transactional repository methods, and traits only where implementations vary are leaner and safer under concurrency than transliterating Zig control flow or adding a general handler framework.
- **Surface-area checklist:** OpenAPI paths yes, behaviour-preserving only; CLI no; user docs no unless parity inspection finds drift; release/version yes at CHORE(close); schema no; spec-vs-rules conflict no.

## Implementing agent — read these first

1. `src/agentsfleetd/http/route_template.zig` + `http/route_scopes.zig` — the admin/operator route × scope inventory (platform scopes stay off the public docs page on purpose).
2. `docs/AUTH.md` §Fleet Bundle import and credential boundary — bundle content is untrusted until validated; import may store metadata and required-credential *names*, never resolve or store credential values; preview never reads the workspace vault.
3. `src/agentsfleetd/fleet_library/importer.zig` + `fleet_library/github_source.zig` — import pipeline being ported.
4. `docs/architecture/runner_fleet.md` §Runner state — `admin_state` enum + derived liveness + append-only `fleet.runner_events`; cordon/drain/revoke deliver through the auth read (M177).
5. `src/agentsfleetd/state/` — platform-key and model-library repositories whose SQL ports verbatim.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_api/**` | EDIT | Route variants + handler modules: admin fleet-libraries, platform-keys, admin models; operator bundles, runners, streams |
| `rustd/crates/afd_library/**` | CREATE | fleet-library catalogue, importer, GitHub source, bundle validation + R2 upload |
| `rustd/crates/afd_state/**` | EDIT | platform-key + model-library repositories (admin write paths) |
| `rustd/crates/afd_fleet/**` | EDIT | runner administration service (cordon/drain/revoke/rotate), streams overview reads |
| `rustd/crates/afd_core/**` | EDIT | Import the existing Zig registry entries newly reachable from Rust |
| `rustd/crates/afd_wire/**` | EDIT | Admin/operator request and response types consumed by both service and HTTP layers |
| `rustd/crates/agentsfleetd/**` | EDIT | Compose the new stores and import credentials at daemon startup |
| `rustd/Cargo.toml` | EDIT | new member |
| `make/test-integration-rustd.mk` | EDIT | admin/operator integration subset against the Rust binary |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (bundle content is untrusted input — validation before any storage decision), NSQ, CTM (platform-key handling), KYS (runner/event list pagination), ECL, UFS, NDC, TST-NAM, MSID, ERR, FLL.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — REST rules for every handler.
- `docs/AUTH.md` — auth-flow rule: platform-scope gates and key-reveal semantics.
- `dispatch/write_rust.md` — REVIEW cites Microsoft guideline mnemonics.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | importer split by pipeline stage; one module per route group |
| LOGGING | yes | admin actions log scoped events with actor + resource ids; key material never logged |
| MILESTONE-ID | yes | none in source/tests |
| UFS | yes | scope names, R2 bucket/key patterns as named constants |
| SCHEMA GUARD | no | no schema change |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/**` admin/operator groups + `src/agentsfleetd/fleet_library/` (Zig daemon) — behaviour source of truth.
- **Reference:** `~/Projects/oss/core_api-develop` — crate-per-integration pattern: the GitHub source isolated behind a trait in `afd_library` mirrors its vendor-crate discipline (adopt the topology, not the blocking client).
- **Reference:** M177's R2 read path — the upload side reuses the same store client; one owner for the bucket layout.

## Sections (implementation slices)

### §1 — Admin plane: libraries, platform keys, models

`/v1/admin/fleet-libraries[/{id}]`, `/v1/admin/platform-keys[/{provider}]`, `/v1/admin/models[/{id}]` — platform-scope-gated CRUD. Platform keys live in the vault (caller-owned key names, M176 crypto); reveal semantics match the Zig daemon (metadata on list, never plaintext).

- **Dimension 1.1** — every admin route refuses tenant-scoped and `agt_t` principals with the documented code; platform scopes pass → Test `test_admin_scope_gates` — DONE
- **Dimension 1.2** — platform-key store/rotate: vault-backed, list shows metadata only → Test `test_platform_key_vault_semantics` — DONE
- **Dimension 1.3** — admin model + library CRUD shape parity on seeded data → Test `test_admin_crud_shape_parity`
- **Dimension 1.4** — every route + method in this spec's Interfaces inventory exists in the Route enum; extras and gaps both fail → Test `test_route_inventory_matches_interfaces` — DONE

### §2 — Bundle import and validation

`/v1/fleets/bundles`: upload/import with the trust boundary from `docs/AUTH.md` — parse and validate untrusted bundle content, store parsed metadata, required credential *names*, required tools, network hosts, and the immutable source snapshot in R2; never resolve or store credential values; preview reads no vault. Content-hash addressing feeds the M177 serving path.

- **Dimension 2.1** — valid bundle → metadata + snapshot stored; content hash serves via the M177 route → Test `test_bundle_import_roundtrip` — DONE
- **Dimension 2.2** — malicious/malformed bundles (oversize, bad manifest, path traversal in entries, credential values embedded) → rejected with documented codes; nothing stored → Test `test_bundle_import_rejects_hostile` — DONE
- **Dimension 2.3** — preview performs zero vault reads (instrumented) → Test `test_bundle_preview_no_vault` — DONE

### §3 — Fleet-library importer and GitHub source

The importer pipeline (platform gallery + per-tenant onboarding) with the GitHub source behind a trait; rate-limit and failure classes preserved (ECL).

- **Dimension 3.1** — import from a fixture GitHub source → catalogue rows parity vs the Zig importer → Test `test_library_import_parity` — DONE
- **Dimension 3.2** — source failures (404, rate-limited, truncated download) → typed errors, no partial catalogue writes → Test `test_library_import_failure_classes` — DONE

### §4 — Operator plane: runners and streams

`/v1/fleets/runners[/{runner_id}[/events|/leases]]` (list, detail, `PATCH` admin-state transitions), `/v1/fleets/streams` overview. Cordon/drain/revoke/rotate write `fleet.runners` + append `fleet.runner_events`; delivery to the runner is the M177 auth read — this milestone only writes state. Three-category status (admin_state + derived liveness + events) rendered as the Zig daemon does.

- **Dimension 4.1** — admin-state transitions write the row + append the event; illegal transitions refused → Test `test_runner_admin_transitions` — DONE
- **Dimension 4.2** — rotation swaps `token_hash`; old token 401s on next use (M177 read), new token works → Test `test_runner_rotation_takeover`
- **Dimension 4.3** — runner list/detail/events with keyset pagination + derived-status parity → Test `test_runner_views_parity`
- **Dimension 4.4** — streams overview shape parity on seeded fleets → Test `test_streams_overview_parity`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 | §2 bundle import | Claude Code · Opus 5 · xhigh | untrusted-content trust boundary; hostile-input surface |
| B1 | §1 admin plane | Claude Code · Opus 5 · high | scope gates + vault reuse, crisp oracles |
| B2 | §3 importer/GitHub | Codex · GPT 5.6 tera · high | isolated vendor integration with fixture oracle |
| B2 | §4 operator plane | Claude Code · Opus 5 · high | state-machine writes over M177-owned reads |

Runs concurrently with M178 (disjoint files/routes). Indy decides how many agents actually spin.

## Interfaces

```
Routes (per src/agentsfleetd/http/route_template.zig):
  /v1/admin/fleet-libraries[/{id}] · /v1/admin/platform-keys[/{provider}]
  /v1/admin/models[/{id}]
  /v1/fleets/bundles · /v1/fleets/runners[/{runner_id}[/events|/leases]]
  /v1/fleets/streams
Scope gates: platform scopes per http/route_scopes.zig (incl. runner:read /
runner:write / runner:enroll split per docs/AUTH.md). R2 bucket layout:
unchanged from the Zig daemon (content-hash keys shared with M177 serving).
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Hostile bundle | crafted upload | validation rejects with documented code; zero writes to R2/Postgres |
| R2 outage on import | store down | typed 5xx retryable; no partial snapshot; import re-runnable |
| GitHub source failure | 404/rate-limit/truncation | typed per-class errors; catalogue untouched |
| Illegal runner transition | bad admin request | refused with documented code; no event appended |
| Rotation race | request in flight during rotate | old token 401s from the next auth read; in-flight verb completes per Zig semantics |
| Platform-key reveal attempt | list/read after store | metadata only — plaintext unreachable via any admin route |
| Foreign principal | tenant token on platform route | 403 UZ-AUTH-022 naming the missing scope |

## Invariants

1. Bundle import never resolves or stores credential *values*; only names — enforced by `afd_library` taking no vault-read dependency + `test_bundle_preview_no_vault`.
2. Platform-key plaintext never leaves the vault via admin routes — reveal-free response types; `test_platform_key_vault_semantics`.
3. `fleet.runner_events` is append-only — no update/delete statements exist in the repository module; `test_runner_admin_transitions`.
4. Admin-state delivery stays the M177 auth read — this milestone adds no push channel; enforced mechanically by rubric R5 (diff must stay inside Files Changed, which contains no delivery-channel surface).
5. R2 keys are content hashes shared with the M177 serving path — single bucket-layout constant; `test_bundle_import_roundtrip`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet.runner_events` rows (existing) | ops | admin-state transitions | state, actor id, runner id | no token material | `test_runner_admin_transitions` |
| existing import/admin log events (unchanged set) | ops | import + admin writes | resource ids, outcome | no bundle content, no keys | `test_library_import_parity` |

No product-analytics changes (operator surface; parity port).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration (negative) | `test_admin_scope_gates` | tenant JWT / `agt_t` / missing scope → 403 UZ-AUTH-022; platform scope → 200 |
| 1.2 | integration | `test_platform_key_vault_semantics` | store→rotate→list: vault rows correct, responses metadata-only |
| 1.3 | e2e | `test_admin_crud_shape_parity` | seeded data → field-level parity vs Zig daemon |
| 1.4 | unit | `test_route_inventory_matches_interfaces` | Interfaces inventory ⊆ Route enum with methods; extras/gaps named |
| 2.1 | integration | `test_bundle_import_roundtrip` | import → metadata rows + R2 object; M177 route serves identical bytes |
| 2.2 | integration (negative) | `test_bundle_import_rejects_hostile` | oversize / bad manifest / traversal / embedded secret → documented code each, zero writes |
| 2.3 | integration | `test_bundle_preview_no_vault` | preview of a credential-requiring bundle → 0 vault reads recorded |
| 3.1 | integration | `test_library_import_parity` | fixture source → catalogue rows equal Zig importer output |
| 3.2 | integration (negative) | `test_library_import_failure_classes` | 404 / rate-limit / truncation → typed error each, no partial rows |
| 2.1 (FM) | integration (negative) | `test_bundle_import_r2_outage` | object store down mid-import → typed retryable 5xx; no partial snapshot; re-run succeeds |
| 4.1 | integration (negative) | `test_runner_admin_transitions` | legal transitions write row+event; illegal → refused, no event |
| 4.2 | integration | `test_runner_rotation_takeover` | rotate → old token 401 next call, new token 200 |
| 4.3 | integration | `test_runner_views_parity` | list/detail/events pagination + derived status parity |
| 4.4 | integration | `test_streams_overview_parity` | seeded fleets → overview shape parity |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Route inventory parity for the admin/operator groups (§1) | `cd rustd && cargo test test_route_inventory_matches_interfaces` | exit 0 | P0 | |
| R2 | Integration subset green on the Rust daemon | `make test-integration` (admin/operator lane) | exit 0 | P0 | |
| R3 | Trust boundary holds (§2) | `cd rustd && cargo test bundle` | exit 0 | P0 | |
| R4 | Runner administration parity (§4) | `cd rustd && cargo test runner_admin` + `cargo test test_runner_rotation_takeover` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

**Credential gate (in scope):** this milestone adds the Cloudflare R2 **write** keys and the GitHub token for library import to the family enumeration — same fetch location (`~/.config/agentsfleet/` via `provision-env-1password`); the boot preflight extends to name them.

## Out of Scope

- Tenant/workspace surface — M178 (concurrent partner).
- Signed ingress, connectors, cron — M180.
- Scheduler/trust-class placement design (`trust_class`, `allowed_workspace_ids`) — deferred exactly as in the Zig daemon; parity port only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a platform operator curates a library entry, imports a bundle, cordons a runner, and rotates its token against staging `agentsfleetd-rs` — every gate, view, and event row identical to production behaviour.
2. **Preserved user behaviour** — the operator dashboard and CLI flows change nothing; platform scopes gate exactly as documented.
3. **Optimal-way check** — concurrent with M178 under the same two oracles; the only faster path would skip hostile-input tests on import, which is the one place not to economize.
4. **Rebuild-vs-iterate** — pure port; importer redesign is post-cutover material. "Pure port" bounds the redesign, not the parity rule: a superseded or compatibility path that meets M181's single-implementation evidence bar (no in-tree emitter plus Indy's sign-off, recorded in Discovery) is left unported and registered as a declared divergence, not reproduced.
5. **What we build** — one library crate, admin/operator handler groups, runner-administration service, import trust boundary.
6. **What we do NOT build** — trust-class placement, new admin surfaces, R2 layout changes, push-based runner state delivery.
7. **Fit with existing features** — compounds with M177 (serving path, auth read); must not destabilize the shared R2 bucket layout.
8. **Surface order** — N/A — existing operator surfaces only; no new surface designed.
9. **Dashboard restraint** — no UI change; no new controls.
10. **Confused-user next step** — refusals name the missing scope (UZ-AUTH-022) or the documented import code; existing operator docs remain accurate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices with the trust boundary (§2) isolated on the strongest tier and vendor I/O (§3) isolated behind a trait — privilege and hostile input, not route count, drive the split.
- **Alternatives considered:** folding this milestone into M178 (rejected: it would breach the one-PR-per-milestone budget and mix trust planes in one review); importing bundles through the tenant fleet-create path (rejected: different scope plane and validation profile, per `docs/AUTH.md`).
- **Patch-vs-refactor verdict:** this is a **refactor** (same behaviour, new runtime); verbatim SQL + hostile-input fixtures keep the privileged plane honest.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — verification cadence approved by Indy:
  > Indy (Aug 26, 2026 11:56 PM): "Run the test-unit* after you have completed all the sections in the milestone." — context: repository-wide unit verification runs after implementation, not per Dimension.
  > Indy (Aug 26, 2026 11:56 PM): "Also the orly gate pr runs make test-integration-rustmd, i prefer we run it at that point." — context: the live datastore integration lane runs at the Pull Request gate; the intended repository target is `make test-integration-rustd`.
