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

# M206_003: A Slack-requested fleet reads its failed run's evidence over HTTP, and a write fleet opens at most one draft PR

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 003
**Date:** Sep 23, 2026
**Status:** DONE
**Priority:** P0 — the drill's diagnosis needs Continuous Integration (CI) evidence no fleet token can read today, and under the Claude Tag authority model the write fleet's reach is the only boundary a channel member's request meets.
**Categories:** API
**Batch:** B1 for §1, §2, and Dimensions 3.1–3.2, which need no other workstream; Dimension 3.3 after M206_002 admits a mention. Ships in M206_002's Pull Request.
**Branch:** feat/m206-slack-incident-responder
**Folded-into:** `M206_001`
**Baseline revision:** f3edd3c17062087a6db7f7e271606bf0c3901259
**Test Baseline:** `unit=2662 integration=572` at the comparison revision, inherited from `M206_001` — one branch, measured once.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M206_001-f3edd3c17.md`
**Depends on:** M206_002 — Dimension 3.3 delivers a signed mention twice.
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4`; re-cut Sep 24, 2026 at `f3edd3c17` after the owner decisions quoted in Discovery
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §7–§8

---

## Overview

**Goal (testable):** a read-bound fleet's GitHub token reaches its repository's runs, jobs, job-log requests and check-run annotations, and the daemon's read rules admit those paths for the bound repository and nothing beside it, so the fleet reads a failed run over `http_request` as its SKILL.md directs; a write-bound fleet attached to the channel turns one request into at most one daemon-issued branch and one draft Pull Request (PR), and has no path to merge, mark ready, update another ref, or change a workflow.

**Problem:** the responder cannot see why a run failed. A read mint asks GitHub for `contents: read` and nothing else (`rustd/crates/afd_credential/src/credential/github.rs:107-119`), so runs, jobs and annotations refuse the token. Those permission names are `&'static str` constants keyed into a map (`github.rs:47-53,101`), so each new permission is one more string a typo can miss. Separately, Indy chose Claude Tag's authority model: attaching a write fleet to a channel authorises draft PRs, with no per-request approval. That leaves the write fleet's own reach as the only boundary between any channel member's words and the repository, and nothing proves that reach for a Slack-requested lease.

