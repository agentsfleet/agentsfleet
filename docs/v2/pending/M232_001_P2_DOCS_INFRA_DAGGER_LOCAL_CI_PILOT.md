# M232_001: Measure shared local and cloud checks with Dagger

**Prototype:** v2.0.0
**Milestone:** M232
**Workstream:** 001
**Date:** Oct 24, 2026
**Status:** PENDING
**Priority:** P2 — measure local test and Continuous Integration (CI) feedback
**Categories:** DOCS, INFRA
**Batch:** B1 — evaluation spec included in Pull Request (PR) #784; pilot implementation follows
**Implementation branch:** ci/dagger-local-cloud-pilot
**Baseline revision:** 28593a3daedc201af7a2449cd73a2396360a2fbf
**Test Baseline:** pending — measure declared unit and integration lanes before the pilot implementation PR
**Baseline evidence:** pending — report path or run URL with revision, commands, counts and environment
**Depends on:** none
**Provenance:** drafted with Codex from Indy's approved measured pilot
**Delivery:** This PR includes the evaluation plan only. Cold, warm, changed-input and Cloud proofs remain pending.
**Canonical architecture:** `docs/architecture/testing.md` — Public lanes

## Overview

**Goal:** The same Make checks pass locally and in Dagger Cloud, with measured cache reuse and changed-input invalidation.
**Problem:** Repeated tool installation, compilation and failure diagnosis can delay test feedback. Existing Rust shards and warmed caches require a measured comparison.
**Solution summary:** Pin Dagger `1.0.0-beta.20`, wrap existing Make checks, and compare cold, warm and changed-file runs. Enable Cloud Checks after local validation demonstrates useful reuse. Keep existing required checks during the pilot.

## PR Intent & comprehension handshake

- **PR title:** ci: measure shared local and cloud checks with Dagger
- **Intent:** Developers and coding agents reproduce checks before pushing and reuse unchanged work on later runs.
- **Handshake:** Same Make commands, explicit inputs, measured reuse, and automatic cloud checks after local proof. Assumptions: preserve existing assertions, coverage floors, database resets and native kernel proofs.
- **Quality ceiling:** A thin wrapper preserves current test ownership. Replacing test discovery or Make recipes would widen the pilot without proving faster feedback.
- **Surface checklist:** OpenAPI, product CLI, schema and release version: no changes. Contributor docs and CI integration: yes. Published product docs: no user behavior changes.

## Implementing agent — read these first

1. `docs/architecture/testing.md` — authoritative checks, isolation and native kernel requirements.
2. `make/test.mk` and `make/test-unit.mk` — unit and coverage selections.
3. `make/test-integration-rustd.mk` and `make/test-infra.mk` — real dependencies and reset ownership.
4. `.github/workflows/test-unit-rustd.yml` and `.github/workflows/warm-cache.yml` — existing comparison.
5. `https://docs.dagger.io/getting-started/install/` — installation; the owner explicitly selected beta.20.
6. `https://docs.dagger.io/cloud-checks/` — GitHub integration and automatic checks.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/v2/pending/M232_001_P2_DOCS_INFRA_DAGGER_LOCAL_CI_PILOT.md` | MOVE/EDIT | Record lifecycle and measured outcomes. |
| `dagger.toml` | CREATE | Workspace modules and settings. |
| `dagger.lock` | CREATE | Pin module dependencies. |
| `.dagger/` | CREATE | Checks that call existing Make recipes with explicit inputs. |
| `docs/development.md` | EDIT | Installation, local selection, cache evidence and failure recovery. |
| `docs/architecture/testing.md` | EDIT | Pilot ownership, isolation and adoption limits. |
| `.gitignore` | EDIT | Ignore only generated Dagger runtime output if required. |

Cloud account and GitHub integration are external setup. No GitHub Actions workflow is added for Cloud Checks. No existing required check is removed.

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md`: named constants (UFS), no dead code (NDC), no retained replacement paths (NLR).
- `.orly/dispatch/lifecycle.md`: spec-first opening, final Make checks and measured baselines.
- `.orly/dispatch/name_architecture.md`: preserve the documented test topology.
- `.orly/dispatch/write_documentation.md`: current commands, explicit limitations and concise instructions.
- `.orly/dispatch/write_any.md`: bounded files and functions; no credentials in source or output.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Spec template | yes | Dimensions, failures, proofs and declared commands agree. |
| Architecture consult | yes | Update testing architecture with the wrapper. |
| Secret scan | yes | Pass secrets through Dagger's secret API; exclude private environment files. |
| File shape and length | yes | One small check module; generated clients remain generated. |
| CI edits | external Cloud integration | Owner approved the pilot; measure before enabling automatic runs. |
| Schema, UI and error registry | no | No product behavior changes. |

## Prior-Art / Reference Implementations

