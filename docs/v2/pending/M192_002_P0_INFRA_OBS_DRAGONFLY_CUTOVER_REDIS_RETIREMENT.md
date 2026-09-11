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

# M192_002: Cut over to Dragonfly and retire source Redis

**Prototype:** v2.0.0
**Milestone:** M192
**Workstream:** 002
**Date:** Sep 11, 2026
**Status:** PENDING
**Priority:** P0
**Categories:** INFRA, OBS
**Batch:** B3
**Branch:** pending; proposed operational follow-up, implementation branch assigned at CHORE(open).
**Test Baseline:** pending; record comparison revision at CHORE(open), canonical counts before the Pull Request.
**Depends on:** M192_001 reviewed candidate with passing local/Cloud, import-tool, and preflight proofs; candidate need not be merged before live preparation.
**Provenance:** live procedure aligned with Indy's local-container and manually created Swarm direction after Fable re-review.
**Canonical architecture:** `docs/architecture/datastore_scaling.md`.

## Overview

**Goal (testable):** the approved deployed revision runs on Dragonfly Swarm with reconciled accepted work and authentication state, and source Redis is retired.
**Problem:** a passing implementation or rehearsal does not prove a live provider switch or safe source removal.
**Solution summary:** verify readiness and live inventory, obtain revision-specific approval, cut over under fencing, observe, and record retirement.

This proposed successor owns the complete live outcome as P0. M192_001 completion must never be reported as completion of M192 overall.

## PR Intent & comprehension handshake

- **PR title:** ops: verify Dragonfly cutover and retire source Redis
- **Intent:** close the migration only after live evidence demonstrates preserved work and successful retirement.
- **Handshake:** refuse unsupported source state, missing approval, or incomplete Cloud evidence before any mutation.
- **ASSUMPTIONS I'M MAKING:** environment existence and access are unverified; select targets from actual deployment inventory, never from workflow names alone.

## Implementing agent — read these first

1. `docs/architecture/datastore_scaling.md`, migration identity, authentication, source-state, and recovery rules.
2. `docs/v2/pending/M192_001_P0_API_INFRA_OBS_DRAGONFLY_SCALE_REDIS_PARITY.md`, readiness dependencies and evidence format.
3. `.github/workflows/deploy-dev-fly.yml` and `.github/workflows/release.yml`, source secret references and automatic deployment behavior.
4. `docs/AUTH_DEVICE_LOGIN.md` and `playbooks/operations/teardown/redis/001_playbook.md`, authentication state and destructive-action boundaries.
5. https://www.dragonflydb.io/docs/cloud/datastores, Cloud settings and managed behavior.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| This spec; `docs/v2/reviews/datastore-scale-evidence.md` | EDIT / MOVE | Record live evidence, approval, and completion. |
| `AGENTS.md`, `docs/architecture/{datastore_scaling,data_flow,scaling,testing,roadmap}.md` | EDIT | Describe observed deployment and completed retirement. |
| `playbooks/operations/datastore_scaling/{001_playbook.md,*.sh}`, `playbooks/README.md` | EDIT | Run and finalize the procedure prepared in M192_001. |
| `.github/workflows/{deploy-dev,deploy-dev-fly,deploy-dev-verify,release,bench,test-integration-rustd}.yml`, `deploy/**`, `docker-compose.yml` | EDIT after approval | Select Dragonfly, prevent old-source deployment, remove obsolete server dependencies. |
| `rustd/crates/afd_bench/{src/**/*.rs,tests/**/*.rs}` | EDIT if required | Grade protected import receipt and all-environment resource-retirement proof; runtime transport is already cluster-only. |
| `playbooks/founding/**`, `playbooks/operations/teardown/redis/**` | EDIT / DELETE after caller inventory | Retire source-specific operational procedures and repair all callers. |
| `rustd/crates/afd_bench/{src/**/*.rs,tests/**/*.rs}`, `make/bench.mk` | EDIT if rehearsal gaps require it | Enforce live proof provenance and terminal checks; durability design remains M192_001's. |
| Separate `~/Projects/docs` branch, affected operator pages, and changelog | EDIT during rollout | Document observed provider, Fly secret setup, and recovery procedure. |

Expand exact operational paths before mutation; inventory external datastore and vault consumers separately. No credential values belong in evidence or commits.

## Applicable Rules