**Solution summary:** a read mint asks for `contents` read plus whichever of `actions` and `checks` read the installation holds, and a write mint keeps those and adds `contents` and `pull_requests` write; the request side names permissions with an enum. No daemon evidence step: the fleet reads the run itself, the same way fleets read repository files today, under the read rule the daemon already issues — GET and HEAD under `/repos/{repository}/` on `api.github.com` (`rustd/crates/afd_gate/src/policy/egress/read.rs:19-27`). §2 pins that the rule covers the CI paths and stops at the repository. The write boundary needs no new gate: the existing rules (`afd_gate/src/policy/egress/write.rs:54-91`) admit Git objects, the one daemon-issued ref and a draft PR against the trusted base, and the runner denies anything else on a ruled host (`src/runner/engine/runtime/http_request_policy.zig:22-30`). §3 proves that for Slack-requested leases. The job log's second hop, to GitHub's storage, is held by the owner (Out of Scope).

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(evidence): a fleet's GitHub token reads CI evidence, and a Slack repair stays one draft PR`
- **Intent (one sentence):** the fleet can read why its run failed, and a request typed in Slack can at most propose a fix as one draft PR.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_credential/src/credential/github.rs` — `ScopedRequest::for_binding` and `Granted::verify`; both move together.
2. `rustd/crates/afd_gate/src/policy/egress/read.rs` — the trailing-slash prefix §2 proves.
3. `rustd/crates/afd_gate/src/policy/egress/write.rs` — the rules §3 proves.
4. `src/runner/engine/runtime/http_request_policy.zig` — how the runner matches a rule: exact or prefix path, unmatched on a ruled host denied.
5. `dispatch/write_rust.md` §"Functional design (RULE FN-RS)".

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_credential/src/credential/github.rs` · `github/exchange.rs` · `github/tests.rs` · `github/tests/transport.rs` | EDIT | Permission names become an enum; the exchange reads the installation first, and the read mint adds `actions` and `checks` read where it holds them; `Granted::verify` expects exactly the request. |
| `rustd/crates/afd_credential/src/credential/github/request.rs` · `github/tests/request.rs` | CREATE | The request side, split out of `github.rs` at the length cap, and its cases. |
| `rustd/crates/afd_credential/Cargo.toml` · `rustd/Cargo.lock` | EDIT | `axum` as a dev-dependency: a loopback GitHub that routes both calls and parses the token request. |
| `rustd/crates/afd_gate/src/policy/egress/read.rs` | EDIT | A test pins that the prefix covers the CI evidence paths and no neighbour. |
| `rustd/crates/afd_gate/src/policy/egress/tests.rs` · `egress/tests/slack.rs` | CREATE | The binding proofs moved out of `mod.rs`, and the write-reach proofs for a Slack-requested lease. |
| `rustd/crates/afd_fleet/tests/integration_lease_gates.rs` · `integration_lease_gates/slack.rs` | EDIT · CREATE | One Slack request is one lease: the ledger's key, the stream backlog, the branch. |
| `rustd/crates/afd_gate/src/policy/egress/mod.rs` | EDIT | Declares the test module. |
| `docs/architecture/scenarios/slack-incident-responder.md` | EDIT | §8 and §10 statuses when this ships. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (run and annotation text is data), UFS, TST-NAM, ERR-RS, FLL, PSR.
- `dispatch/write_rust.md` RULE FN-RS with `M-STRONG-TYPES` — the request's permission names are a closed `serde` enum, so an unlisted permission cannot be requested because it cannot be named; the response side keeps a string map, because `Granted::verify` must see a name it does not know in order to refuse it (`github.rs:167,243-260`). RULE PSR — no hand-rolled helper where the workspace or a `[workspace.dependencies]` crate has one.
- `docs/RUST_ERROR_STANDARD.md` — any changed fallible signature.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS | yes | Permission names are enum variants with one `serde` spelling each. |
| MILESTONE-ID | yes | No milestone identifiers in source or test names. |
| File & Function Length (≤350/≤50/≤70) | yes | The write-reach proofs live in their own `egress/tests.rs`. |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/incident-repairer/SKILL.md:57` — a fleet reading repository files over `http_request` under the read prefix; the CI reads are the same shape on other paths.
- **Reference:** Claude Tag's bug-to-PR flow (`claude.com/docs/claude-tag/users/use-cases/fix-bugs.md`) — a draft PR with no approval step once GitHub is attached to the channel; §3 is the same authority, with the reach pinned by rules the runner enforces.

## Sections (implementation slices)

### §1 — The read token reaches CI evidence

The request side names permissions with `GithubPermission { Actions, Checks, Contents, PullRequests }`, serialised `snake_case`. The exchange first reads the installation (`GET /app/installations/{id}`), because a token cannot carry a permission its App was never granted. `for_binding` asks for `contents` read, and `actions` and `checks` read where the installation holds them; a write binding keeps those reads and adds `contents` and `pull_requests` write whether held or not, so a repair the installation cannot make fails at the mint. `Granted::verify` requires exactly the request and still refuses a widened token.

- **Dimension 1.1** DONE — a read binding on an installation holding the evidence reads requests `{contents, actions, checks}` read and nothing else → Test `read_mint_requests_ci_evidence_reads`
- **Dimension 1.2** DONE — a grant missing `actions`, or carrying `actions: write` or `workflows`, is refused → Test `verify_refuses_a_ci_scope_mismatch`
- **Dimension 1.3** DONE — a write binding requests the three reads plus `contents` and `pull_requests` write → Test `write_mint_keeps_the_evidence_reads`
- **Dimension 1.4** DONE — an evidence read the installation does not hold, or holds at a level the daemon does not model, is left out of the request → Test `evidence_reads_follow_what_the_installation_holds`
- **Dimension 1.5** DONE — a write binding's own reach is asked for even where the installation lacks it, so the repair fails at the mint rather than narrowing → Test `write_reach_is_asked_for_where_the_installation_lacks_it`
- **Dimension 1.6** DONE — against an installation holding no Checks, the token request leaves `checks` out and the mint succeeds → Test `an_installation_without_checks_mints_without_asking_for_them`
- **Dimension 1.7** DONE — a failed installation read keeps its retry posture and no token is asked for → Test `an_installation_read_failure_keeps_its_retry_posture`

### §2 — The fleet reads the run over HTTP

No new daemon step and no runner change. The read rule is a GET and HEAD prefix on `/repos/{repository}/`, which the runner matches with `startsWith` (`http_request_policy.zig:90-95`). The test runs the CI paths the ci-responder's SKILL.md reads (M206_004 §1) against that prefix, and a neighbouring repository's same paths against it, so a future change to the rule that drops a CI path or widens past the slash fails here.

- **Dimension 2.1** DONE — for `acme/widgets`, GET `/actions/runs/123`, `/actions/runs/123/jobs`, `/actions/jobs/456`, `/actions/jobs/456/logs` and `/check-runs/789/annotations` under `/repos/acme/widgets/` are admitted; the same paths under `/repos/acme/widgets-private/` and `/repos/other/repo/` are not → Test `read_rules_cover_ci_evidence_paths`

### §3 — A write fleet reached from Slack opens at most one draft PR

No approval step: attaching a write fleet to the channel is the authority (Discovery). The lease for a Slack-requested event on a write-bound fleet carries the fleet's own rules, and the proofs pin their reach: one ref, `refs/heads/agentsfleet-repair/<base64 of the event id>` (`afd_gate/src/policy/repair.rs:44-68`); a pull request only with that head, the trusted base and `draft: true`; no PUT, PATCH or DELETE rule and no `/graphql` path; a token with no `workflows` permission. One Slack event is one event (M206_002's producer key), so one request is one branch.

- **Dimension 3.1** DONE — the assembled rules admit a ref only for the event's branch and a pull request only as a draft from it against the trusted base → Test `slack_write_lease_admits_one_branch_and_one_draft`
- **Dimension 3.2** DONE — no rule admits PUT, PATCH, DELETE or a `/graphql` path, so a merge, a ready-for-review, a ref update or a deletion is denied → Test `slack_write_lease_has_no_merge_or_ref_update_path`
- **Dimension 3.3** DONE — a Slack retry of one request yields one lease and one branch name; a second request yields a different branch → Test `one_slack_request_is_one_attempt`

## Interfaces

```
permission names     enum GithubPermission { Actions, Checks, Contents, PullRequests }  (serde snake_case)
installation read    GET /app/installations/{id} → the permissions the installation holds
GitHub read mint     {contents: read} + {actions, checks: read} where held
GitHub write mint    + {contents: write, pull_requests: write}; never workflows
fleet's CI reads     GET under /repos/{repository}/ on api.github.com:
                     actions/runs/{run_id} · actions/runs/{run_id}/jobs · actions/jobs/{job_id}
                     actions/jobs/{job_id}/logs (answers 302; the hop is owner-held) · check-runs/{id}/annotations
