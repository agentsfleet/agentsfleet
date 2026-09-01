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

# M181_003: The daemon-generated OpenAPI document and its coverage gate

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 003
**Date:** Sep 01, 2026
**Status:** PENDING
**Priority:** P0 — the served-versus-documented direction is unguarded on both daemons until this lands
**Categories:** API | DOCS
**Batch:** B7 — serial after M181_002 merges: every annotation sits on a handler that branch carries
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M181_002 **merged** (the full route surface, `Route::verbs`, and the mount-grading test are its diff; annotating an unmerged handler is annotating a moving target)
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — Dimensions 1.2–1.3 of M181_002, split out on Indy's parallelization call; the design decisions live in that spec's Discovery ("Decision — OpenAPI generation is utoipa 5.5.0, feature-gated out of production" and the external review beneath it) and are inherited here, not re-litigated
**Canonical architecture:** `docs/architecture/testing.md` + `docs/REST_API_DESIGN_GUIDELINES.md`

---

## Overview

**Goal (testable):** a `--features openapi` build emits an OpenAPI document generated from the daemon's own handlers; a Rust test proves the served route × method set and the documented set are equal in both directions; the committed `public/openapi.json` equals what the build emits; the production build carries none of it.

**Problem:** the committed document is 321KB of hand-written JSON that nothing generates or grades — the served-versus-documented direction is unguarded on both daemons, and the checker family that once graded the Zig side was deleted with the Zig gating. M181_002 built the served half (`Route::verbs`, graded against the mount); nothing yet produces or grades the documented half.