- VLT, OWN, ECL, ORP, TCF, and STR in `docs/greptile-learnings/RULES.md`: secret references, recovery, caller sweeps, and real proofs.
- `dispatch/lifecycle.md`, `dispatch/write_rust.md`, `docs/RUST_ERROR_STANDARD.md`, `dispatch/write_shell.md`, `dispatch/write_documentation.md`, and `docs/DOCUMENTATION_RULES.md`: approved operational changes and honest completion.
- `docs/AUTH.md` and `docs/AUTH_DEVICE_LOGIN.md`: migration preserves one-time authentication and expiry.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| SPEC TEMPLATE | Authoring | Mapped manual and automated proofs; complete canonical commands. |
| Deployment/destructive approval | Before each approved action scope | Concrete procedure, target IDs, revision, recovery, and observation window. |
| Architecture, AUTH, VERIFY | Rollout and close | Observed-state docs, session proofs, canonical suites, and authentic live evidence. |

## Prior-Art / Reference Implementations

Use M192_001's validated migration tool and Dragonfly evidence grader. The new daemon is already cluster-only; this spec switches the deployment and retires old source resources without another transport rewrite.

## Sections (implementation slices)

### §1: Verify readiness and authorize the live procedure

Require every M192_001 readiness and rehearsal result for the intended build and topology, including populated source import, closed-until-import admission, and workflow preflight dry-run. Recheck both datastore budgets, sequential frame-batch/request latency, Fly TLS/ACL access to advertised primaries after failover, replicas and backup/restore settings. An unmitigated auth-replay result needs explicit Indy exception before rollout.
Expand the canonical source key-prefix inventory for every actual environment; unknown keys, unresolved no-expiry claims, or unknown outbound jobs block.
Credentials remain references: upstash-dev/api-url under VAULT_DEV and upstash-prod/api-url under VAULT_PROD, plus approved destination references.
Freeze the live revision, import-tool revision, capacity, observation window, reconciliation conditions, pre-admission abort, and forward recovery procedure. Include provider redelivery access/deadlines and historical billing-collision audit disposition.
Indy creates the Swarm datastore and supplies its vault reference; the existing deploy-dev-fly/release flow stages it into Fly after preflight.
Indy approves the environment, tested candidate, fencing/import steps, and first merge/deploy together; missing inputs leave the implementation branch unmerged.

- **Dimension 1.1**: absent readiness, changed resources, or unknown source state refuses the switch → Test `test_live_cutover_preflight_refuses_incomplete_readiness`.
- **Dimension 1.2**: Indy approves the concrete procedure and target revision → Test `review_live_cutover_authorization`.

### §2: Reconcile, switch, and observe

Under the approved procedure, stop every old Fly daemon Machine/external writer and suspend actual restart/deploy paths including deploy-dev-verify. The inspected daemon has no proxy service; fence actual starters. Record stopped IDs and absence of writes/leases/renewals before import.
The tool writes the protected PostgreSQL completion receipt only after reconciliation and Indy's fence confirmation; schema success or an empty database cannot self-authorize admission.
Old IDs, absolute dedup expiry, no-expiry tombstones, sessions, gate mirrors, nonces, anomaly windows and historical billing audit results reconcile before destination admission; preserve legacy orphan nulls and require recorded disposition for detected collision damage. PostgreSQL admission remains the authority.
The workflow calls the self-tested shell preflight before secret/Machine changes; the daemon independently requires the receipt. Pin running and total counts by process/region; when starting from zero Machines use --ha=false, establish approved counts and verify all images before restoring automation.
The pre-admission abort restores old-build schema compatibility and source state before resuming old writers. After admission, recover forward through the rehearsed durable path.
An operator restore keeps writers fenced while the tool purges canonical auth/session, connector nonce and gate-response prefixes across all primaries, reconciles approvals/anomalies and verifies old codes fail; only then reopen.
After the switch, redeliver failed/unconfirmed provider deliveries from the fence window with original identities; GitHub requires explicit redelivery within its documented window. Reconcile results before closing observation.
Run real API/dashboard/CLI acceptance and record live samples for the full frozen window; relabeled rehearsal evidence must fail.

- **Dimension 2.1**: old work, identity, claims, and authentication reconcile at cutover → Test `test_live_reconciliation_preserves_accepted_state`.
- **Dimension 2.2**: live behavior and recovery satisfy the approved observation window → Test `review_live_dragonfly_observation`.