Mirror the official beta workspace and Cloud Checks guides. Existing Make recipes remain the repository claims. Compare against the current Rust shards and warmed cache, rather than assuming they are absent.

## Sections (implementation slices)

### §1 — Local checks use the existing lanes

Install beta.20 and verify its version. Discover beta modules before selecting the smallest wrapper for Rust and Bun. Pin modules in `dagger.lock`.

Build tool installation from lockfiles before source inputs. Separate Rust and frontend source inputs so unrelated changes retain reusable work. Exclude credentials, local environment files, build output and `.git` unless a check requires Git metadata.

Use an isolated Docker service for integration recipes that own Docker Compose. Preserve Postgres, the four-node Dragonfly cluster and the repository's other declared dependencies. Each run owns its reset namespace. Never attach the pilot to the shared development database.

Before any reset, prove that the Make client and its owned Docker service share the loopback network used by `make/test-infra.mk`.
The preparation check must reach Postgres, every advertised Dragonfly node, its Transport Layer Security (TLS) endpoint, Mailpit and QStash.
Record run-owned service identities and reject any endpoint outside that run before resetting state.

A Docker API connection alone is insufficient: published ports must be reachable from the test process's namespace.
If the beta cannot provide this routing without changing the existing recipes, record integration as blocked and continue datastore-free measurements only.

Native kernel and systemd proofs remain in their existing native lane. Container execution does not establish host sandbox behavior.

- **Dimension 1.1** — Selected local checks call the authoritative Make recipes and retain failure exits → Test `test_local_make_checks`.
- **Dimension 1.2** — Integration runs start, reset and stop owned dependencies without changing another run → Test `test_isolated_integration_checks`.

### §2 — Measure reuse and prove invalidation

Run the same checks twice on identical inputs. Retain wall time, engine architecture, revision, selected commands, result counts, trace links and cache observations. A cached successful result is identified as reused; it is not reported as a new test execution.

In owned temporary source copies, change one frontend input and one Rust input separately. Verify affected checks rerun and unrelated operations retain reuse. Introduce a known failing assertion in each copy; a reused green result must not hide it. Remove only owned temporary data after retaining proof.

Compare native warm-run time as well as cold-to-warm Dagger time. Report startup/upload overhead. Warm reuse alone does not establish faster everyday development.

Before measuring, record the primary check selection, revision, architecture, processor and memory limits, and three repetitions per warm-run comparison.
Include engine startup and source transfer in elapsed time; report differences in host or operating system explicitly.
Compare the median developer-visible elapsed time for Dagger with the native warm median for that same selection.

- **Dimension 2.1** — Cold, warm and changed-input results distinguish reuse from execution, and injected failures return nonzero → Test `test_cache_reuse_and_invalidation`.

### §3 — Automatic cloud runs follow local proof

Cloud enablement requires passing local checks, invalidation and isolation proofs, plus a lower Dagger warm median than the recorded native warm median.
The implementing agent checks these results against the declared comparison; it cannot relax the criterion or omit overhead.
If the criterion passes, run `dagger cloud checks on` under Indy's existing pilot approval and use the existing Dagger account when available.

If the comparison is slower, inconclusive, or integration remains blocked, leave automatic Cloud runs disabled and record a no-go result.
The evaluation can close with that measured no-go; Cloud proof remains NOT RUN, and adoption requires a later owner decision.
Required account sign-in and repository access remain explicit dependencies; never invent successful integration.

Push the pilot branch and verify its exact revision appears with passing Cloud Checks. Read a failed check's trace and reproduce the selected check locally. Compare cloud timing and resource use with existing checks. Existing required checks stay active until a separate adoption decision.

- **Dimension 3.1** — A qualifying local result leads to matching Cloud push proof; otherwise retain an explicit no-go with Cloud disabled → Test `test_cloud_checks_on_push`.

## Interfaces

`dagger version` must report `v1.0.0-beta.20`. `dagger check -l` lists the installed checks. `dagger check` runs the selected workspace's checks. `dagger cloud checks on` connects the repository; `dagger workspace activity` shows matching runs.

Document the actual supported filters after module discovery. Do not substitute beta.21 or the retired 0.x workspace format.

## Failure Modes

| Mode | Cause | Handling and negative proof |
|---|---|---|
| Cached green after a source change | omitted input | Inject a failing assertion; affected check must rerun and fail in `test_cache_reuse_and_invalidation`. |
| Dependency change reuses old install | missing lockfile input | Change a lockfile in an owned copy and verify dependency work invalidates in the same test. |
| One test run resets another | shared Compose project or state | Concurrent isolated runs retain their own markers in `test_isolated_integration_checks`. |
| Make clients cannot reach owned service ports | separate loopback namespaces or unreachable advertised nodes | Refuse resets until ownership, all cluster nodes and TLS routing pass in `test_isolated_integration_checks`; never substitute shared development endpoints. |
| Container cannot prove kernel behavior | insufficient host features | Keep the existing native lane required; do not mark it covered by `test_local_make_checks`. |
| Missing cloud identity or repository grant | incomplete account setup | Record the prerequisite failure and no passing cloud run in `test_cloud_checks_on_push`. |
| Cache overhead exceeds savings | startup, upload or broad inputs | Retain comparative measurements; leave adoption undecided and automatic runs disabled. |