write reach          POST /repos/{r}/git/{blobs,trees,commits} · POST /repos/{r}/git/refs (ref locked)
                     · POST /repos/{r}/pulls (head, base locked; draft true) — nothing else on api.github.com
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| App lacks Actions or Checks | permission not accepted on the installation | The mint leaves that read out and still succeeds; the fleet's GET answers 403 and its skill names the missing permission (M206_004 playbook step). |
| Installation read fails | App uninstalled, GitHub down, unreadable body | Classified as the token request is: 401/404 → reconnect, 5xx or unreadable → retry; no token is asked for. |
| Log request answers 302 | GitHub serves logs from storage | The tool reports `HTTP 302` with no location (`http_request.zig:173-190`); the skill answers from jobs, steps and annotations and says the log was not read. |
| Run in another repository | thread links elsewhere | The path misses the prefix; the runner answers `request_policy_not_allowed` (`policy_http_request.zig:127-134`). |
| Injected instructions in an annotation | hostile test output | Data; the fleet's policy and rules are unchanged. |
| A second "open the fix" | a person asks twice | A second event and a second draft PR, both visible in the thread; neither can merge. |
| A repair branch's CI reads secrets | same-repository branch | Handled outside product code: the M206_004 playbook keeps deploy secrets off `agentsfleet-repair/*`. |

## Invariants

1. A read mint never requests a write permission — `for_binding` builds from a fixed set of enum variants and `Granted::verify` refuses anything it did not ask for (tests 1.1, 1.2).
2. CI evidence is read only inside the bound repository — the only read rule is the trailing-slash prefix (test 2.1).
3. A Slack-requested write lease reaches one ref and one draft PR — the rules are the whole allowlist for `api.github.com`, and the runner denies unmatched requests there (tests 3.1, 3.2).

## Metrics & Observability

