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
**Status:** IN_PROGRESS
**Priority:** P0 — the served-versus-documented direction is unguarded on both daemons until this lands
**Categories:** API | DOCS
**Batch:** B7 — serial after M181_002 merges: every annotation sits on a handler that branch carries
**Branch:** feat/m181-003-openapi-coverage-gate
**Test Baseline:** unit=2186 integration=156
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
- **Handshake** (filled at PLAN): `public/openapi.json` stops being a document somebody maintains and becomes a build output. The handlers already know their own paths, methods and payloads; utoipa reads that back out under a non-default feature, five per-plane collectors merge into one document, and a test asserts that document's path × method set equals `Route::all() × Route::verbs()` in both directions. The shipped binary compiles none of it.

`ASSUMPTIONS I'M MAKING:`
1. **`#[utoipa::path]` sits on generic handlers.** Every handler is `handle<D: Services>`; the macro's `__path_*` item carries no generics, so the annotation is unaffected. Spiked in §1 before the bulk pass — if it fails, the fallback is a per-plane hand-built `paths` map and §2 changes shape.
2. **`RawValue` is the real `value_type` case, not `Cow`.** `&'a RawValue`, `Box<RawValue>` and `BTreeMap<&'a str, &'a RawValue>` have no `ToSchema` impl and are opaque JSON by design; each takes one override naming that. Borrowed `Cow<'a, str>` fields take none — the external review settled that.
3. **The gate compares path × METHOD, not paths.** Two route identities share one template (`PollSession`/`DeleteSession`, connector `Callback`/`Complete`), so a path-only comparison would pass while a verb went missing.
4. **The committed artifact is replaced wholesale, not merged into.** It documents routes neither daemon serves (`/metrics`, `/v1/connectors/slack/events`) and omits the entire runner plane; hand-written prose is carried into annotations where it survives the reconciliation, and the gate decides what is left.
5. **No new `make` target.** `test-unit-rustd` already runs `cargo test --workspace --all-features` and `lint-rustd` already runs clippy with `--all-features`, so the feature, the gate and the artifact diff are graded by the lanes that exist.
6. **The parity lane's roster grows.** `scripts/parity_lane.sh` reflects over this file, so a regenerated artifact changes which routes the live cutover lane probes. Its hermetic self-test is fixture-driven and unaffected; the live lane's new rows are a M181_006 concern, named in Session Notes rather than silently absorbed.

*Correct me now or I proceed on these.*

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
| `make/quality.mk` or CI workflow | NOT NEEDED | `test-unit-rustd` and `lint-rustd` already pass `--all-features`, so the feature, the gate and the artifact diff are graded by lanes that exist — no new target, no workflow edit, no approval needed |
| `scripts/check_documentation_rules*.py` · `docs/REST_API_DESIGN_GUIDELINES.md` · `docs/EXECUTE_DOC_READS.md` | EDIT | Dead Code Sweep: the lint globbed the deleted `public/openapi/` tree and therefore checked nothing; §6 still called that tree the source of truth |
| `ui/packages/app/tests/workspace-client.test.ts` | EDIT | pinned a claim the daemon never honoured — `name` required on create; corrected with Indy's approval, quoted in Discovery |
| `ui/packages/design-system/src/design-system/DataTableView.tsx` · `ui/packages/design-system/vitest.config.ts` | EDIT | the TypeScript coverage floor goes to 100% on Indy's in-session call; the package sat at 99.78% on one unreachable ref guard, excluded the way `website/src/components/HowItWorks.tsx` already excludes its defensive invariant |
| `ui/packages/website/src/App.test.tsx` | EDIT | the unit lane's flake: four `React.lazy` routes asserted with `findByRole`, whose own 1s timeout is independent of the suite's 20s one; observed failures all landed 1367-1709ms, just past the default. The bound is now stated rather than inherited |

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

- **Dimension 1.1** — DONE — the spike compiles and emits: a lifetime-carrying type with a `Cow` field and an opaque `RawValue` field produces a correct schema under the feature → Tests `a_borrowed_field_is_a_string_and_an_opaque_body_is_an_object`, `a_type_carrying_only_borrowed_text_still_emits_an_object`, `a_map_of_borrowed_keys_to_raw_values_publishes_as_an_object`
- **Dimension 1.2** — DONE — every public wire type derives `ToSchema` behind the feature; the DEFAULT build's dependency graph and wire bytes are unchanged → Test `the_default_build_carries_no_schema_generator` + the existing `afd_wire` byte-parity suite, re-run unchanged
- **Dimension 1.3** — DONE — `value_type` overrides exist ONLY where the serialized form differs from the Rust form, each carrying a comment naming the difference → Test `every_value_type_override_names_its_serialized_difference`

### §2 — Annotations and per-plane collectors

Every handler gains `#[utoipa::path]`; every plane crate gains one `#[cfg(feature = "openapi")]` collector module exposing `document()`; the composition root merges the five documents. Handlers stay `pub(crate)` — the collector lives inside each crate precisely so nothing is made public for a build-time tool.