## Invariants

- Make commands retain their assertions and coverage floors; the wrapper propagates failures.
- Explicit directory and lockfile inputs determine reuse. Integration resets are run-owned.
- No credential enters a source directory, lockfile or trace. External secrets use Dagger secret values.
- Existing required checks and native host proofs remain active during the pilot.
- No model calls are added by the pipeline. Token savings are an observation to measure, not a promised rate.

## Metrics & Observability

Report per-selection elapsed time, exit result, executed or reused state, source revision, architecture and trace. Cloud measurements include observed resource use and cost when available. No product analytics or funnel changes are required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | integration | `test_local_make_checks` | Existing Make checks pass; known command failures propagate. Native kernel coverage is explicitly separate. |
| 1.2 | integration | `test_isolated_integration_checks` | Verify owned service identities and client routing before reset, including every cluster node and TLS. Fresh dependencies, concurrent reset isolation and owned cleanup are verified. |
| 2.1 | integration | `test_cache_reuse_and_invalidation` | Identical inputs show reuse; changed source and lockfiles invalidate; injected failures are observed. Native and Dagger timings are retained. |
| 3.1 | manual | `test_cloud_checks_on_push` | Verify the enablement criterion first. For go, record account access, matching revision, result, trace and local replay. For no-go, retain measurements, leave Cloud disabled and label its execution proof NOT RUN. Missing access blocks a go result. |

## Acceptance Rubric (single scoring surface)

| ID | Outcome | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | Measured local result | `test_cache_reuse_and_invalidation` procedure | Passing cold and warm checks, three-repetition warm medians with overhead, explicit reuse and invalidation proof, and a go/no-go decision against the declared comparison | P1 | |
| R2 | Cloud decision follows evidence | `test_cloud_checks_on_push` procedure | Go: matching revision, passing Cloud Checks and local replay. No-go: Cloud disabled, reason recorded, execution proof NOT RUN | P1 | |
| S1 | Conform | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint | `make lint-all` | exit 0 | P0 | |
| S3 | Unit | `make test-unit-all` | exit 0 with counts | P0 | |
| S4 | Integration | `make test-integration-rustd` | exit 0 with counts and isolated state | P0 | |
| S5 | Version | `make check-version` | exit 0 | P0 | |

## Dead Code Sweep

No production code is replaced. Generated starter examples are removed before shipping the module. Existing workflows remain the comparison during the pilot.

## Out of Scope

Release signing, deployments, product behavior, removal of required checks, changes to test assertions or coverage floors, and replacing native sandbox proofs.

## Product Clarity (authoring record)

1. **Successful moment:** reproduce a failed check locally and see unchanged work reused on the next run.
2. **Preserved behavior:** current tests, coverage and native checks keep their meaning.
3. **Optimal path:** measure the smallest Dagger wrapper against existing warmed checks.
4. **Rebuild or iterate:** add execution and observability around existing recipes.
5. **What ships:** pinned workspace, checks, evidence and contributor instructions.
6. **What stays outside:** deployments, product UI and immediate replacement of required checks.
7. **Fit:** Make owns selection; Dagger owns execution and reuse.
8. **Surface order:** local proof, invalidation proof, then Cloud connection.
9. **Dashboard restraint:** no product dashboard changes.
10. **Recovery:** a failure names its check, trace and local replay command.

## Decomposition & alternatives (patch vs refactor)

Publish this evaluation spec in PR #784, then implement the pilot with existing recipes. Rewriting tests around vendor-specific modules would increase scope and obscure parity. Adopting Cloud before local measurements would leave the user's speed claim untested.

## Discovery (consult log)

- Indy approved: "Approve the measured pilot" covering local tests and CI.
- Indy selected `1.0.0-beta.20` and `dagger cloud checks on`; do not use the 0.x installation.
- Indy confirmed Orly 0.17.0. The base checkout had a stale 0.15.0 pin; `orly update --no-hooks` materialized the matching files. The delivery and companion documentation checkouts already pinned 0.17.0.
- M231 belongs to an existing parked stream; M232 avoids reusing its identifier.
- Indy directed: "the dagger spec must go in this PR". Include this pending evaluation plan in PR #784.
- Astra's publication review identified missing go/no-go criteria and loopback routing proof. Both are now explicit prerequisites before Cloud enablement or integration resets.
- Installation and Orly setup were attempted on the pilot branch; no cold, warm, changed-input or Cloud proof is complete.
- Skills and measured outcomes: pending during implementation.
- Deferrals: none.
