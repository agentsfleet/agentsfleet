<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec
  skill; after filling, DELETE every "tpl:" guidance comment.
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M224_001: First production launch — tag `v0.71.0` ships the signed Rust runner release to production, with three new production fleet proofs and one inherited development proof

**Prototype:** v2.0.0
**Milestone:** M224
**Workstream:** 001
**Date:** Oct 12, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — first production installation; release and hostname proofs are still required
**Categories:** DOCS, INFRA
**Batch:** B1 — one launch; Indy pushes the tag
**Branch:** feat/m224-dev-first-launch
**Baseline revision:** 7148b1d4ff9a5ad7da3c4ba3c562ec539abef7a1
**Test Baseline:** pending — measure unit/integration lanes before the Pull Request (PR)
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M224_001-launch.md`; repository lane counts remain pending.
**Depends on:** M213_001 transferred release and fleet proofs (Discovery); M235_001 supplies CI log retrieval and instruction proofs before Dimensions 3.1–3.2. Founding steps 01–06 precede the first tag (§1).
**Provenance:** drafted with Claude Opus 5.5 from M213_001; reconciled with current founding and native-authoring runbooks. Historical runs are in Discovery.
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox and the scenario pages in Read First

---

## Overview

**Goal:** Release `v0.71.0` serves the production API through its intended tunnel, ships the static runner, signed content-addressed toolbox, software bill of materials (SBOM) and offline bundle, and registers every configured production runner without degradation. Three reference fleets deliver the results in Dimensions 3.1–3.3 on production after their development proofs; the fourth result is inherited development evidence only.
**Problem:** M213_001 transferred its unexecuted first production release and three remaining fleet proofs here. Smoke run 38176618397 failed to resolve `api.agentsfleet.net`; a development build does not prove production installation or delivery.
**Solution summary:** Verify foundational inputs, deploy the control plane with runners disabled, prepare hosts and real runner tokens, then rerun the same release for canary, approved fleet and acceptance. Author, review and Save each fleet using the supported private-draft path; send its trigger only after setup is active. Record correlated final results, not just processed events.

## Execution order

1. Verify the existing development installation and workspace inputs under §1.
2. Complete M235_001's development source-read prerequisites, then run all three development demonstrations under §3.
3. Verify production prerequisites under §1 after development evidence passes.
4. Release the production control plane, then prepare and deploy its runners under §2.
5. Repeat all three demonstrations in production under §3.
6. Record final evidence and complete the acceptance rubric before closing.

## PR Intent & comprehension handshake

- **PR title (eventual):** chore(m224): record the first production launch and its bundle proofs
- **Intent:** A production fleet owner receives the promised failure diagnosis or draft fix on the Rust runner.
- **Handshake:** Start with development readiness and the three development demonstrations. Production readiness, release, runners and repeated demonstrations follow passing development evidence.
- **ASSUMPTIONS I'M MAKING:** Current authorization covers development inspection and preparation. Indy supplies workspace selection, external approvals and demonstration triggers.

## Implementing agent — read these first

1. `playbooks/founding/README.md` — first installation, steps 07–09 and human approvals.
2. `.github/workflows/release.yml` — tag, artifacts, deployment and acceptance.
3. `docs/v2/done/M213_001_P0_API_DOCS_INFRA_RUST_RUNNER_CUTOVER_ZIG_RETIRED.md` — transferred and inherited proofs.
4. `docs/architecture/fleet_definitions.md` — private drafts, publication and immutable setup.
5. `docs/architecture/user_flow.md` — §8.2.2: health, model, billing, review, Save and activation.
6. `docs/architecture/scenarios/slack-incident-responder.md` — Slack diagnosis and bounded draft repair.
7. `docs/architecture/scenarios/production-deploy-repair.md` — bounded repair; post-merge linkage is unimplemented and excluded.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` → `docs/v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` | MOVE / EDIT | open the stream and record development-first execution, Dimensions and evidence |
| `playbooks/operations/acceptance/baselines/M224_001-launch.md` | CREATE | revision-bound manual launch and fleet evidence |
| `docs/v2/pending/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` | CREATE | specify the separate supporting source repair, Markdown instructions and development acceptance |
| `docs/architecture/ci_drill_evidence.md` → `playbooks/operations/acceptance/ci_evidence_format.md` | MOVE / EDIT | keep the pending proof format and attribution procedures alongside the acceptance playbook |

