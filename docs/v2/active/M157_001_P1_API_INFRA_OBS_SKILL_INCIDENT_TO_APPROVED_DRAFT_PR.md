<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M157_001: A detected incident ends as one approved, bounded draft PR citing its evidence

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 001
**Date:** Aug 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer/operator-facing; the product wedge and the Forge the Future 2026 hackathon entry are the same build
**Categories:** API, INFRA, OBS, SKILL
**Batch:** B1 — single workstream; Sections sequence by dependency
**Branch:** feat/m157-incident-draft-pr
**Test Baseline:** unit=3390 integration=539
**Depends on:** M156_001 (runner leases must complete on the restored dev fleet before any end-to-end proof here can run)
**Provenance:** LLM-drafted (Claude Opus 5, adversarially reviewed by Codex CLI 0.146.0, Aug 01, 2026)
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md` (this spec proves its 🔨 rows; its §6 statuses flip here)

---

## Overview

**Goal (testable):** A seeded regression in an instrumented workload is detected by a scheduled sweep, diagnosed with cited evidence from Elastic's piped query language (ES|QL), and — after exactly one human approval — becomes exactly one bounded draft Pull Request (PR), idempotent under replay and refused on a moved base.

**Problem:** An operational incident today produces a diagnosis at best. Nothing owns the step from "we know the cause" to "a reviewable fix exists", so code-caused incidents fall into limbo between the on-call person who found the cause and the repository where the fix belongs. The architecture scenario documents this repair path and marks its write half unproven (🔨).

**Solution summary:** A read-only investigation run (scheduled sweep over Elastic + Grafana + repository history, one OpenTelemetry-traced incident class) ends by emitting a structured **repair proposal** — evidence, repository, base commit hash, file allowlist, proposed diff — through the existing run-report path. The daemon persists the proposal immutably, content-addresses it, and parks it behind the existing approval machinery (dashboard + Slack). Approval triggers a **deterministic daemon-side apply**: no second model run — the daemon validates bounds and base freshness, mints a short-lived GitHub App token, applies exactly the approved bytes as a branch, and opens a draft PR whose body cites the ES|QL query, the trace identifier, and the commit range that justified it. A benchmark harness with a calibration/evaluation split measures time-to-detect against a frozen threshold baseline, and a demo playbook stands the whole loop up on Amazon Elastic Compute Cloud (EC2) with telemetry flowing into Elastic Cloud on Amazon Web Services (AWS).

**Credentials (milestone gate — enumerate before any M157_002+):**

| Credential | Fetch location | Exists? |
|---|---|---|
| Elastic Cloud endpoint + API key | 1Password `op://ops` (create for the hackathon org) | missing |
| Grafana service-account token | 1Password `op://ops` (demo Grafana provisioned by the §6 playbook) | missing |
| Jira API token or OAuth connector | dashboard connector flow, else 1Password `op://ops` static token | missing |
| GitHub App (read + contents write, PR create) | platform-configured installation | exists |
| Slack connector | existing workspace connector | exists |
| Model provider key for runs | existing tenant-provider mechanism (vault) | exists |
| AWS account for EC2 provisioning | 1Password `op://ops` (human-created) | missing |

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m157): an incident becomes an approved draft PR citing its evidence
- **Intent (one sentence):** An operator whose deploy regressed wakes up to a Slack diagnosis, a Jira issue, and — after one click of approval — a draft PR that shows the query that found it, with nothing merged and nothing written without that approval.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/scenarios/production-deploy-repair.md` — the canonical flow; §3–§4 name the bound-write requirements this spec implements; §6 is the proof ledger this spec flips.
2. `docs/architecture/capabilities.md` §1–§3 — `TRIGGER.md` policy shape, the tool bridge (`${secrets.NAME.FIELD}` substitution, minted GitHub tokens), and the per-lease ExecutionPolicy.
3. `docs/architecture/fleet_bundles.md` §Two layers + §The publish gate — library entries are immutable content-addressed snapshots; publish is `draft` → `public`.
4. `src/agentsfleetd/fleet_runtime/approval_gate.zig` + `approval_gate_async.zig` — `requestApproval` / `resolveApproval` / `EventGateRef` are reused verbatim; this spec adds no parallel approval machinery.
5. `tests/fixtures/fleetbundle/zoho-sprint-daily-summarizer/` — the scheduled-bundle frontmatter shape (`x-agentsfleet: triggers/tools/credentials/network/budget`) the §3 bundle mirrors.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/047_repair_proposals.sql` | CREATE | Immutable proposal rows; single-concern file per schema conventions |
| `schema/embed.zig` | EDIT | Register 047 in the embed + migration array |
| `src/agentsfleetd/fleet/repair_proposal.zig` | CREATE | Pure kernel: parse, validate, canonical hash, base freshness, derived branch name. Extracted so the report path and the apply path share ONE implementation — the hash must be identical at both ends or approval stops binding bytes |
| `src/agentsfleetd/fleet/repair_proposal_test.zig` | CREATE | Kernel unit tests (hash canonicality/immutability, validation refusals, ownership) |
| `src/agentsfleetd/fleet/repair_bounds.zig` | CREATE | Apply-time bounds: unified-diff path extraction against the approved allowlist and caps |
| `src/agentsfleetd/fleet/repair_bounds_test.zig` | CREATE | Bounds unit tests, including the header-hidden-in-hunk case |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | The `UZ-REPAIR-*` entries themselves (the registry file holds only the constants) |
| `src/agentsfleetd/errors/gen_error_codes.zig` | EDIT | Public category copy for the REPAIR family — its comptime coverage gate blocks a family without one |
| `src/agentsfleetd/tests.zig` | EDIT | Test-root registration for the two new fleet modules |
| `src/agentsfleetd/state/repair_proposals.zig` | CREATE | Store: insert, load, status transitions; no UPDATE of content fields |
| `src/agentsfleetd/state/repair_proposals_test.zig` | CREATE | Unit + DB tests for the store |
| `src/agentsfleetd/fleet/repair_proposal_service.zig` | CREATE | Validate structured result block, canonical hash, persist, `requestApproval` |
| `src/agentsfleetd/fleet/repair_apply.zig` | CREATE | Deterministic apply: re-validate, mint, branch + draft PR, refusals |
| `src/agentsfleetd/fleet/repair_apply_test.zig` | CREATE | Apply unit tests (hash, bounds, idempotency key) |
| `src/agentsfleetd/fleet/repair_integration_test.zig` | CREATE | Proposal → approval → apply/refusal against the live test stack |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Detect the structured proposal block on the report path |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `UZ-REPAIR-*` family (stale base, bounds, duplicate, upstream) |
| `library/incident-responder/SKILL.md` | CREATE | The investigation bundle's reasoning prompt |
| `library/incident-responder/TRIGGER.md` | CREATE | Cron trigger, `http_request`-only tools, credentials, host allowlist, budget |
| `bench/incident-response/` | CREATE | Injector, seed manifests, frozen baseline config, scoring, report |
| `make/bench.mk` | EDIT | `bench-incident` target (rubric-mandated caller) |
| `build.zig` | EDIT | `bench-incident` + `bench-incident-test` steps in the `with-bench-tools` graph (repo pattern for bench executables) |
| `playbooks/demo/forge-2026/` | CREATE | EC2 + collector + Elastic Cloud bring-up, failure injection, Kibana, replay proof |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Flip proven rows; record the deterministic-apply implementation shape |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (no dead code at write time), NLR (touch-it-fix-it), NLG (no legacy framing pre-2.0.0 — the scenario doc rewrite describes the target design as *the* design), UFS (proposal status strings, `UZ-REPAIR-*` codes, bundle name shared verbatim across surfaces), ORP (orphan sweep), FLL (length caps).
- `~/Projects/dotfiles/dispatch/write_zig.md` — pg-drain, tagged-union results, errdefer, cross-compile; all new daemon code.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` + STS/NSQ/SGR/ITF — `047` is a new single-concern file; no static strings in DDL; edited in place, never ALTERed (teardown-rebuild era; coordinate with M154's schema-rebuild worktree).
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` + `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured events, init/deinit lifecycles; `~/Projects/dotfiles/dispatch/write_shell.md` — playbook/bench scripts, and vault-reading playbook scripts pass both vault gates (`check-vault-gate-parity`).
- REST guidelines: not applicable — no new public HTTP route; approval rides existing gate surfaces.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; linux test graph clean |
| PUB / Struct-Shape | yes | FILE SHAPE DECISION per new pub surface in `state/` + `fleet/` |
| File & Function Length | yes | apply/validate/notify split across the three new fleet files |
| UFS | yes | status strings, error codes, bundle name, ES index names as named constants |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes | structured events; `UZ-REPAIR-*` registered with negative tests |
| SCHEMA GUARD | yes | new 047 file + `embed.zig` + migration array in one commit |
| UI Substitution / DESIGN TOKEN | no | no UI files in scope |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/{platform-ops,github-pr-reviewer,zoho-sprint-daily-summarizer}` — bundle prose style, constraint framing, scheduled frontmatter; the §3 bundle is their production-grade sibling.
- **Reference:** `docs/architecture/connectors.md` §GitHub App + §Bounded outbound — daemon-side token minting and armed vendor calls; apply reuses, never re-invents.
- **Benchmark:** greenfield — no in-repo prior art; shape defined by this spec's §5 protocol (calibration/evaluation split, frozen baseline).