### §3: Retire source Redis and verify the live record

Require reconciliation and observation in every inventoried environment before retiring source resources. Identify every remaining server, workflow, fixture, and vault consumer.
There is no temporary runtime mode to remove; verify the deployed cluster-only build, secret references, and source-consumer inventory before any deletion.
Remove Upstash bindings and source resources only within retirement approval; keep redis-rs, afd_redis::client::Redis for fixtures/import tools, historical evidence, and approved recovery materials.
Record retirement actions with control-plane receipts, configuration commit, live revision, observation evidence, and the named human verifier.
The grader verifies raw digests, authenticated run/artifact provenance, datastore identity, and the manual approval record; a document claiming success is insufficient.

- **Dimension 3.1**: rehearsal-only, forged, or incomplete live records refuse completion → Test `test_rollout_grader_rejects_unverified_retirement`.
- **Dimension 3.2**: source retirement refuses unobserved environments or remaining consumers → Test `test_retirement_requires_all_environments_on_cluster`.
- **Dimension 3.3**: Indy verifies reconciliation, observation, and approved source retirement → Test `review_live_cutover_and_retirement_evidence`.

## Interfaces

Use M192_001's planned `make bench-datastore CHECK=rollout` evidence grader; it must never mutate a deployment.
Discovery records Indy's verbatim approval naming environment, live revision, observation window, and approved retirement scope, followed by the verified retirement commit.
Manual reviews use the operation's 001_playbook.md and attach immutable receipts; no agent fabricates the person's sign-off.

## Failure Modes

| Mode | Handling and negative proof |
|---|---|
| Wrong revision, missing Cloud proof, unexpected source state | Refuse; `test_live_cutover_preflight_refuses_incomplete_readiness`. |
| Duplicate writers, lost old claim TTL, unaccounted active lease | Keep admission stopped; `test_live_reconciliation_preserves_accepted_state`. |
| Rehearsal mislabeled live, altered raw files, missing retirement receipt | Refuse completion; `test_rollout_grader_rejects_unverified_retirement`. |
| Early source deletion or wrong deployed build | Refuse retirement; `test_retirement_requires_all_environments_on_cluster`. |
| Live budget failure | Apply the approved recovery procedure; failed `review_live_dragonfly_observation` blocks retirement. |

## Invariants

1. Source and destination admission never run concurrently during migration; fenced controls and writer checks enforce this.
2. Source destruction follows reconciliation, observation, and explicit approval; the procedure fails closed before destructive actions.
3. Old accepted work and authentication state retain their documented meaning; reconciliation and negative tests enforce it.
4. Live completion requires authentic evidence and human verification; rehearsal files cannot satisfy the rollout grader.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Migration reconciliation | ops | State-class import or drain | Counts, class, revision | No payload/session contents | `test_live_reconciliation_preserves_accepted_state` |
| Live readiness and recovery | ops | Observation samples | Outcome, rate, latency, backlog | Bounded dimensions | `review_live_dragonfly_observation` |
| Retirement verdict | ops | Completion grading | Revision, receipt IDs, outcome | No secrets or connection URLs | `test_rollout_grader_rejects_unverified_retirement` |