External changes use existing runbooks: production tunnel/domain, runner-readiness variable, hosts/tokens, providers, drafts/grants and npm tags. Indy approves provider/host writes and releases. Required source/workflow repairs stop for a scope amendment; none are implicitly included.

M235_001 owns the proposed HTTP-tool and reference-instruction changes in its own source scope.
This authoring amendment changes launch requirements and adds that dependency; it implements no source repair.

## Applicable Rules

- `.orly/standards/code.md` — no speculative compatibility and no dead code.
- `.orly/AGENTS.md` Hard Safety — release and deploy edits need Indy's per-item approval; `gh release create` and `git push --tags` are his.
- `.orly/standards/docs.md` — launch documentation; cross-repository edits require approval.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | sections complete, no template residue |
| Release / Continuous Integration edits | yes — tag | Indy pushes; workflow fixes require approval by name |
| File & Function Length (≤350/≤50/≤70) | no | docs only |

## Prior-Art / Reference Implementations

- **Reference:** [development run 38176583117](https://github.com/agentsfleet/agentsfleet/actions/runs/38176583117), build/sign/deploy on `875fe40c9`; production needs separate setup and proof.

## Sections (implementation slices)

### §1 — Development readiness before production preflight

Verify development first using `playbooks/founding/06_verify_dev/001_playbook.md`; do not start another deployment for this inspection.
Record the deployed revision, API health/readiness, runner deployment and browser/command-line acceptance results.
Confirm current workspace binding with `agentsfleet --api https://api-dev.agentsfleet.net doctor --json`.
Check runtime models, balance, connections, required secret existence, repair grants and each demonstration's actual evidence sources before its §3 trigger.
Record outstanding allowlisting, observability and provider verification separately from a green deployment.
Run the three development demonstrations under §3 before continuing to production preflight.

- **Dimension 1.2** — Development deployment evidence, live API checks, workspace binding and demonstration inputs pass → Test `test_dev_readiness_passes`

First verify founding steps 01–06 read-only; resume at the first missing result. Indy creates missing accounts/resources and approves billing through those runbooks before the tag. This inspection can start now; the routine-deploy route assumes an installed system.

Pull Request #784 merged at `89415c2a0`. Development readiness inspection has started; no production release is authorized by this opening.

Record the selected commit and its `VERSION`; `0.71.0` is the current planning example. Recheck before Indy pushes. A changed version requires aligning the title, goal and tag example first.

Run `ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`. Indy supplies named missing credentials directly to the environment-selected vault. Under founding step 03 §3, Indy routes `api.agentsfleet.net` through the production Cloudflare tunnel to `agentsfleetd-prod.internal:3000`. Record tunnel identifier, hostname and origin without secrets. A DNS answer alone cannot prove the route; live `/readyz` follows deployment.

Resolve inputs in each environment before use; record non-secret identifiers and checks:

| Input | Resolution before use | Dependent action blocked on failure |
|---|---|---|
| Access and workspace | Indy logs into the intended environment/workspace; `agentsfleet doctor --json` must pass connectivity, workspace selection and binding. | Review/Save |
| Runtime model and wallet | Prime production model catalogue under founding step 07 after API deployment; Indy approves its price diff. Read `agentsfleet tenant provider show --json` and `agentsfleet billing show`; choose the available runtime model on the draft and fund exhausted balance through existing billing setup. | Activation/trigger |
| Slack channel and CI evidence | Select the channel and exact failed `agentsfleet/linkwarden` run attempt. Connect Slack, join its bot and bind the reviewed mention trigger to the channel identifier. Verify fleet-owned GitHub run/job metadata and job-log reads, including reviewed download hosts. The CI-only definition requires `github`; it does not require `grafana` or `elastic`. | Dimensions 3.1–3.2 |
| GitHub and repair access | Connect the workspace GitHub App to the exact repository; accept required Actions/Checks permissions. Indy approves the reviewed repository-scoped write grant for repairers. Record the trusted base and audit repair-branch workflows for secret exposure before a write. | Repair trigger |
| Incident sources | Indy supplies `elastic` and `grafana` secrets via workspace setup and connects `github` to `agentsfleet/agentsfleet`; record passing readiness checks, not secret values. Select a failed `workflow_run` with a bounded code fix. | Dimension 3.3 |

M235_001 specifies the GitHub log-storage read and source-specific Markdown instructions; prove its development reads before the corresponding production demonstration.
Native review, Save and activation need runtime evidence; fixtures and source inspection do not suffice.
Grafana supplies context only for a configured workload whose logs relate to the selected failure.

An absent Grafana source does not block a GitHub Actions diagnosis or the CI-only setup.
Dimension 3.3 retains its own `elastic`, `grafana` and `github` prerequisites and remains incomplete while a required source is unavailable.

- **Dimension 1.1** — Foundational results and production credential preflight pass; the recorded hostname routes through the intended production tunnel and origin → Test `test_prod_preflight_passes`

### §2 — Release the control plane, then its runners

Follow `playbooks/founding/07_deploy_prod/001_playbook.md`, then 08 and 09:

1. After §1, verify `PROD_RUNNER_READY=false`; Indy approves and pushes the exact tag. Retain commit, `RELEASE_TAG` and `PROD_RELEASE_RUN_ID`. First run builds/signs/publishes artifacts and npm `next`, then deploys API/tunnel/dashboard; runners, live acceptance and `latest` intentionally skip. Public `/readyz` and private Fly.io checks must pass. Indy attaches a missing dashboard domain and approves the model-catalogue price diff before priming.
2. After API readiness, Indy supplies host/Tailscale inventory and approves preparation; every `prepare.sh` check must pass. Complete administrator bootstrap and provider registration/sync from step 08. Rerun the retained release with runners disabled. At Avatar → Platform → Runners Indy creates one runner per host, matching `tailscale-hostname` and selecting Landlock; store once-revealed tokens directly in matching production vault items. Lost tokens require revoke/recreate, never placeholders.
3. After host/provider/token checks, enable `PROD_RUNNER_READY=true` and rerun the same release. Green canary precedes Indy's `production-fleet` approval and remaining rollout. Verify every runner active, exact-version live acceptance, provider/allowlisting/observability gates and public domains. Only then promote npm `latest` and confirm it equals selected `VERSION`. Skips are not final proof.

- **Dimension 2.1** — The binary is static; the toolbox is content-addressed, its manifest verifies against the release public key, and the SBOM and offline bundle ship on the GitHub Release → Test `test_release_artifacts_are_static_and_addressed`
- **Dimension 2.2** — Production API/domain checks and final acceptance pass; every inventory runner registers with Landlock, seccomp, `cpu io memory pids`, bubblewrap and egress enforcement true and `degraded: false` → Test `test_prod_runner_registers_with_capabilities`

### §3 — The reference fleets deliver their promised results

Indy sends triggers; the agent captures evidence. Author private drafts and review name, model, trigger, repository/base, hosts, credentials/connections and grants. Record immutable candidate revision and `review_digest`; changes require rereview.

Indy explicitly publishes the reviewed candidate where Library is required; record its revision. Save the reviewed candidate or pinned Library revision with its digest and retained idempotency key; Save never publishes. Record fleet/setup-operation identifiers. Resolve definition/model/wallet/credential/connection/grant checks via §1; retry the same operation/key and fleet after resolution. No proof trigger before all checks pass and `active`.

Run development proofs first; after §2, repeat setup and triggers in production. `github-pr-reviewer` is inherited development evidence only. Capture environment, revision, setup/fleet, trigger/run/event and final-result links in `playbooks/operations/acceptance/baselines/M224_001-launch.md`; cite it in Session Notes. Exclude secrets and private draft text.

- **Dimension 3.1** — `ci-responder` answers the selected failed run attempt in its Slack thread with GitHub job-log evidence, source timestamps and a supported fix proposal → Test `test_mention_thread_settles`
- **Dimension 3.2** — Addressed `ci-repairer` opens exactly one draft fix Pull Request against `agentsfleet/linkwarden` base `dev`, and posts its link in the triggering Slack thread → Test `test_repair_line_opens_one_draft_pr`
- **Dimension 3.3** — `incident-repairer` is privately authored, reviewed, explicitly published to native platform Library and saved active with satisfied `elastic`, `grafana` and `github` checks. A failed `workflow_run` on `agentsfleet/agentsfleet` produces a diagnosis and one bounded draft fix Pull Request, correlated to that run → Test `test_workflow_run_thread_settles`

People retain merge/deploy authority. Post-merge linkage/verification is unimplemented and excluded; a processed event alone cannot satisfy 3.3.

The CI responder's Markdown instructions select endpoints, read evidence and explain the failure.
The platform enforces reviewed access and delivers the answer to the recorded Slack thread.
Every changed definition needs a fresh review and saved revision before its proof trigger.

A required CI source-read failure ends investigation with the source, run attempt, failed operation and observed refusal or status.
The responder supplies no unsupported cause or repair recommendation, and the repairer stops if it cannot independently verify required evidence.
That stopped outcome proves failure handling; it cannot pass the successful diagnosis or draft-fix demonstration.

The repairer acts only on an explicit addressed request and checks the run evidence, current trusted base and exact branch before writing.
Replay safety covers repeated delivery of the same producer event, including uncertain writes reconciled against remote state.
A separate human request has a separate event identity and does not promise updates to an earlier draft.

## Interfaces

```
git tag v0.71.0 && git push origin v0.71.0          # Indy; recheck the selected commit's VERSION first
GET /v1/fleets/runners/{runner_id}                  # achievable{landlock,seccomp,cgroup_controllers,bubblewrap,egress_enforcement}, degraded
GET /v1/workspaces/{ws}/fleets/{fleet}/events/{id}  # correlate trigger, status=processed, failure_label=null
POST /v1/workspaces/{ws}/fleets                   # reviewed candidate or Library revision + review_digest; retain idempotency key
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Tag and `VERSION` disagree | tag pushed off a stale commit | `verify-tag` fails before any build; delete nothing, re-tag from the right commit (Indy) |
| Production credential missing | vault item absent | `check-credentials` fails first and names the item; nothing deploys |
| Toolbox over its ceiling | an image change grew it | the build refuses before signing; no release is created |
| Runner refuses boot | a controller or capability missing on a production host | `deploy.sh` calls the unit unhealthy and the canary stops the fleet rollout |
| Fleet result fails or evidence unreachable | credentials, log access or egress fault | retain failed event/run identity; post the exact gap and stop unsupported diagnosis or repair. Required-source failure does not pass the demonstration. M235_001 specifies the generic read repair; no production retry before development passes. |
| Fleet setup remains pending | missing readiness input or grant | retain fleet/setup operation and idempotency key; resolve named needs through §1 and retry the same operation; no duplicate fleet or trigger before `active` |
| Partial installation | control plane ready, runner bootstrap incomplete or canary fails | retain release/run and host inventory; keep rollout disabled until bootstrap passes, or stop remaining rollout on canary failure; resume the retained run after approved recovery, never a new tag to hide the failure |
| Stale review or uncertain repair write | setup changed or response lost | rereview changed setup before Save; read the exact remote branch and all Pull Request states before retrying a repair write; retain the event identity and never merge automatically |

## Invariants

1. A release never deploys a binary whose toolbox manifest does not verify against the built-in public key — enforced by the runner's admission at boot (`afr_sandbox::toolbox::admit`) and `deploy.sh`'s bundle check.
2. The tag matches `VERSION` — enforced by `release.yml`'s `verify-tag` job.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

All rows are manual operational acceptance: Indy supplies approvals/triggers; the agent captures non-secret evidence in the launch baseline. No row is marked passed before execution.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|---|---|---|---|
| 1.2 | manual | `test_dev_readiness_passes` | exact-revision development deployment jobs pass; live health/readiness succeed; doctor proves the selected workspace binding. Models, balance, connections, required secrets and repair grants pass. Fleet-owned GitHub metadata/job-log reads unblock 3.1–3.2 without Grafana. The separate incident sources still gate 3.3; full readiness waits for all required sources. |
| 1.1 | manual | `test_prod_preflight_passes` | founding prerequisites verified and credential gate exits 0; recorded hostname/tunnel identifier/origin match production configuration. A nonempty DNS answer routed elsewhere fails; a missing credential blocks the tag. |
| 2.1 | manual | `test_release_artifacts_are_static_and_addressed` | exact tag/commit release artifacts: zero binary `NEEDED`; toolbox digest and signature verify, length ≤262,848,512 bytes; Release lists runner, signed manifest, SBOM and offline bundle. Tag mismatch or oversize image refuses publication. |
| 2.2 | manual | `test_prod_runner_registers_with_capabilities` | retained run shows control-plane readiness before host/token preparation and canary before approved remaining rollout. Every inventory runner reports all named capabilities and `degraded: false`; exact-version live acceptance, provider gates, API/dashboard and npm `latest` checks pass. Missing token or red canary blocks rollout; skips or one healthy runner do not pass. |
| 3.1 | manual | `test_mention_thread_settles` | selected failed run plus addressed or eligible unaddressed bot mention → matching event and visible final answer in the same channel/thread. Record run, attempt, commit, failed job/step, returned timestamps and a job-log quote from this fleet's own read. Verify a supported proposal against source; Grafana is unnecessary for CI-only failure. Retain trigger/answer permalinks and source-read trace. A processed event, pasted external logs or required-source gap cannot pass. |
| 3.2 | manual | `test_repair_line_opens_one_draft_pr` | explicit addressed repair line → independently verified required evidence and one draft Pull Request on `agentsfleet/linkwarden`, base `dev`, whose diff fixes the selected failure. Post its visible link in the triggering thread. Record event, exact run attempt/commit, trusted branch, PR head and regression validation: failing before the fix, passing at that head. Replay of the same event and uncertain-write retry create no second draft; wrong scope, unrelated diff, missing link or unverified regression fails. |
| 3.3 | manual | `test_workflow_run_thread_settles` | retain reviewed candidate/digest, explicitly published Library revision and saved pinned setup. Readiness is satisfied and fleet active before the selected failed `workflow_run`; its matching processed event has `failure_label=null`, diagnosis identifying the run's failure and one draft Pull Request against the reviewed repository/base with the bounded fix. Record event/run/result/Pull Request links. Pending setup, stale review, an unrelated diagnosis or duplicate repair fails. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | Development readiness and three development demonstrations (§1, §3) | manual — inspect development readiness, M235_001 evidence, and 3.1–3.3 result links in launch baseline | all development assertions pass before production preflight; CI uses GitHub evidence, incident sources remain independently required | P0 | INCOMPLETE — source reads and all three demonstrations remain unproved; the CI requirement amendment is not a passing result |
| R1 | First-install preflight (§1) | `ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`; manual — compare hostname/tunnel/origin configuration and preceding founding evidence | exit 0 and all Dimension 1.1 assertions satisfied in launch baseline | P0 | |
| R2 | Artifacts and complete production installation (§2; M213_001 R4) | manual — inspect retained release/run, artifact checks, host inventory and final acceptance in launch baseline | all Dimension 2.1–2.2 assertions satisfied; no missing or skipped final check | P0 | |
| R3 | Three production results plus one inherited development proof (§3; M213_001 R5) | manual — inspect development and production records for 3.1–3.3 and copy M213_001's inherited `github-pr-reviewer` proof into launch baseline | all 3.1–3.3 assertions satisfied in both environments; inherited proof labelled development with source | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Commit gate green | `orly gate work` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading:** Run spec-specific checks as written; record ✅/❌ and decisive output in Graded. All declared repository commands above remain required at the final `orly gate pr` boundary; cite its output in Session Notes. Missing/red P0 evidence blocks completion. Transfers require the user-approved mapping in `.orly/standards/spec.md`; baseline metadata follows SPEC-BASELINE there.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- arm64, dm-verity and Firecracker remain outside this launch.
- Posting the `github-pr-reviewer` review: egress refuses the write today (`docs/architecture/scenarios/github-pr-reviewer.md` §4); M225_001 records the token cost that refusal causes.
- Revoking a compromised toolbox — deferred by Indy in M213_001.

## Product Clarity (authoring record)

- **Useful result:** a production fleet owner receives the promised answer or draft fix for their triggering failure.
- **Preserved behaviour:** development keeps serving; npm `next` is published first and `latest` moves only after final production acceptance (founding step 09).
- **Chosen scope:** reuse first-install runbooks and native authoring; M235_001 supplies the separate generic HTTP and Markdown-instruction work.
- **Next step on failure:** the named input/readiness check blocks its dependent action and identifies recovery; no result is marked complete from a processed event alone.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** verify foundation → control plane → hosts/providers/tokens → canary/approved rollout → active fleets and final results. Each dependent action waits for its named input checks.
- **Alternatives considered:** keeping these rows in M213_001 and parking it (rejected by Indy: "Move to a launch spec (Recommended)").
- **Patch-vs-refactor verdict:** operational execution using existing code; required repairs need separate scope approval.

## Discovery (consult log)

- **Consults** — Transfer from M213_001 (Oct 12, 2026). Inherited whole: M213_001 R4 and Dimension 1.1 → this spec's R2 and Dimension 2.1; M213_001 R5 and the three unproved bundles of its Dimension 1.3 → this spec's R3 and Dimensions 3.1–3.3. The fourth bundle, `github-pr-reviewer`, settled on dev in M213_001 (event `1791777420543-1356`) and carries over as a link, not a Dimension.
  > Indy (2026-10-12): "release tag - defer/park i will push a production push later" — context: M213_001 R4, the first production launch.
  > Indy (2026-10-12): "Move to a launch spec (Recommended)" — context: where M213_001's P0 release row goes.
  > Indy (2026-10-12): "Only the two GitHub ones" — context: which bundle triggers M213_001 runs; the Slack pair moves here.
  Dimension 3.3 (`incident-repairer`) was one of "the two GitHub ones"; it could not run on dev for the blockers below.
  > Indy (2026-10-12): "Move to M224 (Recommended)" — context: the `incident-repairer` `workflow_run` proof, which needs a library entry, a source repository and `elastic`/`grafana` secrets dev lacks.
  Known blockers found while closing M213_001: `api.agentsfleet.net` does not resolve (smoke run 38176618397, `ENOTFOUND`); the dev platform library holds only `github-pr-reviewer`; no `incident-repairer` source repository exists; the dev workspace holds no secrets.
- **Authoring reconciliation** — Founding 07–09 replace the routine-deploy assumption; `latest` promotion follows final acceptance. Draft authoring removes the historical source-repository prerequisite. The Slack log-storage read remains an execution prerequisite; no runtime success is claimed.
- **Metrics review** — no analytics or funnel playbook update required: no product signal changes.
- **Development-first opening:** Indy: "But should we do like check in dev and then go to production readiness and production release, and production runners, and demonstrate again for production and close?" Then: "Can you start on that what do you need for development readiness? 1  - 2" Development inspection and preparation start here; production work remains dependent on passing development results and its named approvals.
- **Opening evidence:** `orly doctor` passes. Development run [41006375734](https://github.com/agentsfleet/agentsfleet/actions/runs/41006375734) passes on the comparison revision. Native `curl` reports version `0.71.0`, commit `7148b1d` and readiness true. Workspace commands report `AUTH_REQUIRED`. Linkwarden exposes no Actions runs or registered workflows to the current GitHub identity. See the launch baseline for commands and remaining inputs.
- **Fresh-session continuation:** The environment API key authenticates `nkishore@megam.io`; doctor passes for workspace `01a154ce-2ad5-7ce5-9493-f3a9d95a0b0e`. The Slack fleets are active with all setup checks ready after human GitHub grant approvals and resumed Save operations. The incident candidate remains unsaved. Channel `C0ARL465ZJP` has resident-delivery evidence and reviewed specialist mention bindings. Missing telemetry credentials, grants, failed development runs and fleet-owned log access block the demonstrations. The launch baseline records immutable candidate identifiers and review digests. No production action occurred.
- **Skill-chain outcomes** — `orly-build` opened the stream; development acceptance and repository verification remain pending.
- **Development draft:** Indy requested checking Linkwarden pull requests and creating one if needed. Draft #3 targets `dev` with a reproducible, test-only title-decoding failure. Indy approved the single test-step workflow amendment, now pushed at `d029957192e53ce4a3a83a928c1ed78a23e75683`. After fork enablement, development run `41027232532` failed at the intended regression step. Its exact job-log endpoint returns HTTP 302, and recent Grafana log queries remain empty; fleet-owned source proofs are still blocked. This preparation is not a fleet-produced repair or demonstration pass.
- **CI source amendment:** Indy clarified that CI failures can lack Grafana and requested Markdown-driven investigation. M235_001 specifies generic admitted job-log downloads, exact attempt/timestamp/source reads, stopping behavior and bounded-repair proof. Dimensions 1.2 and 3.1–3.2 now require their own GitHub evidence without a Grafana dependency.

  Earlier dual-source CI requirements are superseded; historical observations remain unchanged.

  > "I dont want to build a tight integration but rather use the LLM md instructions to tune and tailor what we need."

  > "Okay update the M224 and the above must be spec out using orly-spec skill?"

- **Scope of the amendment:** M235_001 is a pending source-repair dependency with independent review and verification. The amendment supplies no passing fleet evidence, production authorization or authentication change. Dimension 3.3 keeps its `elastic`, `grafana`, Library and workflow-run requirements.
- **Deferrals** — Indy (2026-10-30): "I think only have grafana for now, we will elastic later". Elasticsearch setup waits for a later continuation. Dimension 3.3 remains incomplete and blocks closure; its incident acceptance requirements remain unchanged.