N/A — no new metric or event: the fleet's reads already appear as its tool-call activity frames (`src/runner/engine/runner_progress.zig:40`), and the mint's scope change is visible in the existing mint path.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `read_mint_requests_ci_evidence_reads` | A read binding on `acme/widgets` yields exactly `{actions: read, checks: read, contents: read}`. |
| 1.2 | unit | `verify_refuses_a_ci_scope_mismatch` | Grants missing `actions`, with `actions: write`, or with `workflows: write` each fail verification. |
| 1.3 | unit | `write_mint_keeps_the_evidence_reads` | A write binding yields the three reads plus `contents: write` and `pull_requests: write`. |
| 1.4 | unit | `evidence_reads_follow_what_the_installation_holds` | No `checks` held → `{actions, contents}` read; neither → `{contents}`; `checks: maintain` → left out; evidence held at write → asked at read. |
| 1.5 | unit | `write_reach_is_asked_for_where_the_installation_lacks_it` | A write binding on a `contents: read` installation asks `{contents: write, pull_requests: write}`. |
| 1.6 | unit | `an_installation_without_checks_mints_without_asking_for_them` | A loopback GitHub holding no Checks receives a token request for `{actions, contents}` read, and the mint succeeds. |
| 1.7 | unit | `an_installation_read_failure_keeps_its_retry_posture` | An installation read answering 401 or 404 reconnects, 503 or `not-json` retries, and no token request arrives. |
| 2.1 | unit | `read_rules_cover_ci_evidence_paths` | The five CI paths under `/repos/acme/widgets/` start with the rule's prefix; under `/repos/acme/widgets-private/` and `/repos/other/repo/` none does; the rule's methods are GET and HEAD only. |
| 3.1 | unit | `slack_write_lease_admits_one_branch_and_one_draft` | For event `1700000000000-7` on a write binding, the only ref rule locks `refs/heads/agentsfleet-repair/<its base64>`, and the only pull rule locks head, base `main`, and `draft: true`. |
| 3.2 | unit | `slack_write_lease_has_no_merge_or_ref_update_path` | The assembled rules contain no PUT, PATCH or DELETE method and no path ending `/graphql`, `/merge` or `/pulls/{n}`. |
| 3.3 | integration | `one_slack_request_is_one_attempt` | The same Slack mention admitted twice leaves one stream entry and one lease, with no approval asked, on the branch named for that event; a second mention is a second entry whose branch differs. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Permission names are an enum (§1) | `grep -c 'enum GithubPermission' rustd/crates/afd_credential/src/credential/github.rs` | `1` | P0 | |
| R2 | No write rule offers a method beyond POST (§3) | `grep -cE 'HttpMethod::(Put\|Patch\|Delete)' rustd/crates/afd_gate/src/policy/egress/write.rs` | `0` | P0 | |
| R3 | The CI path proof exists (§2) | `grep -c 'fn read_rules_cover_ci_evidence_paths' rustd/crates/afd_gate/src/policy/egress/read.rs` | `1` | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R5 | Patch coverage meets the repository bar | `gh pr checks --json name,state --jq '.[] \| select(.name\|startswith("codecov/patch")) \| .state'` | every line `SUCCESS` — `rust-afd` at 99% of added lines (`codecov.yml`, threshold 0%) | P0 | |
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

## Dead Code Sweep

`grep -nE 'const PERMISSION_(CONTENTS|PULL_REQUESTS)' rustd/crates/afd_credential/src/credential/github.rs` → 0 matches once the enum replaces the constants.

## Out of Scope