## Sections (implementation slices)

### §1 — A repair proposal is an immutable, content-addressed record parked behind the existing approval gate

The investigation run's final report may carry one structured repair-proposal block. The report path validates it (shape, size caps, allowlist sanity), persists it as an immutable row, computes a canonical hash over `repo | base_sha | allowlist | diff`, and calls the existing `requestApproval` with an ActionDetail carrying cause, diffstat, bounds, and evidence links — never secret material and never an unbounded blob. **Implementation default:** the proposal rides the run result through `service_report` rather than a new runner tool or protocol verb, because the report path already crosses the trust boundary once per run and carries lease identity.

- **Dimension 1.1** — A valid proposal block persists a row and parks a pending approval → Test `test_proposal_persists_and_parks`
- **Dimension 1.2** — Proposal content is immutable and its hash is canonical (field reorder → same hash; any byte of diff → new hash) → Test `test_proposal_hash_canonical_and_immutable`
- **Dimension 1.3** — A malformed or oversized block degrades to diagnosis-only: no row, no approval, run result preserved → Test `test_malformed_proposal_is_diagnosis_only`
- **Dimension 1.4** — Denial and timeout resolve terminally; nothing is ever applied from a non-approved proposal → Test `test_denied_or_timed_out_never_applies`

### §2 — Approval triggers a deterministic apply: the approved bytes become one draft PR