**Solution summary:** adopt utoipa 5.5.0 exactly as recorded in M181_002's Discovery — optional dependency + `cfg_attr` derives in `afd_wire` (the `json-patch`/`compact_str`/`google-apis-rs` pattern), `#[utoipa::path]` annotations over the existing handlers, one feature-gated `OpenApi` collector per plane crate exposing `document()`, merged at the composition root; an in-process set-equality test as the gate; the committed artifact regenerated from the build and diffed in CI. NOT `utoipa-axum`'s router integration — the total match in `mount.rs` is the stronger invariant and survives untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): the daemon generates its OpenAPI document, gated against the route table
- **Intent (one sentence):** the published API document becomes an artifact the daemon's own build emits and a test grades, so a served route can no longer go undocumented or a documented one unserved.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. M181_002's Discovery — the utoipa decision, the external review (lifetimes, feature gate, per-plane collectors, the rejected `OpenApiRouter`), and the two amendments this spec inherits. Do not re-open those; they carry Indy's quotes.
2. `rustd/crates/afd_api/src/router/mount.rs` — the total match this spec must not break; the reason annotations were chosen over router integration.
3. `rustd/crates/afd_http/src/route/` — `Route::all()` × `Route::verbs()` is the served side of the gate; it is already graded against the mount by `test_declared_verbs_match_the_mounted_router`.
4. `rustd/crates/afd_wire/Cargo.toml` + `src/lib.rs` — the byte-parity contract; the optional feature must leave the default dependency graph and serialization untouched.
5. `~/Projects/oss/mistral.rs/mistralrs-mcp/` and `~/Projects/oss/google-apis-rs/gen/pubsub1/` — local clones of the `cfg_attr` pattern to mirror, and `~/Projects/oss/mistral.rs/mistralrs-server-core/src/openapi_doc.rs:119` as the hand-kept collector list whose drift the gate exists to catch.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/**` | EDIT | `utoipa` as an OPTIONAL dependency; `cfg_attr` schema derives on the public wire types; `value_type` overrides only where the serialized form differs from the Rust form |
| `rustd/crates/afd_api_tenant/**` · `afd_api_runner` · `afd_api_operator` · `afd_api_ingress` · `afd_api` | EDIT | `#[utoipa::path]` annotations over the existing handlers; one feature-gated `OpenApi` collector per plane, each exposing `document()` |
| `rustd/crates/agentsfleetd/**` | EDIT | the feature-gated emitter that writes the merged document |
| `rustd/Cargo.toml` | EDIT | the workspace utoipa dependency, pinned, default-features off |
| `public/openapi.json` | EDIT | regenerated from the build; hand-written prose reconciled into annotations where it survives |
| `make/quality.mk` or CI workflow | EDIT | the artifact-equality diff runs where lint runs — needs Indy's approval if the workflow file itself changes |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no schema derive lands without the collector that reads it), UFS (the feature name and document path as named constants), TST-NAM, MSID, FLL (collector files under the cap), ORP (the retired hand-written sources swept).
- `dispatch/write_http.md` → **`docs/REST_API_DESIGN_GUIDELINES.md`** — the generated document is graded against the design guide; reconciling hand-written prose is judgment, not typing.
- `dispatch/write_rust.md` — the derive pass must not alter any `Serialize` behaviour; REVIEW cites the reference guideline identifiers.
- `dispatch/write_documentation.md` — `public/openapi.json` is published surface; its regeneration travels with client expectations.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length | yes | collectors are per-plane lists, each in its own gated module; annotation blocks live beside their handlers |
| UFS | yes | feature name spelled once per crate manifest; document path a named constant |
| LOGGING | no | no runtime behaviour change — the feature is off in production |
| MILESTONE-ID | yes | none in source |
| SCHEMA GUARD | no | no database schema change |
| CI/CD edit approval | conditional | fires only if the diff touches `.github/workflows/**`; sought at PLAN |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/google-apis-rs/gen/pubsub1/src/api.rs:181` — `#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]` at scale (66 derives), optional dependency in the manifest.
- **Reference:** `~/Projects/oss/mistral.rs/mistralrs-mcp/Cargo.toml:28,33` — the workspace-optional + feature pattern verbatim.
- **Counter-example:** `~/Projects/oss/axum-openapi3/src/lib.rs:24` — registration-site binding over a global mutable registry; the shape this spec rejects.
- **Production evidence** (recorded in M181_002's Discovery): crates.io (axum 0.8.9 + utoipa 5.5.0), Kanidm, Restate, Lakekeeper; Lemmy's drift report is the failure the gate here closes.

## Sections (implementation slices)

### §1 — Wire schemas behind a feature, spike first

The one unknown the external review left open is spiked before the bulk pass: a standalone top-level `Cow<'a, str>` as a reusable component is suspect (`impl<'a, T: ToSchema + Clone> ToSchema for Cow<'a, T>` implies `T: Sized`), while `Cow<'a, str>` as a field is documented to work. The spike is one lifetime-carrying wire type, one `Cow` field, one opaque `serde_json::Value` field.

- **Dimension 1.1** — the spike compiles and emits: a lifetime-carrying type with a `Cow` field and an opaque `Value` field produces a correct schema under the feature → Test `test_schema_spike_lifetime_cow_value`
- **Dimension 1.2** — every public wire type derives `ToSchema` behind the feature; the DEFAULT build's dependency graph and wire bytes are unchanged → Tests `test_default_build_carries_no_utoipa` (dependency-graph assertion) + the existing `afd_wire` byte-parity suite, re-run unchanged
- **Dimension 1.3** — `value_type` overrides exist ONLY where the serialized form differs from the Rust form, each carrying a comment naming the difference → Test `test_value_type_overrides_are_justified`

### §2 — Annotations and per-plane collectors

Every handler gains `#[utoipa::path]`; every plane crate gains one `#[cfg(feature = "openapi")]` collector module exposing `document()`; the composition root merges the five documents. Handlers stay `pub(crate)` — the collector lives inside each crate precisely so nothing is made public for a build-time tool.

- **Dimension 2.1** — every plane's `document()` builds under the feature and the merged document parses as OpenAPI 3.x → Test `test_plane_documents_merge`
- **Dimension 2.2** — the annotations' response codes agree with the handlers' registry codes — each documented error code appears in the handler's refusal mapping → Test `test_documented_codes_match_refusals`

### §3 — The gate and the artifact

The served set (`Route::all()` × `Route::verbs()`) and the documented set (the merged document's paths) are typed values in one process; the gate is their set equality in both directions. The comparison is not redundant: `#[utoipa::path]` restates the path string, so the route table and the annotations are two declarations of route identity and utoipa cannot prove they agree.

- **Dimension 3.1** — served ∖ documented = ∅ and documented ∖ served = ∅, and a seeded removal fails naming the route, the method and the direction → Test `test_coverage_gate_rust_source`
- **Dimension 3.2** — the feature build's emitter writes the document, and the committed `public/openapi.json` equals what it emits — regenerated, never hand-patched → Test `test_openapi_build_is_the_source`
- **Dimension 3.3** — the gate and the diff run where lint runs, and the production build path never enables the feature → Test `test_release_build_excludes_openapi` (asserts the feature absent from the release build invocation and the dependency graph it produces)

## Interfaces

```
openapi (cargo feature)           non-default; the only switch between production and CI builds
document() per plane crate        the one public item each gated collector exposes
public/openapi.json               the committed artifact; regenerated from the build, diffed in CI
Route::all() × Route::verbs()     the served side of the gate — owned by M181_002, read-only here
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Served route absent from the document | handler without an annotation, or annotation missing from a collector list | `test_coverage_gate_rust_source` names the route, method and direction |
| Documented route not served | a stale annotation, or a path string typo'd in `#[utoipa::path]` | same gate, other direction |
| Artifact drifts from the build | `public/openapi.json` hand-edited | `test_openapi_build_is_the_source` fails; the fix is regeneration, never a hand-patch |
| Feature leaks into production | the release build invocation gains the feature | `test_release_build_excludes_openapi` fails on the dependency-graph assertion |
| Wire bytes move under the derive | a derive interacting with serialization | impossible by construction — `ToSchema` touches no `Serialize` impl — and re-proven by the unchanged byte-parity suite (Dimension 1.2) |
| A documented error code no handler answers | prose reconciled wrongly | `test_documented_codes_match_refusals` names the code and the path |

## Invariants

1. The default build of every crate this spec touches has an unchanged dependency graph — `test_default_build_carries_no_utoipa`.
2. The generated document is the only source of the committed artifact — `test_openapi_build_is_the_source`.
3. The total match in `mount.rs` is untouched: no route registration moves into an annotation; enforced by `test_declared_verbs_match_the_mounted_router` (M181_002's, still running) plus review against the Files Changed table.
4. Handlers and their modules stay `pub(crate)` — the collectors live inside each plane crate; `unreachable_pub` stays green.

## Metrics & Observability

No product or operational signal changes: the feature is off in production, and the annotations generate documentation, not behaviour. The one observability-adjacent effect — the retired hand-written document sources — is a Dead Code Sweep entry, not a signal.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit | `test_schema_spike_lifetime_cow_value` | the three suspect shapes emit correct schemas under the feature |
| 1.2 | unit | `test_default_build_carries_no_utoipa` + existing byte-parity suite | default dependency graph unchanged; wire bytes identical |
| 1.3 | unit | `test_value_type_overrides_are_justified` | every override names its serialized-form difference |
| 2.1 | unit | `test_plane_documents_merge` | five `document()`s merge into one parseable 3.x document |
| 2.2 | unit | `test_documented_codes_match_refusals` | documented error codes ⊆ handler refusal codes, per path |
| 3.1 | unit | `test_coverage_gate_rust_source` | set equality both directions; seeded removal fails naming route+method+direction |
| 3.2 | unit | `test_openapi_build_is_the_source` | emitted document == committed artifact, byte-for-byte after canonicalization |
| 3.3 | unit | `test_release_build_excludes_openapi` | release invocation and its dependency graph carry no utoipa |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Served set equals documented set, both directions (§3) | `cd rustd && cargo test --workspace --features openapi coverage_gate` | exit 0 | P0 | |
| R2 | Committed artifact equals the build's emission (§3) | `cd rustd && cargo test --workspace --features openapi openapi_build_is_the_source` | exit 0 | P0 | |
| R3 | Production carries none of it (§1, §3) | `cd rustd && cargo tree -p afd_wire \| grep -c utoipa` | `0` | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S5 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.integration`, `verify.lint`, `verify.version`); S6–S7 are the template's hygiene gates; R-rows name oracles this spec's own sections create.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

The hand-maintained provenance of `public/openapi.json` retires: any residual generator scripts, hand-edit instructions, or documentation claiming the file is hand-written are found by `grep -rn "openapi.json" docs/ scripts/ make/` and updated or removed in this diff. The parity lane's roster (`scripts/parity_lane.sh`) keeps reading the file and is untouched — the file's CONTENT becomes generated; its path and role do not move.

## Out of Scope

- `backfill` parity — deferred with Indy's quote and the measurement in M181_002's Discovery.
- `doctor` parity — dropped by the same evidence standard: no deploy step, playbook or workflow invokes `agentsfleetd doctor`, and its two jobs already ship — bare `agentsfleetd` runs the environment preflight, `/readyz` probes live dependencies and is what the runbook actually uses. Quote in Discovery.
- Schema prose beyond what annotations carry — reconciliation judgment lands here, but rewriting descriptions for style does not.
- `utoipa-axum`, `axum_extras`, or any router-integrated registration — rejected in the inherited decision.
- Serving the document at runtime (`GET /openapi.json` or Swagger UI) — nothing serves it today; adding a route is a product decision, not this gate's.

## Product Clarity (authoring record)

1. **Successful user moment** — an API consumer reads `public/openapi.json` knowing it is what the daemon actually serves, because a test fails when it is not.
2. **Preserved user behaviour** — every request path byte-identical; the production binary is unchanged by construction (R3).
3. **Optimal-way check** — feature-gated generation beats shipping the emitter (production pays zero) and beats hand-maintenance (the drift is the disease); in-process set equality beats the retired external-checker shape (typed values, no serialization boundary).
4. **Rebuild-vs-iterate** — iterate: annotations over existing handlers; the route table stays the source of reachability.
5. **What we build** — feature-gated schemas, annotations, per-plane collectors, the gate test, the artifact diff.
6. **What we do NOT build** — router-integrated registration, owned schema DTOs, a served document endpoint, `backfill`, `doctor`.
7. **Fit with existing features** — the parity lane's roster reads the same file it always read, now generated; the total match keeps owning reachability.
8. **Surface order** — N/A — no new user surface; the document is a published artifact, not an endpoint.
9. **Dashboard restraint** — N/A — no signals.
10. **Confused-user next step** — a contributor whose PR fails the gate reads the failure line: it names the route, the method, and which side is missing it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three slices — spike-then-derives, annotations+collectors, gate+artifact — ordered so the one open risk (the `Cow` papercut) is retired first.
- **Alternatives considered:** building `paths` programmatically from `Route::all()` with annotations carrying only prose (rejected: it fights utoipa's collector model, and the set-equality gate closes the same drift at lower complexity — recorded with the external review); a Python checker over two JSON dumps (rejected: two typed sets in one process need no serialization boundary; the earlier design is amended out in M181_002's Discovery).
- **Patch-vs-refactor verdict:** this is a **patch** adding a generated artifact and its gate; the one genuinely new surface in the family, as the parent spec said.

## Discovery (consult log)

> Indy (2026-09-01): "I donot need parity with zig if its not useful" — context: `doctor` parity dropped from this spec by the evidence standard that already retired `backfill`: nothing operational invokes `agentsfleetd doctor`, and the bare-invocation preflight plus `/readyz` answer both of its questions today.


> Indy (2026-09-01): "I am leaning towards Option 2 — full generation" … "Yes go lets follow the practicse by the crates" … "Yes, 5 specs as drawn" — context: the generation scope, the `cfg_attr` optional-dependency pattern, and this spec's existence as the split-out Dimensions 1.2–1.3 of M181_002. The full decision record, the external review, and the production evidence live in M181_002's Discovery and bind here.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
