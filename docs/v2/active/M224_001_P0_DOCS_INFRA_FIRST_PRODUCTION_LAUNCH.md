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
**Branch:** feat/m224-dev-verification
**Baseline revision:** 3f2ba3faccf8556c9717fdefd9dee9ab72556660
**Test Baseline:** pending — measure unit/integration lanes before the Pull Request (PR)
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M224_001-launch.md`; repository lane counts remain pending.
**Depends on:** M213_001 transferred release and fleet proofs (Discovery); M235_001 supplies guarded HTTP retrieval, policy, instruction and adapter code; this milestone owns their development verification before production. Founding steps 01–06 precede the first tag (§1).
**Provenance:** drafted with Claude Opus 5.5 from M213_001; reconciled with current founding and native-authoring runbooks. Historical runs are in Discovery.
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox and the scenario pages in Read First

---

## Overview

**Goal:** Release `v0.71.0` serves the production API through its intended tunnel, ships the static runner, signed content-addressed toolbox, software bill of materials (SBOM) and offline bundle, and registers every configured production runner without degradation. Three reference fleets deliver the results in Dimensions 3.1–3.3 on production after their development proofs; the fourth result is inherited development evidence only.
**Problem:** M213_001 transferred its unexecuted first production release and three remaining fleet proofs here. Smoke run 38176618397 failed to resolve `api.agentsfleet.net`; a development build does not prove production installation or delivery.
**Solution summary:** Verify foundational inputs, deploy the control plane with runners disabled, prepare hosts and real runner tokens, then rerun the same release for canary, approved fleet and acceptance. Author, review and Save each fleet using the supported private-draft path; send its trigger only after setup is active. Record correlated final results, not just processed events.

## Execution order

1. Verify the existing development installation and workspace inputs under §1.
2. Consume M235_001 runtime support, complete §4 development verification and run all three development demonstrations under §3.
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
| `ui/packages/app/components/domain/chat/DraftChatHeading.tsx; ui/packages/app/components/domain/chat/FleetSetupActions.tsx; ui/packages/app/tests/workspace-chat/fixture.tsx; ui/packages/design-system/src/design-system/Accordion.tsx` | EDIT | move confirmed navigation into the composer flap, close publishing into real progress, and respect reduced motion |
| `package.json; patches/@assistant-ui%2Freact@0.15.31.patch` → `patches/@assistant-ui%2Freact@0.15.32.patch`; `ui/packages/app/tests/assistant-ui-reserve.test.ts` | EDIT / MOVE | retain the existing resize and scroll fixes while updating the chat dependency |
| `rustd/crates/afd_authoring/src/creation/system.rs; ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | keep system saves unlabelled and make long runtime requests expandable |
| `ui/packages/app/package.json; bun.lock; ui/packages/app/components/domain/MessageExpansion.tsx; ui/packages/app/components/domain/MessageExpansion.test.tsx` | EDIT / CREATE | apply the assistant-ui and Next.js patch updates and share accessible message expansion |
| `ui/packages/app/tests/chat-context-progress.test.tsx; ui/packages/app/tests/draft-chat-recovery.test.tsx; ui/packages/app/tests/draft-client.test.ts; ui/packages/app/tests/draft-thread-messages.test.ts; ui/packages/app/tests/fleet-pending-chat.test.tsx; ui/packages/app/tests/fleet-setup-recovery.test.tsx; ui/packages/app/tests/setup-access.test.tsx` | EDIT | verify setup progress and saved copy through existing conversation boundaries |
| `ui/packages/app/tests/fleet-thread/role-reasoning.test.ts; ui/packages/app/tests/fleet-thread/surface.test.ts; ui/packages/app/tests/fleet-tool-calls.runner.test.tsx; ui/packages/app/lib/events/event-summary.body.test.ts; ui/packages/app/lib/streaming/fleet-stream-frames.figures.test.ts` | EDIT | retain reasoning, stream and runner behavior with collapsed command groups |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable.test.tsx; ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.test.tsx; ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx; ui/packages/app/tests/e2e/acceptance/fixtures/private-chat.ts` | EDIT | assert approved network labels and locate saved-version notices within the transcript |
| `rustd/crates/afd_api/tests/authoring/saving/mod.rs; rustd/crates/afd_api/tests/authoring/saving/provenance.rs; rustd/crates/afd_api/tests/authoring/saving/revisions/apply.rs` | EDIT / CREATE | exercise provenance at the real Save/Apply HTTP boundary and assert durable receipts |
| `ui/packages/app/components/domain/FleetMarkdown.tsx; ui/packages/app/components/domain/chat/DraftConversation.tsx; ui/packages/app/tests/workspace-chat/; ui/packages/app/tests/fleet-markdown.test.tsx` | EDIT | make long messages expandable and verify the approved progress, navigation and copy changes |
| `ui/packages/app/components/domain/FleetToolCalls.tsx; ui/packages/app/tests/fleet-tool-calls.test.tsx; ui/packages/app/tests/fleet-tool-progress.test.tsx; ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts; ui/packages/app/tests/fleet-thread/context-progress.test.ts` | EDIT | group command activity with keyboard-accessible details and recorded outcomes |
| `ui/packages/app/components/domain/chat/SetupStatusFlap.tsx; ui/packages/app/components/domain/chat/SetupStatusFlap.test.tsx; ui/packages/app/components/domain/chat/ChatComposer.tsx; ui/packages/app/components/domain/chat/DraftMessage.tsx; ui/packages/app/components/domain/chat/LibraryBrowser.tsx; ui/packages/app/components/domain/chat/WorkspaceChat.tsx` | CREATE / EDIT | show real setup progress with icons, confirmed versions, Open fleet and pinned New fleet actions |
| `ui/packages/app/lib/api/draft-state.ts; ui/packages/app/lib/api/draft-publication.ts; ui/packages/app/tests/draft-setup-progress.test.ts` | EDIT / CREATE | retain operation progress through uncertain responses and render confirmed publication receipts |
| `ui/packages/app/components/domain/chat/SetupSummary.tsx; ui/packages/app/lib/api/runners-types.ts; ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` | EDIT | describe public internet and restricted access in plain language without changing enforcement |
| `ui/packages/app/components/domain/chat/SetupChecks.tsx; ui/packages/app/tests/workspace-chat/setup-status.test.tsx` | EDIT | show the grant service as one aligned link to its safe approval destination |
| `ui/packages/app/lib/events/fleet-activity.ts; ui/packages/app/tests/setup-activity.test.ts; ui/packages/app/lib/api/drafts.ts; ui/packages/app/lib/api/draft-continuation.ts` | EDIT | label recorded CLI saves and retain dashboard/chat conversation behavior |
| `cli/src/commands/chat/setup-session.ts; cli/test/chat/setup-session.test.ts; cli/test/chat/pinned-source.test.ts` | EDIT | report CLI provenance on Save and Apply while preserving operation identity |
| `rustd/crates/afd_api_wire/src/fleet/input.rs; rustd/crates/afd_api_wire/src/fleet/revision.rs; rustd/crates/afd_api_wire/src/fleet/mod.rs; public/openapi.json` | EDIT | declare optional display provenance on the existing Save and Apply requests |
| `rustd/crates/afd_api_tenant/src/handler/fleet/native/input.rs; rustd/crates/afd_api_tenant/src/handler/fleet/native/revision.rs` | EDIT | parse the supported provenance values without changing access checks |
| `rustd/crates/afd_authoring/src/types/creation.rs; rustd/crates/afd_authoring/src/types/revision.rs; rustd/crates/afd_authoring/src/types/mod.rs; rustd/crates/afd_authoring/src/creation/source.rs; rustd/crates/afd_authoring/src/creation/reserve.rs; rustd/crates/afd_authoring/src/creation/revision/apply.rs; rustd/crates/afd_authoring/src/creation/revision/persist.rs; rustd/crates/afd_authoring/src/creation/chat_save.rs; rustd/crates/afd_events/src/activity.rs` | EDIT | capture provenance in the first immutable receipt without changing semantic retry identity |
| `rustd/crates/afd_authoring/tests/draft/creation/identity.rs; rustd/crates/afd_authoring/tests/draft/creation/fixture.rs; rustd/crates/afd_authoring/tests/draft/creation/improvement.rs; rustd/crates/afd_authoring/tests/draft/creation/stopped.rs` | EDIT | prove first-receipt provenance survives replay metadata changes; keep existing fixtures explicit |
| `tests/fixtures/fleets/ci-responder/definition.json; tests/fixtures/fleets/ci-repairer/definition.json` | EDIT | admit the observed GitHub log host and stop after the first required-source refusal |
| `docs/v2/pending/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` → `docs/v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` | MOVE / EDIT | open the stream and record development-first execution, Dimensions and evidence |
| `playbooks/operations/acceptance/baselines/M224_001-launch.md` | CREATE | revision-bound manual launch and fleet evidence |
| `scripts/toolbox/manifest.txt` | EDIT | refresh the approved Debian snapshot to include the patched Brotli package blocking development deployment |
| `.github/workflows/test.yml; .github/workflows/build-candidate.yml` | EDIT / CREATE | require optimized, unsigned release candidates and the shipping toolbox vulnerability gate before merge |
| `.github/actions/build-toolbox/action.yml; .github/actions/toolbox-tools/action.yml; .github/actions/scan-toolbox/action.yml; .github/actions/sign-toolbox/action.yml` | EDIT / CREATE | separate unsigned construction and scanning from trusted signing without changing size, vulnerability or signature requirements |
| `.github/workflows/deploy-dev-build.yml; .github/workflows/deploy-dev.yml; .github/workflows/deploy-dev-fly.yml; .github/workflows/deploy-dev-metal.yml; .github/workflows/deploy-dev-verify.yml` | EDIT | verify and promote the tested candidate, deploy its immutable image, check its original build identity and retain successful development evidence |
| `.github/workflows/release.yml` | EDIT / CREATE | publish the development-verified native binaries and images on a tag without compiling them again; retain production approvals |
| `scripts/candidate.py; scripts/build-candidate.sh; scripts/release_candidate/; scripts/test_release_candidate.py; make/check-safety-gates.mk; make/release-candidate.mk; Makefile` | CREATE / EDIT | validate source/run identities, payload checksums and retained artifact handoffs with focused failure tests called by Continuous Integration |
| `docs/architecture/release_candidates.md` | CREATE | document each job change, promotion rules, recovery and remaining environment-specific builds |
| `docs/v2/active/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` → `docs/v2/done/M235_001_P0_API_DOCS_INFRA_CI_EVIDENCE_REACHES_SLACK_AND_DRAFT.md` | MOVE / EDIT | close source delivery and map outstanding development verification here |
| `docs/architecture/ci_drill_evidence.md` → `playbooks/operations/acceptance/ci_evidence_format.md` | MOVE / EDIT | keep the pending proof format and attribution procedures alongside the acceptance playbook |
| `rustd/crates/afd_api/tests/integration_slack_mention/mod.rs` | EDIT | register the producer replay proof in the existing integration suite |
| `rustd/crates/afd_api/tests/integration_slack_mention/replay.rs` | CREATE | prove repeated and concurrent producer deliveries keep one admission identity |
| `rustd/crates/afd_api/tests/slack_mention_live/fixture.rs` | EDIT | share signed producer input and captured outcomes for replay |
| `tests/fixtures/fleets/ci-investigation/cases.json` | CREATE | pin the small manual behavior corpus and expected observations |
| `scripts/check-ci-drill-evidence.ts` | CREATE | validate complete evidence records and compute behavior thresholds offline |
| `cli/test/ci-drill-evidence.unit.test.ts` | CREATE | reject missing cases, stale digests, unsupported grades and mismatched source identities |
| `playbooks/operations/acceptance/baselines/M224_001-ci-evidence.json` | CREATE | store sanitized grades, result identities and every hashed witness in its inline named-record list |
| `rustd/crates/agentsfleetd/tests/support/https.rs; rustd/crates/agentsfleetd/tests/support/definition_repair.rs; rustd/crates/agentsfleetd/tests/integration_rust_runner_repairs.rs; rustd/crates/agentsfleetd/tests/integration_rust_runner_repairs/ci/tests.rs` | EDIT | implement remote-state and lost-acknowledgement proof using local boundaries |
| `playbooks/operations/acceptance/baselines/M235_001-ci-evidence.md` | EDIT | retain historical implementation evidence and link new M224 ownership without rewriting results |


External changes use existing runbooks: production tunnel/domain, runner-readiness variable, hosts/tokens, providers, drafts/grants and npm tags. Indy approves provider/host writes and releases. Required source/workflow repairs stop for a scope amendment; none are implicitly included.

Indy approved the setup-chat amendment: grouped command disclosures with actual outcomes, compact save/publish progress and confirmed versions, public/restricted network wording, and Open fleet/New fleet actions. Icons accompany text. Reading and refining labels follow actual request and authoring states. The proposed `.env` editor remains outside this approved amendment. Before review, every changed Rust source file must reach 99% coverage and every changed TypeScript source file 100%; patch-only coverage does not satisfy this requirement.

M235_001 owns the HTTP-tool, policy, reference-instruction and Linux adapter implementation. This milestone owns the transferred deterministic replay/recovery tests, offline evidence checker, behavior campaign and external acceptance below. The approved build amendment below adds candidate promotion to the existing deployment workflows; authentication and production approval boundaries remain unchanged.

## Applicable Rules

- `.orly/standards/code.md` — no speculative compatibility and no dead code.
- `.orly/AGENTS.md` Hard Safety — release and deploy edits need Indy's per-item approval; `gh release create` and `git push --tags` are his.
- `.orly/standards/docs.md` — launch documentation; cross-repository edits require approval.
- `.orly/standards/languages/rust.md`, `docs/RUST_ERROR_STANDARD.md` and the testing standard — native replay/recovery fixtures and observable failure assertions.
- `.orly/standards/languages/typescript.md` — closed evidence schemas, checker errors and complete TypeScript coverage.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | sections complete, no template residue |
| Release / Continuous Integration edits | yes — tag | Indy pushes; workflow fixes require approval by name |
| File & Function Length (≤350/≤50/≤70) | yes — transferred Rust tests and TypeScript checker | split modules and test files without suppressions |
| Rust/TypeScript checks and coverage | yes — transferred implementation | meaningful failure proofs; Rust patch/changed-file floors and TypeScript 100% remain required |

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

### §4 — Development verification before production

M235 delivers runtime and instruction support. This section owns the outstanding tests and actual development proofs; none is required to close M235. Every row below remains P0 and blocks M224 production progression.

**Golden set:** Eight cases in `tests/fixtures/fleets/ci-investigation/cases.json` cover source completeness, identity, scope, injection and explicit repair. New failures add cases and explicitly amend the denominator.

Repeated delivery means the same signed Slack producer event, not a separately authored human message. Send at least 100 concurrent copies through real producer admission, then correlate one admission, event-derived branch and remote draft. Use local deterministic boundaries; never manufacture provider faults against a customer's repository.

For lost ref/draft acknowledgements, let the local remote commit the resource before losing the reply. A retry reads the exact ref and PR identity across all states, creates only the missing resource and stops if reconciliation is unreadable. Instructions do not create a platform exactly-once guarantee.

Keep deterministic network/policy tests, real-model behavior evaluations and external development acceptance as separate evidence. Scripted models prove wiring only; native `gh` log retrieval proves preparation only. Neither can replace the model's own evidence reads or a visible Slack result.

Before live runs, Indy approves the exact implementation revision under `playbooks/deploy/dev/001_playbook.md`. Dispatch `.github/workflows/deploy-dev.yml` on that reviewed branch before M224 development acceptance; verify its `headSha` equals the approved commit. Record successful enabled deployment, observed API commit stamp, active runner identity and passing readiness; do not invent full runtime revisions.

Correlate the API abbreviation to the approved full revision through an unambiguous repository lookup and deployment evidence. Compare the running runner executable's checksum with the selected workflow's binary, preserving a token-free identity probe from that host. These existing operator reads attest the runner without new build fields or workflow edits; unavailable host evidence leaves acceptance incomplete.

Do not change workflows or deploy to production. Do not grant deployment authority to either fleet.

Install the reviewed instructions only after compatible development runtime support is ready. An older runtime or rollback that cannot read required logs returns acceptance to incomplete; the named-gap behavior remains required.

The small corpus contains eight named cases, using actual native fleet runs and documented reviewed setup variants. Cover CI diagnosis, missing link, foreign repository, ambiguous attempt, blocked download with misleading thread text, truncated required log, addressed repair and unsafe repair. Use the same failed development run where possible; the two positive cases are diagnosis and explicit draft repair.

Run every case once per evaluation campaign on its recorded model/settings and accepted instructions; do not combine cases from different campaigns. Capture accepted fleet snapshots before and after each settled case, and finish retaining that case before applying another setup variant. Preserve superseded failed reports in Git before a corrected campaign, using the selection and hashing rules in the evidence format.

Capture native event details and addressed full tool-call reads, retaining identities, original request/answer, call inventory and truncation flags. Match every detail's captured read address and call identifier to that event; missing or omitted required calls leave the case incomplete. Hash exact accepted instruction bytes and canonical accepted configuration, preserving the native review digest separately.

Pin an explicit model reference and retain event-linked lease identity plus existing non-secret model-settings reads under the evidence format's procedure. Record the operator's unchanged-model prerequisite; absent witnesses or settings drift leave the campaign incomplete. The fixed corpus pins each concrete input, reviewed policy variant and expected result before its trigger.

A completed native model turn with zero calls reports an explicit empty trace with `omitted_call_count: 0`. Keep the existing optional wire field for unavailable data; a null trace never proves zero calls. This generic completion fix supports the missing-link case without introducing artificial tool calls or changing permissions.

Prepare positive cases below existing capture limits: 64 KiB per tool output and 1 MiB of records per event. Source truncation and stored-record truncation are different observations; do not raise either limit to obtain a pass. Unexpected record loss or omitted factual witnesses makes a case incomplete.

The intentional truncated-source case may use a truncated stored record only when its retained prefix proves source truncation, status and call identity. Its stopping answer may assert those observed facts, never omitted log content; a missing truncation notice or dropped record cannot pass. The checker tests this expected negative outcome separately from accidental capture loss.

The offline checker validates the corpus denominator, unique case identities, pinned digests and supporting records before computing thresholds. Grounding and required stop/scope behavior must hold in all eight cases; useful outcomes must pass all eight. Manual semantic grading remains attributed review, not an automated factual-truth claim.

Supported review grades are `pass`, `fail` and `incomplete`; only a reviewed pass contributes to either total. Grounding passes when every factual claim has a source witness and all required stop/scope rules hold. Usefulness passes when the case's expected diagnosis, clarification, named stop or bounded draft is actually observed; a correct stop is useful.

Failures and incomplete cases stay in the fixed eight-case denominator and block M224 production progression. An incomplete case lacks required proof; a failure has proof of an incorrect outcome, with that distinction retained in the report. The successful diagnosis/draft flow still requires its own positive evidence, regardless of stopping-case grades.

The development record includes runner/API revisions, accepted setup/digest, fleet/event identifiers and the exact GitHub run attempt/job/commit. Retain sanitized source-read evidence, original-thread trigger/answer permalinks, the addressed repair request, actual draft URL/base/head and reviewed diff. The same regression must fail before the fix and pass at the returned draft head, with command, executor, counts and exit status.

Verify the actual Slack answer and draft-link visibility separately from event processing and delivery scheduling. Indy reviews the externally visible result; the agent records evidence without inventing approval. All three development demonstrations, incident prerequisites and production gates remain required; no result passes from the source implementation alone.

The checker accepts `--report` and `--section` (`grounding`, `behavior`, `development`). Use `playbooks/operations/acceptance/baselines/M224_001-ci-evidence.json`, schema version 1 and one selected campaign. Pin eight unique cases, accepted setup/instruction/model digests, native event/full-call witnesses, review attribution and every inline record under the closed format in `playbooks/operations/acceptance/ci_evidence_format.md`. Implement the checker and meaningful negative/positive unit tests at the repository's TypeScript coverage floor.

Full Linux acceptance runs `make acceptance-codex` on a Linux host with supported direct HTTPS egress, using the unchanged browser and CLI assertions, real Clerk/model fixtures and signed toolbox. This managed Codex VM's deliberate direct identity client cannot use its proxy-only route. The run remains unverified here; do not change authentication, bypass TLS or count scripted output as acceptance. A supported Linux CI host was accepted by Indy. New workflow changes still require their own explicit approval.

- **Dimension 4.1** — Same-event concurrent replay retains one admission/event branch and remote draft → Test `should_keep_one_draft_for_replayed_producer_event`.
- **Dimension 4.2** — Lost ref/draft acknowledgements reconcile before further writes → Test `should_reconcile_uncertain_repair_writes`.
- **Dimension 4.3** — A separate read-only fleet retrieves the exact GitHub job log through its own native call → Test `test_separate_fleet_reads_github_job_log`.
- **Dimension 4.4** — Eight actual-model cases meet grounding, stopping/scope and usefulness thresholds → Test `test_ci_instruction_behavior_corpus`; includes `test_responder_reads_exact_ci_evidence`, `test_responder_stops_on_required_source_gap`, `test_responder_rejects_untrusted_evidence`, `test_addressed_repair_reads_evidence_before_draft` and `test_repairer_stops_on_unsafe_requests`.
- **Dimension 4.5** — The offline checker rejects incomplete, contradictory or stale records → Test `should_reject_incomplete_ci_drill_evidence`.
- **Dimension 4.6** — Actual source reads, same-thread answer, addressed request, bounded draft and unchanged regression correlate → Test `test_development_ci_thread_has_verified_draft`.
- **Dimension 4.7** — Full Linux browser and CLI acceptance passes on the selected host → Test `make acceptance-codex`.

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
| Invalid behavior evidence | absent cases, stale hashes/settings, unsupported or cross-case witnesses | checker rejects and retains failed/incomplete campaign in the fixed denominator; `should_reject_incomplete_ci_drill_evidence` |
| Linux acceptance environment cannot reach identity | direct native identity transport on a proxy-only host | retain the failed run; use the accepted supported Linux CI host, without changing authentication or weakening suites; `make acceptance-codex` remains incomplete |
| Stale review or uncertain repair write | setup changed or response lost | rereview changed setup before Save; read the exact remote branch and all Pull Request states before retrying a repair write; retain the event identity and never merge automatically |

## Invariants

1. A release never deploys a binary whose toolbox manifest does not verify against the built-in public key — enforced by the runner's admission at boot (`afr_sandbox::toolbox::admit`) and `deploy.sh`'s bundle check.
2. The tag matches `VERSION` — enforced by `release.yml`'s `verify-tag` job.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

Launch rows are manual operational acceptance; §4 also adds deterministic integration and checker unit tests. Indy supplies external approvals/triggers; the agent captures non-secret evidence. No row is marked passed before execution. The shared evidence format retains the original M235 test names and concrete assertions; the ownership table below maps them here.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|---|---|---|---|
| 1.2 | manual | `test_dev_readiness_passes` | exact-revision development deployment jobs pass; live health/readiness succeed; doctor proves the selected workspace binding. Models, balance, connections, required secrets and repair grants pass. Fleet-owned GitHub metadata/job-log reads unblock 3.1–3.2 without Grafana. The separate incident sources still gate 3.3; full readiness waits for all required sources. |
| 1.1 | manual | `test_prod_preflight_passes` | founding prerequisites verified and credential gate exits 0; recorded hostname/tunnel identifier/origin match production configuration. A nonempty DNS answer routed elsewhere fails; a missing credential blocks the tag. |
| 2.1 | manual | `test_release_artifacts_are_static_and_addressed` | exact tag/commit release artifacts: zero binary `NEEDED`; toolbox digest and signature verify, length ≤262,848,512 bytes; Release lists runner, signed manifest, SBOM and offline bundle. Tag mismatch or oversize image refuses publication. |
| 2.2 | manual | `test_prod_runner_registers_with_capabilities` | retained run shows control-plane readiness before host/token preparation and canary before approved remaining rollout. Every inventory runner reports all named capabilities and `degraded: false`; exact-version live acceptance, provider gates, API/dashboard and npm `latest` checks pass. Missing token or red canary blocks rollout; skips or one healthy runner do not pass. |
| 3.1 | manual | `test_mention_thread_settles` | selected failed run plus addressed or eligible unaddressed bot mention → matching event and visible final answer in the same channel/thread. Record run, attempt, commit, failed job/step, returned timestamps and a job-log quote from this fleet's own read. Verify a supported proposal against source; Grafana is unnecessary for CI-only failure. Retain trigger/answer permalinks and source-read trace. A processed event, pasted external logs or required-source gap cannot pass. |
| 3.2 | manual | `test_repair_line_opens_one_draft_pr` | explicit addressed repair line → independently verified required evidence and one draft Pull Request on `agentsfleet/linkwarden`, base `dev`, whose diff fixes the selected failure. Post its visible link in the triggering thread. Record event, exact run attempt/commit, trusted branch, PR head and regression validation: failing before the fix, passing at that head. Replay of the same event and uncertain-write retry create no second draft; wrong scope, unrelated diff, missing link or unverified regression fails. |
| 3.3 | manual | `test_workflow_run_thread_settles` | retain reviewed candidate/digest, explicitly published Library revision and saved pinned setup. Readiness is satisfied and fleet active before the selected failed `workflow_run`; its matching processed event has `failure_label=null`, diagnosis identifying the run's failure and one draft Pull Request against the reviewed repository/base with the bounded fix. Record event/run/result/Pull Request links. Pending setup, stale review, an unrelated diagnosis or duplicate repair fails. |

| 4.1 | integration | `should_keep_one_draft_for_replayed_producer_event` | at least 100 concurrent copies of one signed producer event → one admission/event/branch and one external draft; a new internal event cannot stand in for replay |
| 4.2 | integration | `should_reconcile_uncertain_repair_writes` | committed ref/draft with reply loss → reconcile exact remote state and create only a missing resource; unreadable state produces no repeat write |
| 4.3 | manual | `test_separate_fleet_reads_github_job_log` | independent read-only development fleet returns HTTP 200 actual job text through its own call with matching run/attempt/job/commit, accepted setup, event/full-call witnesses and actual quote |
| 4.4 | manual | `test_ci_instruction_behavior_corpus` | all eight manifest cases run on one campaign/model/settings; actual reads support every claim; expected diagnosis, clarification, named stop or bounded draft is useful; no required-source-gap repair advice |
| 4.5 | unit | `should_reject_incomplete_ci_drill_evidence` | preserve the format's complete positive case and prompt-grounded zero-call clarification; missing/duplicate cases, stale digests, invalid spans, cross-case witnesses, contradictory identity/setup/model/runtime, absent review or mixed campaigns are rejected |
| 4.6 | manual | `test_development_ci_thread_has_verified_draft` | original Slack trigger → actual source-read answer → addressed repair request → visible bounded draft link; matched accepted setup/runtime/run/head and same unchanged regression fails before and passes at exact draft head |
| 4.7 | manual | `make acceptance-codex` | unchanged full browser and CLI suites pass on supported Linux host; retain exact revision, command, host/executor, totals and final status; proxy failure, skipped suites or scripted model cannot pass |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | Development readiness, verification and three development demonstrations (§1, §3–§4) | manual — inspect development readiness, M235_001 evidence, and 3.1–3.3 result links in launch baseline | all development assertions and §4 proofs pass before production preflight; CI uses GitHub evidence, incident sources remain independently required | P0 | INCOMPLETE — source reads and all three demonstrations remain unproved; the CI requirement amendment is not a passing result |
| R1 | First-install preflight (§1) | `ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh`; manual — compare hostname/tunnel/origin configuration and preceding founding evidence | exit 0 and all Dimension 1.1 assertions satisfied in launch baseline | P0 | |
| R2 | Artifacts and complete production installation (§2; M213_001 R4) | manual — inspect retained release/run, artifact checks, host inventory and final acceptance in launch baseline | all Dimension 2.1–2.2 assertions satisfied; no missing or skipped final check | P0 | |
| R3 | Three production results plus one inherited development proof (§3; M213_001 R5) | manual — inspect development and production records for 3.1–3.3 and copy M213_001's inherited `github-pr-reviewer` proof into launch baseline | all 3.1–3.3 assertions satisfied in both environments; inherited proof labelled development with source | P0 | |
| R5 | Same-producer replay safety (§4.1; from M235 4.3/R2) | `make test-integration-rustd` | named concurrent replay proof passes with one external draft | P0 | INCOMPLETE |
| R6 | Lost-write acknowledgement recovery (§4.2; from M235 4.4/R2) | `make test-integration-rustd` | named committed-before-reply-loss proofs pass; unreadable state stops | P0 | INCOMPLETE |
| R7 | Separate-fleet actual GitHub log retrieval (§4.3; from M235 5.5) | manual — inspect accepted setup and native source-read records | matching source identities and complete usable fleet-owned text | P0 | INCOMPLETE |
| R8 | Grounding, required stops and scope hold in every case (§4.4; from M235 R3) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M224_001-ci-evidence.json --section grounding` | exit 0; `grounding=8/8; critical_failures=0` | P0 | INCOMPLETE |
| R9 | Every case produces its useful expected result (§4.4; from M235 R4) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M224_001-ci-evidence.json --section behavior` | exit 0; `behavior=8/8; worst_case_failures=0` | P0 | INCOMPLETE |
| R10 | Correlated Slack/draft and unchanged regression proof (§4.6; from M235 R5) | `bun scripts/check-ci-drill-evidence.ts --report playbooks/operations/acceptance/baselines/M224_001-ci-evidence.json --section development` | exit 0; `development=PASS`; exact event/source/thread/draft/head joins | P0 | INCOMPLETE |
| R11 | Full Linux acceptance (§4.7; from M235 5.6) | `make acceptance-codex` | unchanged browser and CLI suites pass on supported Linux host; exact-head totals retained | P0 | INCOMPLETE — this managed VM lacks the direct identity route; accepted Linux CI host not run |
| R12 | Evidence checker implementation and negative proofs (§4.5; from M235 5.2) | `make test-unit-all` | checker tests pass with required TypeScript coverage; malformed/missing/incomplete data cannot pass | P0 | INCOMPLETE |
| S1 | Commit gate green | `orly gate work` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |

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
- **Patch-vs-refactor verdict:** reuse operational paths; add only the explicitly transferred local replay/recovery tests and offline checker. Runtime repairs outside the listed source scope need separate approval.

## Owner-directed transfer from M235

| Original M235 requirement | M224 successor | Acceptance remains |
|---|---|---|
| Dimensions 3.1–3.3, 4.1, 4.5 | Dimension 4.4; R8–R9; existing 3.1–3.2 | eight actual-model cases, exact source reads, required stops and explicit repair |
| Dimension 4.3; replay portion of R2 | Dimension 4.1; R5 | at least 100 concurrent copies of the same signed producer event, one admission/event/branch and one draft |
| Dimension 4.4; uncertain-write portion of R2 | Dimension 4.2; R6 | lost ref/draft acknowledgements reconcile before further writes; unreadable state stops |
| Dimension 5.1; R3–R4 | Dimension 4.4; R8–R9 | grounding and useful expected behavior pass 8/8; failed/incomplete cases remain in the denominator |
| Dimension 5.2 | Dimension 4.5; R12 | implement and test the offline checker; reject missing, stale, contradictory or unsupported evidence |
| Dimension 5.3; R5 | Dimension 4.6; R10 | same-thread source reads, visible answer, addressed request, bounded draft and unchanged regression red/green correlate |
| Dimension 5.5 | Dimension 4.3; R7 | separate read-only development fleet retrieves the exact job log through its own native call |
| Dimension 5.6 | Dimension 4.7; R11 | full Linux browser and CLI acceptance passes on a host with supported HTTPS egress |

Indy requested the following ownership changes on Oct 31, 2026:

> "Replay and lost-acknowledgement tests would you do that in M224"
>
> "Success log retrieval - would you do in M224?"
>
> "the eight case evaluation, slack draft can you do in M224"

For Linux acceptance, Indy identified the agent environment as the blocker:

> "THE LINUX ACCEPTANCE HAS A BLOCKER WHICH IS BLOCKED BY YOU? CRAP ISNT SO"
>
> "if so why are you still keeping that in pending"

The amendment treats that direction as keeping the full Linux acceptance obligation in M224 rather than holding M235 on this managed VM's network limitation. The accepted Linux CI host remains the execution destination; no passing run or waiver is claimed. All transferred criteria stay P0 and block M224's production progression.

## Discovery (consult log)

- **Approved chat follow-up, Oct 31, 2026:** Indy approved grouped command disclosures with actual outcomes, composer save/publish progress, public/restricted internet wording and Open fleet/New fleet actions. He also requested Show more/Show less, icons, truthful reading/refining activity, reduced-motion support and a fluid chat. The `.env` editor remains outside the approved implementation. Whole changed source files require at least 99% Rust coverage and 100% TypeScript coverage. Indy authorized merging this follow-up only after all required checks are green and all Greptile threads are fixed, answered and resolved. M224 remains in progress; the original launch obligations are unchanged.
- **Fleet proof follow-up, Oct 31, 2026:** Responder v2 retrieved the actual job log and delivered its diagnosis, but continued after a denied code search. Repairer v2 stopped at a log refusal caused by missing Bearer authentication in its request. Native revision 3 corrected instruction guidance; its human-triggered repair created Linkwarden draft #4 at `9d114c7e36fcb6274236a1ce07357fe31fd24e81`. It still continued after failed source probes and did not execute the regression. The launch baseline records identities, receipts, truncation flags, successful deployment jobs and remaining acceptance gaps. No production progression follows from these partial results.

- **Owner-requested setup clarity, Oct 31, 2026:** Indy requested an aligned `github` link to workspace approvals and a CLI label on saved setup notices. He directed: "Just put all those fixes in this PR." The existing source Pull Request #793 is merged; these changes use one follow-up. Optional save provenance is display metadata, never authority. Old receipts keep their generic label; chat history remains private and retained until its creator deletes it.
- **Verified continuation, Oct 31, 2026:** Development run `41222658107` passed, including actual runner deployment, browser/command-line acceptance and candidate qualification. Its original native stamp is `3623b66`; the compiled and merged commits share tree `1538fdf13f356396730a85cf26e3b77779227ac7`. Both fresh CI fleets reached active setup revision 1 after Indy's GitHub approvals. The first responder run failed its log read because GitHub selected storage host `productionresultssa3.blob.core.windows.net`; the setup admitted host `productionresultssa6.blob.core.windows.net`. Its 64-call trace violated the required stopping behavior. The launch baseline retains this failed proof; no successful diagnosis, repair or production result follows from it.

- **Approved build reuse amendment (Oct 31, 2026):** Indy requested moving the shipping toolbox Software Bill of Materials and vulnerability check into pull requests, avoiding compilation after merge, and promoting the same native outputs on release tags:

  > "Can you ensure the about build improvements are done in this Pr"

  > "Okay fix with Github action yml in this PR"

  > "Just look how we do builds in the current cI and follow accordingly"

  This authorizes the workflow/action/helper files listed above in Pull Request #793. Pull requests build all existing native release platforms with `dist`, build the runtime containers once, and scan an unsigned toolbox. Trusted development verifies a successful matching source-tree candidate, rescans with current vulnerability data, signs and deploys those outputs. A successful development rollout retains a verified GitHub Actions artifact for 90 days; tags consume it without recompiling native binaries or rebuilding runtime/toolbox images. Missing, stale, tampered or unverified candidates fail closed. Unit, instrumented coverage/kernel tests, environment-specific dashboard builds and npm packaging keep their own requirements. No merge, production tag, production deployment, Dagger migration or new native adversarial review is authorized by this amendment.

- **Approved toolbox snapshot repair (Oct 31, 2026):** Development deployment `41203703412` failed its Critical vulnerability gate on `libbrotli1 1.1.0-2+b7`. Indy approved the proposed one-line change from snapshot `20261015T000000Z` to `20261031T000000Z`:

  > "yes approved the patch."

  The newer security index contains fixed package `1.1.0-2+deb13u1`; the main and updates release files are available. This amendment covers only the manifest refresh and its evidence. A rebuilt image and passing scan remain required; no workflow, scan threshold, signing requirement or production approval changes.

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

- **Scope of the amendment:** M235_001 supplied the independently reviewed source-repair implementation; its original live criteria now belong to this milestone’s §4. The amendment supplies no passing fleet evidence, production authorization or authentication change. Dimension 3.3 keeps its `elastic`, `grafana`, Library and workflow-run requirements.
- **Deferrals** — Indy (2026-10-30): "I think only have grafana for now, we will elastic later". Elasticsearch setup waits for a later continuation. Dimension 3.3 remains incomplete and blocks closure; its incident acceptance requirements remain unchanged.