No second model run. On approval resolution the apply service re-validates the live base against `base_sha`, enforces the file allowlist and diff caps, mints a short-lived GitHub App installation token daemon-side, creates a branch named from the proposal identifier, applies exactly the approved diff, and opens a **draft** PR whose body states cause, evidence (ES|QL query, trace identifier, Grafana reference, commit range), changed files, and rollback note. Refusals (`UZ-REPAIR-*`) notify Slack and never retry silently. **Implementation default:** apply-by-daemon over a hash-bound second model run, because the approved artifact and the shipped artifact must be byte-identical — a second model run reintroduces drift between what a human approved and what lands.

- **Dimension 2.1** — One approval produces exactly one draft PR whose body cites the proposal's evidence → Test `test_approval_opens_one_draft_pr`
- **Dimension 2.2** — Replayed or duplicate approval resolution cannot create a second branch or PR (idempotency keyed on proposal identifier) → Test `test_replay_cannot_duplicate_pr`
- **Dimension 2.3** — A moved base (`base_sha` no longer the branch head) refuses with the stale-base code; no branch is pushed → Test `test_stale_base_refuses`
- **Dimension 2.4** — An allowlist or diff-cap violation at apply time refuses with the bounds code → Test `test_bounds_enforced_at_apply`
- **Dimension 2.5** — Apply authenticates only with a daemon-minted short-lived token; no stored personal token path exists → Test `test_apply_mints_short_lived_token`

### §3 — The incident-responder bundle investigates, diagnoses, and proposes

A library bundle (`library/incident-responder/`) wakes on a cron sweep, queries Elastic (ES|QL over logs, metrics, and one OpenTelemetry trace incident class), queries Grafana, correlates with recent repository history, and produces a structured finding: affected service, incident class, evidence citations. It posts the diagnosis to Slack, opens a Jira issue via `http_request` with the connector credential (static-token fallback if the OAuth mint path is not wired — both ride placeholder substitution), and emits a repair proposal only when the cause is code-shaped and the fix fits the declared bounds. Read-only tools; `http_request` only.