Use M192_001's typed operational signals and update the playbook; no product analytics or funnel changes are introduced.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit / integration | `test_live_cutover_preflight_refuses_incomplete_readiness` | Missing approvals/readiness, unresolved auth-replay proof, drifted topology, unknown jobs, wrong revision or unpinned Machine counts refuse mutation. |
| 1.2 | manual | `review_live_cutover_authorization` | Indy approves exact environment, build, procedure, budget, observation window, and recovery; quote and evidence are recorded. |
| 2.1 | integration | `test_live_reconciliation_preserves_accepted_state` | Old IDs, TTLs, sessions, approvals, leases, legacy ledger nulls and accepted work reconcile without duplicate settlement or simultaneous writers; detected billing damage needs explicit disposition. |
| 2.2 | manual | `review_live_dragonfly_observation` | Verify recorded live acceptance commands and raw samples across the full approved window; failures block retirement. Provider redelivery reconciles; newly advertised primary reachability, replica/backup settings, running/total Machine counts and restore/auth recovery meet the frozen requirements. |
| 3.1 | unit / integration | `test_rollout_grader_rejects_unverified_retirement` | Missing/fabricated receipts, incorrect artifact origin, changed digests, and rehearsal-only evidence cannot pass. |
| 3.2 | unit / integration | `test_retirement_requires_all_environments_on_cluster` | Any unswitched/unobserved environment, wrong deployed build, or remaining source consumer blocks resource retirement. |
| 3.3 | manual | `review_live_cutover_and_retirement_evidence` | Indy verifies live revision, reconciliation, observation window, retirement approval/receipts, and retirement commit before completion. |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Approved live migration and source retirement | `make bench-datastore CHECK=rollout` | Exit 0; all live/manual, import-receipt, and source-retirement proofs pass; unaccounted work and unresolved source consumers = 0. | P0 | |
| R2 | Dashboard live acceptance | `make acceptance-e2e` | Exit 0 against the recorded live Dragonfly revision. | P0 | |
| R3 | CLI live acceptance | `make cli-acceptance` | Exit 0 against the same live revision. | P0 | |
| S1 | Conformance | `make harness-verify` | Exit 0. | P0 | |
| S2 | Unit verification | `make test-unit-all` | Exit 0; coverage gates pass. | P0 | |
| S3 | Integration verification | `make test-integration-rustd` | Exit 0; nonzero passing count. | P0 | |
| S4 | Lint | `make lint-all` | Exit 0. | P0 | |
| S5 | Version | `make check-version` | Exit 0. | P0 | |
| S6 | Secrets | `gitleaks git --no-banner` | Exit 0; no leaks. | P0 | |

Missing live approval or evidence leaves this spec incomplete. Authoring and M192_001 readiness cannot fill these Graded cells.

## Dead Code Sweep

Use `git grep -n -w -i redis` and `git grep -n -w -i upstash`; classify protocol names, history, and actual source dependencies.
Remove only inventoried obsolete server bindings and callers; retain redis-rs and historical baseline evidence. Verify external consumers before deleting vault items.

## Out of Scope

Reopening M192_001 durability design, adding connector delivery, automatic fallback to Redis, and deleting unrelated shared infrastructure are excluded.
A production environment is not assumed to exist merely because a release workflow references it.

## Product Clarity (authoring record)

1. **Successful user moment:** fleet workflows keep working after the approved live switch and source retirement.
2. **Preserved user behaviour:** accepted work, IDs, login, approvals, ordering, fencing, and billing survive.
3. **Optimal-way check:** reuse one proven procedure and verify every state class before destructive retirement.
4. **Rebuild-vs-iterate:** operational completion of the tested M192_001 refactor.
5. **What we build:** live configuration, authentic migration evidence, and retired source bindings.
6. **What we do NOT build:** another migration tool, provider fallback, or unrelated infrastructure changes.
7. **Fit with existing features:** preserve application and authentication paths established by readiness tests.
8. **Surface order:** evidence and operator approval precede live changes.
9. **Dashboard restraint:** no migration-complete claim before live observation and retirement.
10. **Confused-user next step:** the rollout grader names the missing approval, receipt, state class, or budget.

## Decomposition & alternatives (patch vs refactor)

This proposed operational follow-up owns the live outcome separately from readiness, keeping both Pull Requests mechanically verifiable.
A single implementation PR claiming unperformed retirement or relying on a parked-spec gate exception is rejected.

## Discovery (consult log)

- **RTT provenance:** Fable reports roughly 5 ms accepted for Fly iad → AWS us-east-1; measure before freezing performance thresholds. This is not a direct Indy quote or an auth-risk exception.
- **Indy override (verbatim):** "we just stick to local that runs containers today (with the cluster config, no single mode crap for dragonfly)". The new daemon is cluster-only; retirement concerns source resources.
- **Indy deployment direction (verbatim):** "in production this would be stood up by Indy on dragondb just like indy did for upstash and stick the key in deployment to deploy-dev.yml". Indy supplies the datastore and vault reference; the existing called workflow stages the Fly secret.
- **Transfer mapping:** M192_001's former live Dimension 7.4 maps to 1.2, 2.2, and 3.3 here; its live R6 outcome maps to R1 here, all P0.
- **Metrics review:** use existing migration signals; no product analytics changes.
- **Skill-chain outcomes:** orly-spec-new authored the proposed live follow-up; no runtime or manual verdict is claimed.
- **Deferrals:** none; this pending spec explicitly owns the live outcome.
