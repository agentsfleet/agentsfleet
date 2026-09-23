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

# M206_004: In #ci-dev, then #ci-prod, a failed run's thread gets an evidence-cited diagnosis, and a fix waits for approval

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 004
**Date:** Sep 23, 2026
**Status:** PENDING
**Priority:** P1 — the operator-facing proof of the whole milestone; nothing before it has touched a real Slack thread, a real failed run, or a real Grafana stack.
**Categories:** API, DOCS, INFRA, SKILL
**Batch:** B1 for §1–§4, which share no source file with M206_001–003; §5's drills run in B3, after B2 reaches `api-dev`. Its own Pull Request, the milestone's follow-up.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M206_001, M206_002, M206_003 — delivery, routing and the approval boundary the drill exercises.
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4`
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §7–§11

---

## Overview

**Goal (testable):** in `#ci-dev` a mention in a failed run's thread gets one threaded answer that cites the run, its failed job and step, a log excerpt, and Grafana readings over the run's window, or names each source it could not read; `@agentsfleet ci-dev-repairer open the fix` parks, and after a workspace member approves, exactly one draft Pull Request (PR)'s link lands in the thread; the same holds in `#ci-prod` once the development drill has passed.

**Problem:** the fleet cannot read the evidence a Continuous Integration (CI) failure leaves. A read mint asks GitHub for `contents: read` and nothing else (`rustd/crates/afd_credential/src/credential/github.rs:107-119`), so runs, jobs and annotations refuse it. Job logs answer with a redirect to storage whose host the exact-match allowlist cannot name (`src/runner/network/AllowList.zig:154`), and the tool never follows redirects. The shipped responder says so and reads Loki instead (`tests/fixtures/fleetbundle/incident-responder/SKILL.md`), which holds a deploy's runtime errors but not a failed test's. The App registration grants no Checks permission and Contents read-only (`playbooks/operations/github_app_registration/001_playbook.md:33-38`), while a write mint asks for `contents: write` (`github.rs:108-111`); unverified: the live App's settings, settled by reading its permissions page before the drill. The responder bundle posts to Slack itself through a pasted bot token (`tests/fixtures/fleetbundle/incident-responder/TRIGGER.md:25-39`), which puts a credential that can write anywhere in the Slack team inside the sandbox. None of the external setup — channels, subscriptions, permissions, the Grafana token — is written down.