- **The job log's storage hop — owner-held.** A log request answers 302 to a signed storage URL. `http_request` shells out to `curl` without `-L` and returns status and body only (`zig-pkg/nullclaw-*/src/tools/http_request.zig:173-190,250-311`), so the fleet never sees the location; and the storage host is an unenumerated `*.blob.core.windows.net` account (GitHub's self-hosted runner reference lists only the wildcard; twelve sampled jobs of this repository redirected to `productionresultssa{0,2,3,10,12,13,14,16}`), while the host check is exact-match (`policy_http_request.zig:109-110,167-172`). Indy holds the approach (Discovery).
- A daemon step that reads and attaches evidence, and a runner redirect or wildcard host — superseded by the owner's choice that the fleet reads over HTTP.
- A per-request approval for Slack-requested writes, dropped by owner decision (Discovery).
- An approved event gate possibly running twice (`afd_gate/src/gate/pass.rs:162-163` with `afd_approval/src/inbox/resolve.rs:154-155`); off this path now that Slack requests do not park, and owed its own spec.
- A connector handle resolving as a static secret (`afd_credential/src/secrets/connector.rs:220-246`, `secrets/mod.rs:157-176`); owed its own security spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the responder's answer names the failed job, the failing step and the annotation's assertion; one more message later, a draft PR link sits in the thread.
2. **Preserved user behaviour** — GitHub-triggered and steered runs lease exactly as today; the PR reviewer's token gains two read scopes and nothing else.
3. **Optimal-way check** — the fleet reads under a rule it already holds, with no new daemon or runner machinery. The gap to optimal is the log text itself, behind the owner-held storage hop.
4. **Rebuild-vs-iterate** — iterate: one mint scope change, one proof over an existing rule, proofs over write rules that already exist.
5. **What we build** — two read scopes as enum variants, the CI path proof, the write-reach proofs.
6. **What we do NOT build** — a daemon evidence step, a runner redirect, a wildcard host, a per-request approval.
7. **Fit with existing features** — compounds with the grant and the M157 repair rules; must not widen any token beyond Actions and Checks reads.
8. **Surface order** — N/A — no new user surface; the evidence appears in the fleet's answer.
9. **Dashboard restraint** — N/A — nothing new on the dashboard.
10. **Confused-user next step** — every refusal names what closed the door: the missing permission, the repository outside the binding, or the unread log.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** token reach and write reach together, because both decide what a Slack request can see and do, and both touch the GitHub credential path.
- **Alternatives considered:** (a) `agentsfleetd` reads and attaches the run — superseded: the owner chose that the fleet reads. (b) the sandbox admits the storage host mid-run — the kernel egress layer it would extend is not enforced in production (`src/runner/child_supervisor.zig:133-140`, `src/runner/capability_probe.zig:19`), and the Zig tree has no test lane (`make/test.mk:19`) and no Codecov measure (`codecov.yml` ignores `src/**`). (c) a per-request approval park — dropped by owner decision.
- **Patch-vs-refactor verdict:** this is a **patch** because each change adds one type, one scope or one test to an existing mechanism.

## Discovery (consult log)

- **Consults** — the architecture scenario §7 matches the owner's authority model; `connectors.md` trust anchor 6 needs no exception, since an installed grant still covers the write. Source findings: read mint `github.rs:107-119`; read rule `egress/read.rs:19-27`; write rules `egress/write.rs:54-91`; unmatched requests on a ruled host denied, `http_request_policy.zig:22-30`; `http_request` returns no response headers, `http_request.zig:173-190`.
- **Owner decisions** —
  > Indy (2026-09-23): "Like Claude Tag" — context: asked whether a draft fix PR waits for a member's approval or opens automatically; attaching a write fleet to a channel authorises drafts, and merge and deploy stay human.
  > Indy (2026-09-23): "Runner reads" — context: asked whether `agentsfleetd` or the runner reads the job logs.
  > Indy (2026-09-23): "Fleet fetches mid-run" — context: asked which part of the runner reads them.
  > Indy (2026-09-24): "well we will the http way like you do for the files, via the SKILL.md " — context: asked which process fetches; the fleet reads over `http_request` as its SKILL.md directs, as fleets read repository files.
  > Indy (2026-09-24): "this is not a big deal, i think we shouldnt make a holy grail for this " but the runner's allowlist doesn't support wildcards" and move on toe th enext, indy will find a way or simple way." — context: the log's storage hop; held by the owner.
- **Metrics review** — no new events; no analytics or funnel playbook update.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — the job log's storage hop, owner-held per the quote above.
- **GitHub App permissions (Sep 24, 2026)** — the platform App registration grants no Checks permission (`playbooks/operations/github_app_registration/001_playbook.md:33-38`), and GitHub's token documentation says "The installation access token cannot be granted permissions that the app was not granted." `Granted::verify` refuses a token narrower than its request, so asking for Checks unconditionally failed every mint on every fleet. Options put: (A) ask only for what the installation holds, (B) add Checks: read to the App and have every installation accept before deploy, (C) drop Checks.
  > Indy (2026-09-24): "A" — context: chose to read the installation's permissions at mint and ask for evidence reads only where held.
- **Resolved on `main` by M206_004** — a write mint has asked for `contents: write` since before this milestone (`f3edd3c17` `github.rs:108-110`), and the registration playbook granted Contents read-only. The playbook now lists Contents read and write and Checks read-only, each installation accepting the change before the drill (`playbooks/operations/github_app_registration/001_playbook.md:35,38,41`).