- **Dimension 3.1** — A seeded regression yields a structured finding citing a real ES|QL response digest, never an invented identifier → Test `eval_detection_cites_evidence`
- **Dimension 3.2** — The traced incident names the failing span path and the correlated commit range → Test `eval_trace_incident_names_path_and_commit`
- **Dimension 3.3** — Slack diagnosis and Jira issue carry the evidence links (fake sinks in test) → Test `test_diagnosis_posts_slack_and_jira`
- **Dimension 3.4** — Provider-outage and data-shaped incidents stay diagnosis-only: no proposal emitted → Test `eval_noncode_incidents_stay_diagnosis_only`
- **Dimension 3.5** — Cold install from the published library entry onto a fresh workspace succeeds with declared credentials and hosts → Test `test_cold_install_from_library`

### §4 — Data-plane credentials and library publication use only existing mechanisms

Elastic and Grafana keys are plain workspace secrets (never registry entries, per `connectors.md`), reaching the run only as `${secrets.NAME.FIELD}` placeholders substituted at the tool bridge. The bundle publishes through the existing admin library flow (`draft` → `public`, content-addressed snapshot); no new publish machinery.

- **Dimension 4.1** — Elastic/Grafana secrets stay placeholders in prompt and logs; raw bytes appear only in the egress request → Test `test_data_plane_secrets_stay_placeholders`
- **Dimension 4.2** — A host outside the bundle's allowlist is refused for this bundle's leases → Test `test_undeclared_host_refused_for_responder`
- **Dimension 4.3** — Onboard → publish → workspace-visible → installable, via the existing admin flow → Test `test_responder_publishes_and_lists`

### §5 — The benchmark is honest by construction

`bench/incident-response/` seeds an instrumented corpus and injects incidents from seed manifests split into disjoint calibration and evaluation sets. The threshold baseline (competent error-rate, latency, saturation, multi-window rules) is tuned on calibration only, then frozen by config hash. Detection scores only when a structured result names the affected service and incident class within tolerance, citing a supporting ES|QL result — "anomaly found" scores zero. The report carries recall, false positives, median and 95th-percentile time-to-detect, time-to-actionable-remediation, per-incident variance across repeated runs, query/model cost, and the cases where thresholds win (obvious spikes are expected threshold wins; an agent sweep is honest, not embarrassed, about that).

- **Dimension 5.1** — **DONE** — The injector is reproducible: identical seed manifest → identical corpus hash → Test `test_injector_deterministic`
- **Dimension 5.2** — **DONE** — Calibration and evaluation manifests are disjoint; scoring refuses a mixed corpus → Test `test_seed_manifests_disjoint`
- **Dimension 5.3** — **DONE** — The baseline is frozen: scoring refuses a baseline whose config hash drifted after calibration → Test `test_baseline_frozen`
- **Dimension 5.4** — **DONE** — Scoring requires service + class within tolerance; unstructured claims score zero → Test `test_scoring_requires_service_and_class`
- **Dimension 5.5** — **DONE** — The report emits the full metric set, including variance, cost, and threshold-win cases → Test `test_report_metrics_complete`

### §6 — The demo topology runs on AWS and the stage proof is replay-safe

A playbook stands up: a small multi-service OpenTelemetry-instrumented workload on EC2, the Elastic Distribution of OpenTelemetry collector on EC2 shipping logs/metrics/traces to Elastic Cloud on AWS, an `agentsfleet-runner` host on EC2, failure-injection scripts, and a Kibana view over both the workload telemetry and the indexed `agentsfleet` run events + benchmark results. The stage proof: inject a held-out regression live, watch detection → diagnosis → approval → one draft PR, then replay the approval decision and show no second PR.

- **Dimension 6.1** — The playbook's check mode is idempotent: two consecutive runs both exit clean → Test `playbook_check_idempotent`
- **Dimension 6.2** — An injected failure traverses collector → Elastic and is detected end-to-end on the live stack → Test `e2e_injected_failure_detected`
- **Dimension 6.3** — Run events and benchmark results are indexed in Elasticsearch and visible in the Kibana view → Test `e2e_results_indexed`
- **Dimension 6.4** — Replaying the approval on the live stack cannot produce a second PR → Test `e2e_replay_single_pr`

## Interfaces