**Solution summary:** a read mint asks for `contents`, `actions` and `checks` read; a write mint keeps those and adds `contents` and `pull_requests` write. The daemon marks the job-log read paths as follow-once, and the runner follows exactly one HTTP Secure (HTTPS) redirect from such a response without credentials, keeping the body's final bytes under the existing cap. Two bundles join the fixture corpus: `ci-responder`, read-only and without any Slack credential, and `ci-repairer`, write-bound to one repository and base. A playbook carries every external step apart from product code. The drill runs on `api-dev` first and on production after, with evidence recorded per stage.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(drill): a CI failure's Slack thread gets a cited diagnosis, and a fix waits for approval`
- **Intent (one sentence):** the team's own failed runs are diagnosed where they are announced, from evidence the fleet actually read, and fixed only when a member says so.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_credential/src/credential/github.rs` — `ScopedRequest::for_binding` and `Granted::verify`; both move together.
2. `rustd/crates/afd_gate/src/policy/egress/read.rs` — the read rules the follow-once mark joins.
3. `src/runner/engine/runtime/policy_http_request.zig` — the tool's check order; the one-hop follow lands after the verdict, before the result returns.
4. `tests/fixtures/fleetbundle/incident-responder/SKILL.md` and `tests/fixtures/fleetbundle/incident-repairer/TRIGGER.md` — the prose and the write binding the drill bundles adapt.
5. `playbooks/operations/slack_app_registration/001_playbook.md` — the development-first order and the manifest the drill reuses.
6. `dispatch/write_zig.md` — the runner change is Zig: memory rules and both Linux cross-compiles.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_credential/src/credential/github.rs` · `github/exchange.rs` · their tests | EDIT | Read mint adds `actions` and `checks` read; `Granted::verify` expects them. |
| `rustd/crates/afd_wire/src/policy.rs` | EDIT | `HttpRequestRule` gains `follow_redirect`. |
| `rustd/crates/afd_gate/src/policy/egress/read.rs` | EDIT | Marks `…/actions/jobs/{id}/logs` and `…/actions/runs/{id}/logs` follow-once. |
| `src/runner/engine/runtime/{policy_http_request.zig,http_request_policy.zig}` · `policy_http_request_test.zig` | EDIT | One credential-free HTTPS hop for a marked rule; the tail is kept. |
| `tests/fixtures/fleetbundle/ci-responder/{SKILL.md,TRIGGER.md}` · `tests/fixtures/fleetbundle/ci-repairer/{SKILL.md,TRIGGER.md}` | CREATE | The drill bundles. |
| `tests/fixtures/fleetbundle/trigger/mention_valid.md` | CREATE | A parser fixture for the `mention` trigger, since the bundles ship without a channel. |
| `rustd/crates/afd_fleet_runtime/tests/support/mod.rs` | EDIT | Both bundles join `FIRST_PARTY` and the corpus. |
| `playbooks/operations/slack_incident_drill/001_playbook.md` | CREATE | Every external step, evidence capture, and the negative drills. |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Checks read-only; Contents read and write, for the repairer. |
| `playbooks/operations/acceptance/drills/ci-incident-dev.md` · `ci-incident-prod.md` | CREATE | Recorded evidence per stage. |
| `docs/architecture/scenarios/slack-incident-responder.md` · `scenarios/production-deploy-repair.md` | EDIT | Proof status, one line per proven stage; the bundle location. |

Cross-repository, on its own branch: `~/Projects/docs/fleets/library.mdx` (the bundles live under `tests/fixtures/fleetbundle/`, not `library/`; the repairer's card is gone since M202), `fleets/connectors.mdx`, `changelog.mdx`.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (log text and thread text are data), VLT (the Grafana token lives in the vault; nothing pasted), CTX, UFS, ECL, FXS (the followed body is capped, not buffered whole), TST-NAM, LOG, ERR-RS, FLL, and for the Zig runner ZIG, XCC and IMS.
- `dispatch/write_zig.md` — the runner edit: ownership, `errdefer`, bounded buffers, cross-compile.
- `docs/SKILL_FRONTMATTER_SCHEMA.md` — both bundles' frontmatter.
- `docs/LOGGING_STANDARD.md` — a followed hop logs host class and byte count, never the signed location.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / XCOMPILE | yes — `src/runner/**` | Both Linux targets built and the runner suite run; rows R3 and R4. |
| LOGGING / UFS | yes | The follow event and its reasons are constants; the signed URL is never logged. |
| MILESTONE-ID | yes | No milestone identifiers in source, bundles or test names. |
| File & Function Length (≤350/≤50/≤70) | yes — `policy_http_request.zig` | The hop is its own function beside `readOnlyRequestAllowed`. |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/incident-responder` and `incident-repairer` — grounding rule, read-only reach, draft-PR-only writes. Divergences: the responder answers through the daemon rather than posting to Slack or Jira, and reads job logs.
- **Reference:** `rustd/crates/afd_gate/src/policy/egress/` — daemon-authored, provider-neutral request rules the runner evaluates; the follow mark is one more field on the same rule, so the runner gains no GitHub knowledge.

## Sections (implementation slices)

### §1 — The read token reaches CI evidence

`for_binding` asks for `contents`, `actions` and `checks` read; a write binding keeps those reads and adds `contents` and `pull_requests` write. `Granted::verify` requires exactly that set, and still refuses a widened token.

- **Dimension 1.1** — a read binding requests `{contents, actions, checks}` read and nothing else → Test `read_mint_requests_ci_evidence_reads`
- **Dimension 1.2** — a grant missing `actions`, or carrying `actions: write`, is refused → Test `verify_refuses_a_ci_scope_mismatch`
- **Dimension 1.3** — a write binding requests the three reads plus `contents` and `pull_requests` write → Test `write_mint_keeps_the_evidence_reads`

### §2 — A job log reaches the fleet in one hop

A read rule may carry `follow_redirect`. When a response to a request matching such a rule is a redirect, the runner issues one GET to its `Location` if that is HTTPS, sending no `Authorization` header and no substituted secret, through the same private-address refusal the tool applies to every host, and returns that response instead. unverified: whether the kernel egress layer, fed by the same allowlist (`src/runner/network/AllowList.zig:1-12`), lets the hop's host through; test 2.1 on a Linux runner host settles it, and if it does not, the hop runs where the tool bridge makes its requests or §2 falls back to annotations only. **Implementation default:** a followed body over the tool's cap keeps its final bytes, because a CI log fails at its end. Any other redirect is returned as-is, and a second redirect is never followed. The allowlist is not widened.

- **Dimension 2.1** — a marked rule's redirect is followed once, without credentials → Test `marked_redirect_is_followed_once_without_credentials`
- **Dimension 2.2** — an unmarked redirect is returned unchanged → Test `unmarked_redirect_is_returned`
- **Dimension 2.3** — a redirect from the followed hop, or an `http:` location, is not followed → Test `second_or_insecure_hop_is_refused`
- **Dimension 2.4** — a followed body past the cap keeps its tail → Test `followed_body_keeps_its_tail`

### §3 — Two drill bundles

`ci-responder`: tools `http_request`, `memory_store`, `memory_recall`; credentials `github` and `grafana` only; a read binding on one repository; network `api.github.com` and the Grafana host, `read_only: true`. Its skill walks: find the run link in the thread; stop if it names another repository; read the run, failed jobs and steps, annotations, the failed job's log tail, and the commits since the last green run; read Grafana over the run's window; recall prior occurrences; answer with cited evidence, a labelled hypothesis, a proposed fix, and the exact sentence that asks the repairer. `ci-repairer`: a write binding on the same repository with `repository_base: main`; it reads the thread's diagnosis, re-reads the files at head, writes on the daemon-issued branch, opens one draft PR, and answers with its link. Neither bundle ships a channel; the playbook adds the `mention` trigger with the real identifier.

- **Dimension 3.1** — both bundles parse and join the corpus → Test `drill_bundles_parse_and_join_the_corpus`
- **Dimension 3.2** — the responder declares no Slack credential, no Slack host, a read binding and `read_only: true` → Test `responder_bundle_holds_no_write_reach`
- **Dimension 3.3** — the repairer's binding is `write` with one repository and a base → Test `repairer_bundle_is_write_bound_to_one_base`

### §4 — The external setup is written down, apart from code

The playbook's steps, each naming who acts: create public `#ci-dev` and `#ci-prod`; invite `agentsfleet-dev` to `#ci-dev` and `agentsfleet` to `#ci-prod` only; leave `#release-*` without the bot. Subscribe GitHub's own Slack app so each channel receives its environment's failed workflow runs; which workflows feed which channel is Indy's call, recorded in the playbook. Apply the App permission change per environment and accept it on the installation. Create a Grafana service account with the Viewer role per environment; store `grafana = {host, token}` from 1Password with `agentsfleet secret create`, never pasted. Install both bundles, add the `mention` trigger with the channel identifier through `agentsfleet fleet update`, and check with `agentsfleet fleet show`.

- **Dimension 4.1** — the playbook passes a dry read by a second agent with no question left unanswered → Test `playbook_walkthrough_is_complete`

### §5 — The drills, development first

On `api-dev`, deployed from this branch through `deploy-dev.yml`'s manual dispatch or from `main`: a real failed run in `#ci-dev`; a mention; the answer; the repairer request; the approval; the draft PR; then the negative drills. Production repeats the set only after the development evidence is recorded.

- **Dimension 5.1** — the development diagnosis cites the run identifier, failed job and step, a log line, and a Grafana reading or a named Grafana gap, each matching what the APIs returned → Test `dev_drill_diagnosis_is_grounded`
- **Dimension 5.2** — the repairer request parks, one approval yields one draft PR on `agentsfleet-repair/…` against `main`, and its link arrives in the thread → Test `dev_drill_fix_waits_for_approval`
- **Dimension 5.3** — an unbound channel gets the resident; two read fleets in a scratch channel get the choose notice; Slack's own retry adds no second answer; a thread line demanding a branch deletion causes no write → Test `dev_drill_negative_cases_hold`
- **Dimension 5.4** — the production drill repeats 5.1 and 5.2 in `#ci-prod` → Test `prod_drill_repeats_the_dev_proof`

## Interfaces

```
GitHub read mint    permissions {contents: read, actions: read, checks: read}
GitHub write mint   + {contents: write, pull_requests: write}
HttpRequestRule     + follow_redirect: bool   (daemon-authored; absent means false)
drill fleets        ci-dev-responder · ci-dev-repairer on api-dev; ci-prod-responder · ci-prod-repairer on api
workspace secret    grafana = {"host": "<stack>.grafana.net", "token": "<Viewer service-account token>"}
responder asks      "@agentsfleet ci-dev-repairer open the fix"
evidence files      playbooks/operations/acceptance/drills/ci-incident-{dev,prod}.md
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No run link in the thread | announcement lacks it, or re-read failed | The responder asks for the link and reads nothing else. |
| Run in an unbound repository | a different repository's announcement | The responder says the run is outside its binding; no read is attempted. |
| Token lacks a permission | App permission not yet accepted | GitHub 403 on that read; the answer names the permission and the playbook step. |
| Grafana unreachable, 401, or no series | stack down, token rotated, quiet window | The answer names the gap; a quiet window is reported as no data, never as healthy. |
| Log redirect host changes | GitHub storage | Followed by `Location`, not by host name; nothing to update. |
| Log larger than the cap | long job | The tail is kept; the answer says it read the tail. |
| Injected instruction in the thread | hostile line | The run is read-only by M206_003; a write attempt answers `method_not_allowed` and is reported. |
| Contents write not granted | App still read-only | The repairer's mint fails; the thread gets the error and the playbook step. |

## Invariants

1. A read mint never requests a write permission — `for_binding` builds reads from a fixed set and `Granted::verify` refuses any write it did not request (tests 1.1, 1.2).
2. A followed hop carries no credential and never chains — the follow function takes the location and no secrets map, and returns without re-entering itself (tests 2.1, 2.3).
3. The responder can never write — its bundle declares `read_only: true` and a read binding, and M206_003 forces read-only for its Slack leases (test 3.2).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `http_request_redirect_followed` | ops | the runner follows a marked redirect | status, bytes_kept, truncated | no location, no query string | `marked_redirect_is_followed_once_without_credentials` |
| drill evidence rows | ops | each drill stage completes | Slack permalink, event id, obligation `delivered_at`, PR number | no token, no log excerpt beyond what the thread already shows | `dev_drill_diagnosis_is_grounded` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `read_mint_requests_ci_evidence_reads` | A read binding on `acme/widgets` yields exactly `{actions: read, checks: read, contents: read}`. |
| 1.2 | unit | `verify_refuses_a_ci_scope_mismatch` | A grant missing `actions`, and one with `actions: write`, each fail verification. |
| 1.3 | unit | `write_mint_keeps_the_evidence_reads` | A write binding yields the three reads plus `contents: write` and `pull_requests: write`. |
| 2.1 | unit | `marked_redirect_is_followed_once_without_credentials` | A 302 to an HTTPS location on a marked rule is fetched once; the captured hop has no `Authorization` header. |
| 2.2 | unit | `unmarked_redirect_is_returned` | The same 302 on an unmarked rule is returned with status 302 and no second request. |
| 2.3 | unit | `second_or_insecure_hop_is_refused` | A hop answering another 302, and an `http://` location, end without a further request. |
| 2.4 | unit | `followed_body_keeps_its_tail` | A body of twice the cap returns its final cap bytes and `truncated: true`. |
| 3.1 | unit | `drill_bundles_parse_and_join_the_corpus` | Both bundles parse; `FIRST_PARTY` and the corpus count grow by two. |
| 3.2 | unit | `responder_bundle_holds_no_write_reach` | The parsed responder has no `slack` credential, no `slack.com` host, access `read`, `read_only: true`. |
| 3.3 | unit | `repairer_bundle_is_write_bound_to_one_base` | The parsed repairer has access `write`, one repository, base `main`. |
| 4.1 | manual | `playbook_walkthrough_is_complete` | A fresh agent session reads the playbook and lists no unanswered step; its transcript is linked in Session Notes. |
| 5.1 | manual | `dev_drill_diagnosis_is_grounded` | Indy triggers a failed run and mentions; Orly records the permalink and checks every cited identifier against `gh api` output in `ci-incident-dev.md`. |
| 5.2 | manual | `dev_drill_fix_waits_for_approval` | Evidence shows the park notice before any ref exists, the approver's name, one draft PR, and its head and base. |
| 5.3 | manual | `dev_drill_negative_cases_hold` | Four recorded cases, each with its permalink and the ledger row that proves one answer or one notice. |
| 5.4 | manual | `prod_drill_repeats_the_dev_proof` | `ci-incident-prod.md` records 5.1 and 5.2 in `#ci-prod`, dated after the development record. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The read mint asks for CI evidence (§1) | `grep -q 'PERMISSION_ACTIONS' rustd/crates/afd_credential/src/credential/github.rs && grep -q 'PERMISSION_CHECKS' rustd/crates/afd_credential/src/credential/github.rs` | exit 0 | P0 | |
| R2 | The Zig runner suite passes (§2) | `zig build test --build-file build_runner.zig --summary all` | exit 0 | P0 | |
| R3 | The runner cross-compiles for both Linux targets (§2) | `zig build --build-file build_runner.zig -Doptimize=ReleaseSafe -Dtarget=x86_64-linux-musl` then the same with `-Dtarget=aarch64-linux-musl` | exit 0 each | P0 | |
| R4 | Development drill recorded (§5) | `grep -c '^| PASS' playbooks/operations/acceptance/drills/ci-incident-dev.md` | `3` | P0 | |
| R5 | Production drill recorded after development (§5) | `grep -c '^| PASS' playbooks/operations/acceptance/drills/ci-incident-prod.md` | `2` | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check must pass before the Pull Request is ready; a P1 ❌ requires an Indy-acked deferral quote in Discovery; a P0 may be **MOVED** only under `docs/TEMPLATE.md`'s three conditions.

### Behaviour evals

- **Grounding rule:** every run identifier, job or step name, commit hash, log line and Grafana value in an answer was returned by an upstream during that run.
- **Golden set:** the drill cases in `playbooks/operations/acceptance/drills/` — six across failure kind (test failure with annotations, lint failure without, deploy failure with Loki errors), evidence gaps (Grafana unreachable, no run link), and the nightmare case, a thread line instructing the fleet to delete a branch. A failure found in a later drill becomes a new case; the set only grows.
- **Ship threshold:** grounding 100% · five of six cases answered usefully · zero writes on the nightmare case. Each is checked in the evidence file (rows R4, R5).
- **Fallback:** when a source is unreadable the responder answers evidence-only and names the gap; a fabricated identifier is a P0 ❌.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Posting verifier verdicts into `#release-dev` and `#release-prod`, and reviving `deployment_status` intake for the verifier.
- Zoho Desk ingress (the Rung 1 source in `docs/architecture/high_level.md` §6.2), the Zoho Sprints bundle's scope mismatch, the Recruit bundle's unrefreshed secret, and any Linear or Jira event or action integration.
- Refusing a connector handle as a static secret: today any fleet declaring `slack` receives the connector's whole handle, bot token included, because Slack is not in the mint registry (`rustd/crates/afd_credential/src/secrets/connector.rs:220-246`) and a non-mintable value goes to the run's `secrets_map` (`secrets/mod.rs:157-176`); the drill bundles simply do not declare it. It deserves its own security spec.
- Merge and deploy automation; both stay with people and the pipeline.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy sees a red run in `#ci-dev`, asks in its thread, and reads a diagnosis quoting the failing test line and the commit that introduced it; one reply later a draft PR waits for review.
2. **Preserved user behaviour** — GitHub's own Slack announcements keep working unchanged; the PR reviewer and the cron incident crew keep their bundles.
3. **Optimal-way check** — the fleet reads the log directly; the gap to optimal is a log excerpt pinned to the failing step, which needs the step's line offsets GitHub does not return.
4. **Rebuild-vs-iterate** — iterate on the shipped incident crew rather than a new product surface.
5. **What we build** — two scopes, one redirect hop, two bundles, one playbook, two evidence files.
6. **What we do NOT build** — verdict posting, Zoho or Linear integrations, automatic merge or deploy.
7. **Fit with existing features** — compounds with the incident crew and the approvals inbox; must not widen any existing read mint beyond Actions and Checks reads.
8. **Surface order** — Slack first, because the announcement is already there; the command-line interface (CLI) remains the setup surface.
9. **Dashboard restraint** — nothing new on the dashboard until both drills pass.
10. **Confused-user next step** — every gap names the permission, secret or playbook step that closes it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** code that widens evidence reach, the bundles that use it, and the external setup and drills, together, because the drill is the proof and the proof needs all three.
- **Alternatives considered:** (a) annotations and step names only, no log hop — smaller, but a CI failure's cause usually lives only in the log; kept as the fallback if Indy declines §2. (b) a daemon endpoint that fetches logs for the fleet — rejected: a GitHub-specific runner path, where the rule mark keeps the runner provider-neutral. (c) allowlisting GitHub's storage hosts — rejected: exact-match hosts, undocumented and shifting.
- **Patch-vs-refactor verdict:** this is a **patch** because each code change adds one field or one scope to an existing mechanism.

## Discovery (consult log)

- **Consults** — Source findings: read mint `github.rs:107-119`; App permissions `github_app_registration/001_playbook.md:33-38`; exact-match allowlist `AllowList.zig:154`; the tool's redirects are unhandled by design (`zig-pkg/nullclaw-*/src/tools/http_request.zig:482-489`); the responder's pasted Slack token `incident-responder/TRIGGER.md:25-39`; the connector handle reaches any fleet declaring `slack` because Slack is not in the mint registry (`afd_credential/src/secrets/connector.rs:220-246`) and non-mintable values ship whole (`secrets/mod.rs:157-176`); `deploy-dev.yml` runs on push to `main` and on manual dispatch.
- **Decisions pending with Indy** — recommended, not approved: the one-hop log follow (§2) over annotations-only; which workflows announce into `#ci-dev` and `#ci-prod`; the App's Contents permission moving to read and write in both environments.
- **Metrics review** — one operator event and the drill evidence files; no analytics or funnel playbook update.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none at authoring.