- **Dimension 2.1** — DONE — every plane's `document()` builds under the feature and the merged document parses as OpenAPI 3.x → Tests `test_coverage_gate_rust_source` (which reads the merged document) + `test_the_gate_compares_a_non_empty_inventory`
- **Dimension 2.2** — DONE — every operation publishes the refusals its own route metadata guarantees: 401 where the guard is not `Open`, 403 where the scope rung is non-empty, 500 wherever a plane is reached → Test `test_documented_codes_match_refusals`

### §3 — The gate and the artifact

The served set (`Route::all()` × `Route::verbs()`) and the documented set (the merged document's paths) are typed values in one process; the gate is their set equality in both directions. The comparison is not redundant: `#[utoipa::path]` restates the path string, so the route table and the annotations are two declarations of route identity and utoipa cannot prove they agree.

- **Dimension 3.1** — DONE — served ∖ documented = ∅ and documented ∖ served = ∅, and a seeded removal fails naming the route, the method and the direction → Test `test_coverage_gate_rust_source`
- **Dimension 3.2** — DONE — the feature build's emitter writes the document, and the committed `public/openapi.json` equals what it emits — regenerated, never hand-patched → Test `test_openapi_build_is_the_source`
- **Dimension 3.3** — DONE — the gate and the diff run where lint runs, and the production build path never enables the feature → Tests `test_release_build_excludes_openapi` + `test_the_release_invocations_are_still_release_invocations`

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

1. The default build of every crate this spec touches has an unchanged dependency graph — `the_default_build_carries_no_schema_generator`.
2. The generated document is the only source of the committed artifact — `test_openapi_build_is_the_source`.
3. The total match in `mount.rs` is untouched: no route registration moves into an annotation; enforced by `test_declared_verbs_match_the_mounted_router` (M181_002's, still running) plus review against the Files Changed table.
4. Handlers and their modules stay `pub(crate)` — the collectors live inside each plane crate; `unreachable_pub` stays green.

## Metrics & Observability

No product or operational signal changes: the feature is off in production, and the annotations generate documentation, not behaviour. The one observability-adjacent effect — the retired hand-written document sources — is a Dead Code Sweep entry, not a signal.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit | `a_borrowed_field_is_a_string_and_an_opaque_body_is_an_object` (+2 siblings) | the three suspect shapes emit correct schemas under the feature |
| 1.2 | unit | `the_default_build_carries_no_schema_generator` + existing byte-parity suite | default dependency graph unchanged; wire bytes identical |
| 1.3 | unit | `every_value_type_override_names_its_serialized_difference` | every override names its serialized-form difference |
| 2.1 | unit | `test_coverage_gate_rust_source` | five `document()`s merge into one parseable 3.x document |
| 2.2 | unit | `test_documented_codes_match_refusals` | every operation publishes the refusals its guard and scope rung guarantee |
| 3.1 | unit | `test_coverage_gate_rust_source` | set equality both directions; seeded removal fails naming route+method+direction |
| 3.2 | unit | `test_openapi_build_is_the_source` | emitted document == committed artifact, byte-for-byte after canonicalization |
| 3.3 | unit | `test_release_build_excludes_openapi` + `test_the_release_invocations_are_still_release_invocations` | no shipping build names a flag that would compile utoipa in |

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
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -vE '\.md$\|^public/\|Cargo\.(lock\|toml)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

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

**Amendment — the tests take the names the codebase uses, not the spec's drafted ones.**
`docs/architecture/testing.md` §"Rust test naming — the function is a sentence"
binds new files to sentence names with no `test_` prefix, and says outright that
a disagreeing spec is amended rather than the codebase renamed. `afd_wire`'s
suite is already overwhelmingly sentence-named. The Dimensions and the Test
Specification table above carry the real names; the drafted `test_*` spellings
were never written.

**Finding — the first cut of Dimension 1.3's test could not fail.** It asked
whether any doc comment sat above a `value_type` override. `missing_docs` is
denied workspace-wide, so every field already carries prose and a stripped
justification still passed — verified by seeding one. The predicate is now that
the doc run above must NAME `value_type`, re-seeded to confirm it fails and
reports the file and line (RULE TCF).

**Measured — the derive is inert on the wire.** 175 `ToSchema` derives and six
`value_type` overrides across 26 files. `afd_wire`'s byte-parity suite passes
unchanged in the default build (154 tests), `cargo tree -p afd_wire` names
utoipa 0 times by default and twice under the feature.

**Finding — the contract declared a field the daemon has never required.** The
generated document publishes `POST /v1/workspaces` with an OPTIONAL body and an
optional `name`, because that is what the handler does: `workspace.rs:158` reads
an empty body as `{}` and `request.name == None` means "name it for me". The
hand-written contract declared `requestBody.required: true` and
`required: ["name"]`, and two tests in `ui/packages/app` pinned that claim. This
is the same served-versus-documented drift this spec closes for routes, showing
up in the schema dimension — and utoipa reproduced half of it, marking every
request body required until the annotation said otherwise.

> Indy (2026-09-02): "Correct the app tests to the daemon's behaviour" —
> context: `ui/packages/app/tests/workspace-client.test.ts` is outside this
> spec's Files Changed, and editing it needed approval. The tests now pin
> behaviour rather than a hand-written assertion about it.

**Deferral — per-field constraint parity with the retired hand-written document.**

> Indy (2026-09-02): "Defer, with the measurement recorded" — context: the
> generated document carries fewer field-level constraints than the document it
> replaces, and they could not be carried mechanically.

Measured, `8f5a95e48:public/openapi.json` against the generated artifact:

| keyword | hand-written | generated |
|---|---|---|
| `maxLength` / `minLength` / `pattern` | 38 / 14 / 3 | 0 / 0 / 0 |
| `maximum` / `maxItems` / `default` | 19 / 4 / 111 | 1 / 0 / 16 |
| `x-stability` | 38 | 0 |
| `format` / `enum` / `example` | 230 / 58 / 9 | 72 / 12 / 0 |
| `application/problem+json` bodies | 11 | 0 |
| `description` | 714 | 1210 |

Why it did not carry: a `(schema, field)` join lands only 11 of 141 constrained
fields, because the port renamed the schemas — the document's identities are now
the Rust type names. The remaining 130 need a hand-mapped identity per schema
AND a check that each constraint is one the daemon actually enforces; copying a
bound the code does not hold would publish a falsehood, which is the defect
above in a new place. `nullable` is NOT in the deficit: the hand-written file
used the 3.0 keyword inside a document declaring 3.1, and the generated one uses
3.1's `type: ["string", "null"]` (69 occurrences).

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

**Amendment — the coverage floors, and why `design-system` joined the blast radius.**
Indy (2026-09-02): "ensure we keep the rustd coverage > 97%" and "and 100% the
ts, tsx". Asked where the TypeScript work should land, the answer was **fold it
into this PR** rather than a follow-up. `cli`, `ui/packages/app` and
`ui/packages/website` were already pinned at 100%; `ui/packages/design-system`
sat at 99 with a measured 99.78% — one statement and one branch, both the
`if (!viewport) return;` guard in `DataTableView.tsx`. The ref is attached to a
div the component renders unconditionally and a layout effect runs only once
that div is in the DOM, so React cannot make the null case true: it is excluded
with `/* v8 ignore next */`, the same resolution `website`'s
`HowItWorks.tsx` already uses for its unreachable defensive invariant, and the
package's four thresholds go to 100.

> Indy (2026-09-02): "Keep it, fix the 23" — context: the branch added a
> contract test the spec never asked for, that every 2xx carrying content
> describes its body. It was red on 23 responses and was the only thing between
> the branch and a PR. Offered removal or an allowlist; Indy kept the gate and
> asked for the 23 to be described.

**Finding — the gate caught two port defects, not only missing annotations.**
Of the 23, twenty were bodies the handler already serialized and the
annotation did not name: seven pre-serialized upstream (the lease answer, the
catalogue page under its entity tag, the bundle tar), the six webhook
acknowledgements, the two Server-Sent Event streams, three schedule views, a
preference bag and a library entry. The remaining three were the ones the
checkpoint called "genuinely bare", and they were not: `service_activity.zig`
answers `202 {"ok":true}` and `callback.zig` answers `200 {"status":"connected"}`
when the connect landed and no dashboard page can be named, and the port had
dropped both bodies. Both are restored as wire types (`activity::ActivityAccepted`,
`connector::Connected`). The relay leg's 200 was an artifact of sharing one
redirect helper with the completion leg: on the browser leg nothing has landed,
so an unwritable relay is now the same `UZ-CONN-001` refusal a dashboard base
that is not a URL already raises, and the annotation drops the 200 it could
not honestly describe. The probes stop building `serde_json::json!` and answer
`health::Liveness` and `health::Readiness`, the shapes `health.zig` fixed.

**Sweep — two wire types nothing served.** `afd_wire::connector::DeliveryIgnored`
and `HandshakeEcho` duplicated `afd_wire::ingress::Ignored` and `EchoAnswer`,
which are what the events route answers; no handler, collector or suite read
them. Removed with their three tests (RULE NLR, on the file this diff touched).

**Amendment — the unit lane's flake was a latent bug, not this branch's.**
Four runs failed in `ui/packages/website/src/App.test.tsx` and one in
`design-system`, always on a different test, always a `React.lazy` route:
`/fleets`, `/privacy`, `/_design-system`. `findBy*` carries its own 1-second
timeout that the suite's 20-second `describe` timeout does not raise, so every
one of those assertions was racing a dynamic import — the durations, 1367ms to
1709ms, are all just past the 1s bound and none of them near 20s. The suite has
been one slow chunk from red since it was written; three worktrees building Rust
concurrently on 2026-09-02 is what finally made it fire. The four call sites now
name `LAZY_ROUTE_TIMEOUT_MS`. `make test-integration-rustd` went green on the
quiet machine unchanged, which is the same story from the Rust side.