```
repair_proposals row: id (Universally Unique Identifier, v7) · workspace/fleet/run identity ·
  repo · base_sha (base commit hash) · file allowlist · unified diff (size-capped) · evidence refs
  (ES|QL query + response digest, trace id, Grafana ref, commit range) · canonical_hash over
  (repo | base_sha | allowlist | diff) · status: pending → approved|denied|timed_out → applied|refused ·
  gate action id (joins the existing approval gates)
Structured result block (run → report path), fenced JSON, versioned "repair_proposal/1":
  { repo, base_sha, files[], diff, cause, evidence[{kind, ref, digest}] } — report path validates
  shape + caps; anything else → the run stays diagnosis-only.
Apply outcomes: applied{pr_url} · refused{code}, code ∈ UZ-REPAIR-* for
  stale_base | bounds_exceeded | duplicate | upstream. Refusals notify Slack; no silent retry.
Approval reuse: requestApproval/resolveApproval/EventGateRef unchanged; ActionDetail carries cause,
  diffstat, bounds, evidence links — deep code review happens ON the draft PR (native surface).
Bundle policy (TRIGGER.md frontmatter, zoho-fixture shape): cron trigger · tools: http_request only ·
  credentials: elastic, grafana, github, jira, slack · network.allow pinned to those hosts · budget caps.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale base | Base branch moved after proposal | Apply refuses with the stale-base code; Slack notice; proposal terminal `refused`; a fresh sweep may propose anew |
| Bounds exceeded | Diff or file set outside the approved allowlist/caps at apply time | Refused with the bounds code; nothing pushed |
| Duplicate resolution | Approval decision replayed / double-delivered | Idempotency key on proposal id; first outcome stands; no second branch or PR |
| Denied / timed out | Human denies, or the gate deadline passes | Terminal status; no write; diagnosis artifacts remain |
| Malformed proposal | Model emits an invalid or oversized block | Report path degrades to diagnosis-only; run result intact; structured log event |
| Data plane unreachable or secret missing | Elastic/Grafana down mid-investigation, or a declared credential absent | Finding degrades honestly (names what it could not read); no proposal; existing stop-the-tool-call codes in the activity stream |
| Upstream write failure | GitHub rejects branch/PR creation | Refused with the upstream code + response class; no partial state (branch without PR is cleaned or reported) |
| Seed drift | Benchmark run over a corpus whose hash mismatches the manifest | Harness refuses to score; names both hashes |

## Invariants

1. No draft PR exists without a daemon-validated proposal — PR creation lives only in the apply service; a repo grep for the PR-creation call returns that single site (rubric-swept).
2. The applied diff is byte-identical to the approved diff — the canonical hash is recomputed at apply and must match the stored hash.
3. At most one branch and one PR per proposal — branch name derived from the proposal id; creation is idempotent on that key (DB uniqueness + upstream existence check).
4. A proposal's content fields never change after insert — the store exposes no content UPDATE; status is the only mutable column.
5. Raw secret bytes never appear in prompt, result, or logs — existing tool-bridge substitution re-asserted by test for the new credential names.
6. The write path authenticates only via daemon-minted short-lived tokens — no stored personal-token configuration exists in the apply service.
7. Benchmark evaluation incidents never inform tuning — calibration and evaluation manifests are disjoint by construction and the scorer enforces it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `repair_proposal_created` | ops | Report path persists a valid proposal | fleet id, repo, diffstat, evidence kinds | no diff bytes, no secrets | `test_proposal_persists_and_parks` |
| `repair_approval_resolved` | ops | Gate resolves approve/deny/timeout | proposal id, resolution, actor kind | no actor PII beyond existing gate fields | `test_denied_or_timed_out_never_applies` |
| `repair_pr_opened` | ops | Apply opens the draft PR | proposal id, pr url, files changed count | no diff bytes | `test_approval_opens_one_draft_pr` |
| `repair_apply_refused` | ops | Any refusal class | proposal id, `UZ-REPAIR-*` code | code only, no payloads | `test_stale_base_refuses` |
| `benchmark_run_completed` | ops | Harness finishes a scored run | corpus hash, metric summary | aggregate numbers only | `test_report_metrics_complete` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_proposal_persists_and_parks` | valid block on report → row + pending gate ref |
| 1.2 | unit | `test_proposal_hash_canonical_and_immutable` | reordered fields → same hash; one diff byte → new hash; content UPDATE path absent |
| 1.3 | integration | `test_malformed_proposal_is_diagnosis_only` | oversized/invalid block → no row, no gate, result preserved |
| 1.4 | integration | `test_denied_or_timed_out_never_applies` | deny and timeout → terminal, apply never invoked |
| 2.1 | integration | `test_approval_opens_one_draft_pr` | approve → exactly one draft PR against fake GitHub; body carries evidence strings |
| 2.2 | integration | `test_replay_cannot_duplicate_pr` | double resolution → one branch, one PR |
| 2.3 | integration | `test_stale_base_refuses` | moved base head → refused{stale}, zero pushes |
| 2.4 | unit | `test_bounds_enforced_at_apply` | out-of-allowlist file / oversized diff → refused{bounds} |
| 2.5 | integration | `test_apply_mints_short_lived_token` | apply calls the broker mint; no static token config read |
| 3.1 | eval | `eval_detection_cites_evidence` | seeded regression → finding cites returned ES|QL digest |
| 3.2 | eval | `eval_trace_incident_names_path_and_commit` | traced failure → span path + commit range named |
| 3.3 | integration | `test_diagnosis_posts_slack_and_jira` | finding → fake Slack + Jira sinks receive evidence links |
| 3.4 | eval | `eval_noncode_incidents_stay_diagnosis_only` | provider-outage seed → no proposal emitted |
| 3.5 | e2e | `test_cold_install_from_library` | fresh workspace + published entry → installed, scheduled, policy attached |
| 4.1 | integration | `test_data_plane_secrets_stay_placeholders` | prompt/log capture free of raw bytes for elastic/grafana names |
| 4.2 | integration | `test_undeclared_host_refused_for_responder` | non-allowlisted host → tool call refused with existing code |
| 4.3 | integration | `test_responder_publishes_and_lists` | onboard → draft → public → visible + installable |
| 5.1 | unit | `test_injector_deterministic` | same manifest twice → identical corpus hash |
| 5.2 | unit | `test_seed_manifests_disjoint` | overlapping ids → scorer refuses |
| 5.3 | unit | `test_baseline_frozen` | baseline hash drift post-calibration → scorer refuses |
| 5.4 | unit | `test_scoring_requires_service_and_class` | unstructured "anomaly" claim → score 0 |
| 5.5 | unit | `test_report_metrics_complete` | report contains every §5 metric incl. threshold-win cases |
| 6.1 | e2e | `playbook_check_idempotent` | check mode twice → both exit 0 |
| 6.2 | e2e | `e2e_injected_failure_detected` | live injected failure → detection event within sweep bound |
| 6.3 | e2e | `e2e_results_indexed` | run events + benchmark docs present in Elasticsearch |
| 6.4 | e2e | `e2e_replay_single_pr` | live replay of approval → PR count stays 1 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Happy path: proposal → approval → one evidence-citing draft PR (§1–§2) | `make test-integration TEST_FILTER='repair'` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Refusal classes hold: stale base, bounds, duplicate (§2) | `make test-integration TEST_FILTER='repair'` (refusal tests included) | exit 0, all three refusal tests listed as pass | P0 | |
| R4 | Cold install of the published bundle on a fresh workspace (§3–§4) | `make test-integration TEST_FILTER='cold_install'` | exit 0 | P0 | |
| R5 | Benchmark reproducibility (§5) | `make bench-incident SEED_MANIFEST=eval` run twice | identical corpus hash line both runs | P0 | |
| R6 | Benchmark honesty: report carries variance, cost, and threshold-win cases (§5) | `make bench-incident SEED_MANIFEST=eval` | report contains `threshold_wins` with ≥1 obvious-spike case | P1 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

### Behaviour evals

- **Grounding rule:** every finding cites only identifiers and digests actually returned by ES|QL or the repository — a fabricated identifier is a P0 ❌.
- **Golden set:** `bench/incident-response/seeds/` — 12 evaluation incidents + 12 clean windows across four classes (obvious spike, slow burn, cross-service trace failure, deployment-correlated regression — the nightmare case) plus non-code seeds for 3.4.
- **Ship threshold:** grounding 100% · structured detection pass ≥ 75% on the evaluation set · 0 critical failures on the deployment-correlated regression class. Each threshold is computed by the `bench-incident` report (rubric R5/R6).
- **Fallback:** low confidence or unreadable data plane → diagnosis-only (named degradation, no proposal); fabricated output is a P0 ❌.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Website retheme around this wedge — separate spec after the loop is proven live.
- Grafana alert-webhook ingress and a native Jira connector integration surface — cut by adversarial review; the scheduled sweep is the only trigger, and `http_request` with an existing credential is the whole Jira story.
- Automatic merge, deploy, rollback, or any write beyond the one draft PR.
- Post-deployment verification runs and Vercel Log Drain intake (scenario §5 / §1) — follow-up workstreams once this loop is proven.
- General agent-authored write scope beyond the proposal-bound apply — any future path where a model writes without a proposal needs its own spec and review.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens Slack to a diagnosis naming the failing span and commit range, clicks Approve, and GitHub shows one draft PR whose body ends with the ES|QL query that found the incident.
2. **Preserved user behaviour** — Existing fleets, triggers, approvals, and the platform-ops diagnosis flow are untouched; a workspace that never installs this bundle sees zero change.
3. **Optimal-way check** — Direct: the loop reuses report, gates, minting, and publish flows; the one new mechanism (proposal + deterministic apply) is the smallest piece that makes a write approvable. Gap to optimal: repository checks run on the draft PR via the repo's own Continuous Integration (CI) rather than pre-push in a runner workspace — acceptable because the draft PR is the native review surface.
4. **Rebuild-vs-iterate** — Iterate; every needed substrate exists and is sound. Determinism improves (apply is deterministic by design).
5. **What we build** — Proposal record + service, deterministic apply, one bundle, secrets docs, benchmark harness, demo playbook, scenario-doc flip.
6. **What we do NOT build** — Everything in Out of Scope; notably no second model run holding write credentials.
7. **Fit with existing features** — Compounds with approvals, connectors, library, schedules, and the event log (replay story). Must not destabilize: the report path — a malformed proposal block must never break ordinary result reporting.
8. **Surface order** — Runtime + bundle first; the dashboard shows nothing new (existing gate card). Command-line and web surfaces unchanged.
9. **Dashboard restraint** — No new proposal UI: the gate card carries cause/diffstat/bounds; the diff is reviewed on the draft PR, where code review already lives.
10. **Confused-user next step** — Every refusal posts a Slack notice carrying its `UZ-REPAIR-*` code and the proposal id; the activity stream shows the same code; the bundle's SKILL.md tells the model to say what it could not read.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections in one Workstream: record+gate (§1) before apply (§2) before the bundle that feeds them (§3); publication (§4), benchmark (§5), and topology (§6) prove it. One PR carries the whole provable loop.
- **Alternatives considered:** (a) hash-bound second model run for the write — rejected: reintroduces drift between approved and shipped bytes; the deterministic apply is strictly stronger and cheaper. (b) Mid-run tool-time approval gating — rejected: the shipped gate binds at lease; pretending otherwise promised a property the code does not provide (adversarial-review finding). (c) Grafana alert webhook as trigger — rejected: second trigger path, muddies time-to-detect, proves nothing the sweep does not.
- **Patch-vs-refactor verdict:** this is a **patch** — a new capability composed from existing substrate; no subsystem is redesigned, and solution-size matches problem-size.

## Discovery (consult log)

- **Consults** — Architecture: `scenarios/production-deploy-repair.md` anticipates mid-run approval prose; this spec's deterministic-apply default diverges deliberately (approval binds bytes, not intentions) and the doc is updated in the same diff (`name_architecture` satisfied). Adversarial review: Codex CLI session (Aug 01, 2026) — verdict GO on this problem pick; its critical finding (the approval gate binds at lease, not at tool time) is the §2 design; its benchmark protocol is §5; its cuts (webhook trigger, 30-day corpus, native Jira work) are adopted.
  - > Indy (2026-08-01): "How about we focus on the logs fo grafana, elastic and git to push a PR? since that is my need now as well?" — context: choosing the wedge this spec implements.
  - > Indy (2026-08-01): "yes pressure test the pick with codex" → "go" — context: adversarial review accepted; spec authoring authorized.
- **Metrics review** — five operator events added (table above); no product-analytics funnel touched; no funnel playbook update required.
- **Skill-chain outcomes** — empty at creation; populated at VERIFY/REVIEW/CHORE(close).
- **Deferrals** — none.
