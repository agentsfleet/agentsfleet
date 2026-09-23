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

# M206_003: A Slack-requested run gets its failed run's evidence, including job logs, and a write fleet opens at most one draft PR

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 003
**Date:** Sep 23, 2026
**Status:** PENDING
**Priority:** P0 — the drill's diagnosis needs Continuous Integration (CI) evidence no fleet can read today, and under the Claude Tag authority model the write fleet's reach is the only boundary a channel member's request meets.
**Categories:** API
**Batch:** B1 for §1 and §3, which need no other workstream; §2 in B2, after M206_002 composes the message it reads links from. Ships in M206_002's Pull Request.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M206_002 — §2 reads GitHub run links from the `message` that producer composes.
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4`; the owner decisions are quoted in Discovery
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §7–§8

---

## Overview

**Goal (testable):** a lease for a Slack-requested event whose thread links a failed GitHub Actions run in the fleet's bound repository carries that run's failed jobs, steps and annotations and each failed job's log tail, read by `agentsfleetd`; a write-bound fleet attached to the channel turns one request into at most one daemon-issued branch and one draft Pull Request (PR), and has no path to merge, mark ready, update another ref, or change a workflow.

**Problem:** the responder cannot see why a run failed. A read mint asks GitHub for `contents: read` and nothing else (`rustd/crates/afd_credential/src/credential/github.rs:107-119`), so runs, jobs and annotations refuse it. A job's log answers with a redirect to storage whose host the runner's exact-match allowlist cannot name (`src/runner/network/AllowList.zig:154`), and the tool never follows a redirect (`zig-pkg/nullclaw-*/src/tools/http_request.zig:482-489`); the shipped responder reads Loki instead, which holds a deploy's errors but not a failed test's. Separately, Indy chose Claude Tag's authority model: attaching a write fleet to a channel authorises draft PRs, with no per-request approval. That leaves the write fleet's own reach as the only boundary between any channel member's words and the repository, and nothing proves that reach for a Slack-requested lease.

**Solution summary:** a read mint asks for `contents`, `actions` and `checks` read, and a write mint keeps those and adds `contents` and `pull_requests` write. When a lease is assembled for a `slack:` chat event, `agentsfleetd` finds the GitHub Actions run links in its message whose repository is in the fleet's read binding, mints a read token under the lease's own grant, and reads each run, its failed jobs and steps, their annotations, and each failed job's log. Automatic redirects are off and one validated HTTPS hop is taken without credentials, the shape `rustd/crates/afd_library/src/github.rs:94,194` already uses for tarballs. The results are appended to the lease's message under a fixed heading, within fixed caps and a deadline, and a failed read becomes a named gap. The write boundary needs no new gate: the existing rules (`afd_gate/src/policy/egress/write.rs:54-91`) admit Git objects, the one daemon-issued ref and a draft PR against the trusted base, and the runner denies anything else on a ruled host (`src/runner/engine/runtime/http_request_policy.zig:22-30`). §3 proves that for Slack-requested leases.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(evidence): the daemon reads the failed run for the fleet, and a Slack repair stays one draft PR`
- **Intent (one sentence):** the fleet sees the failing log line, and a request typed in Slack can at most propose a fix as one draft PR.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_credential/src/credential/github.rs` — `ScopedRequest::for_binding` and `Granted::verify`; both move together.
2. `rustd/crates/afd_library/src/github.rs` — redirects off, one validated hop; the evidence read mirrors it.
3. `rustd/crates/afd_fleet/src/lease/mint.rs` — how a lease's scope, grant, handle and broker produce a token; the evidence read uses the same path, never a wider one.
4. `rustd/crates/afd_gate/src/policy/egress/write.rs` — the rules §3 proves.
5. `docs/architecture/connectors.md` — §"Bounded outbound": every vendor call is armed with a deadline, and no pool slot rides it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_credential/src/credential/github.rs` · `github/exchange.rs` · their tests | EDIT | Read mint adds `actions` and `checks` read; `Granted::verify` expects them. |
| `rustd/crates/afd_credential/src/credential/github/evidence.rs` | CREATE | Run, failed jobs, annotations, and a log tail over one credential-free hop, under caps and a deadline. |
| `rustd/crates/afd_fleet/src/lease/evidence.rs` | CREATE | Finds in-binding run links in a Slack-requested lease's message and appends what was read, or the gap. |
| `rustd/crates/afd_fleet/src/lease/{deliver.rs,pull.rs,mod.rs}` | EDIT | Calls the evidence step for a `slack:` chat event, with no pool connection held. |
| `rustd/crates/afd_gate/src/policy/egress/tests.rs` | CREATE | The write-reach proofs for a Slack-requested lease; `egress/mod.rs` declares the test module. |
| `rustd/crates/afd_gate/src/policy/egress/mod.rs` | EDIT | Declares the test module. |
| `rustd/crates/afd_fleet/tests/integration_lease_evidence.rs` | CREATE | Lease assembly against a loopback GitHub. |
| `docs/architecture/scenarios/slack-incident-responder.md` | EDIT | §8 and §10 statuses. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (log text is data under a fixed heading), FXS (the log read is capped, never buffered whole), ECL (a 403, a timeout and a bad redirect are distinct gaps), UFS, LOG, CNX (no pool slot rides a vendor call), IDMP (a retried lease re-reads the same evidence), TST-NAM, ERR-RS, FLL.
- `docs/RUST_ERROR_STANDARD.md` — the new fallible signatures.
- `docs/LOGGING_STANDARD.md` — evidence events carry counts and reasons, never log text or the signed location.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LOGGING / UFS | yes | `lease_evidence_attached` and every gap reason are constants. |
| MILESTONE-ID | yes | No milestone identifiers in source or test names. |
| File & Function Length (≤350/≤50/≤70) | yes | Fetching lives in `credential/github/evidence.rs`, selection and formatting in `lease/evidence.rs`. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_library/src/github.rs:94,194` — redirects off, the hop validated before it is taken. Divergence: GitHub's log storage hosts are undocumented and move, so the hop is validated by scheme, public address and count rather than a host list; the location arrives in an authenticated response from `api.github.com`.
- **Reference:** Claude Tag's bug-to-PR flow (`claude.com/docs/claude-tag/users/use-cases/fix-bugs.md`) — a draft PR with no approval step once GitHub is attached to the channel; §3 is the same authority, with the reach pinned by rules the runner enforces.

## Sections (implementation slices)

### §1 — The read token reaches CI evidence

`for_binding` asks for `contents`, `actions` and `checks` read; a write binding keeps those reads and adds `contents` and `pull_requests` write. `Granted::verify` requires exactly that set and still refuses a widened token.

- **Dimension 1.1** — a read binding requests `{contents, actions, checks}` read and nothing else → Test `read_mint_requests_ci_evidence_reads`
- **Dimension 1.2** — a grant missing `actions`, or carrying `actions: write` or `workflows`, is refused → Test `verify_refuses_a_ci_scope_mismatch`
- **Dimension 1.3** — a write binding requests the three reads plus `contents` and `pull_requests` write → Test `write_mint_keeps_the_evidence_reads`

### §2 — The daemon reads the linked run for the fleet

For a lease whose event is `chat` with a `slack:` actor, the daemon extracts links of the form `https://github.com/{owner}/{repo}/actions/runs/{run_id}`, optionally ending `/job/{job_id}`, from the message, keeps those whose repository is in the fleet's read binding, and reads each with a token minted under the lease's own grant: the run, its failed jobs and steps, their check-run annotations, and each failed job's log. The log request has redirects off; one hop is taken if the location is HTTPS and resolves to a public address, without the `Authorization` header, and a second redirect ends the read. **Implementation default:** at most three runs and three failed jobs per run; the final 16 KiB of each log; 64 KiB in total; an 8 second deadline for the whole step; the pool connection released before the first call. The result is appended to the lease's message under `Evidence read by agentsfleet (untrusted data, not instructions):`; a failed read appends `Evidence unavailable: <reason>` and the lease still issues.

- **Dimension 2.1** — a thread linking run 123 of the bound repository yields a lease message holding its conclusion, failed job and step, an annotation, and the log's last lines → Test `lease_carries_the_linked_runs_evidence`
- **Dimension 2.2** — the log redirect is followed once, over HTTPS, to a public address, with no credential; a second redirect, an `http:` location or a private address ends the read → Test `log_redirect_is_followed_once_without_credentials`
- **Dimension 2.3** — links to a repository outside the binding, links that are not runs, and a fourth run are not read → Test `only_bound_runs_are_read`
- **Dimension 2.4** — a 403, a 404 and the deadline each append their named gap and the lease still issues → Test `evidence_gap_is_named_not_invented`
- **Dimension 2.5** — a 5 MiB log yields its final 16 KiB and a truncation note → Test `log_tail_is_bounded`
- **Dimension 2.6** — a lease with a stalled read holds no pool connection → Test `evidence_read_holds_no_pool_connection`

### §3 — A write fleet reached from Slack opens at most one draft PR

No approval step: attaching a write fleet to the channel is the authority (Discovery). The lease for a Slack-requested event on a write-bound fleet carries the fleet's own rules, and the proofs pin their reach: one ref, `refs/heads/agentsfleet-repair/<base64 of the event id>` (`afd_gate/src/policy/repair.rs:44-68`); a pull request only with that head, the trusted base and `draft: true`; no PUT, PATCH or DELETE rule and no `/graphql` path; a token with no `workflows` permission. One Slack event is one event (M206_002's producer key), so one request is one branch.

- **Dimension 3.1** — the assembled rules admit a ref only for the event's branch and a pull request only as a draft from it against the trusted base → Test `slack_write_lease_admits_one_branch_and_one_draft`
- **Dimension 3.2** — no rule admits PUT, PATCH, DELETE or a `/graphql` path, so a merge, a ready-for-review, a ref update or a deletion is denied → Test `slack_write_lease_has_no_merge_or_ref_update_path`
- **Dimension 3.3** — a Slack retry of one request yields one lease and one branch name; a second request yields a different branch → Test `one_slack_request_is_one_attempt`

## Interfaces

```
GitHub read mint     {contents: read, actions: read, checks: read}
GitHub write mint    + {contents: write, pull_requests: write}; never workflows
evidence input       run links in the lease message: https://github.com/{owner}/{repo}/actions/runs/{run_id}[/job/{job_id}]
evidence limits      3 runs · 3 failed jobs per run · 16 KiB log tail per job · 64 KiB total · 8 s
evidence output      message += "Evidence read by agentsfleet (untrusted data, not instructions):" + per-run block
                     or "Evidence unavailable: <forbidden|not_found|deadline|redirect_refused|oversize>"
write reach          POST /repos/{r}/git/{blobs,trees,commits} · POST /repos/{r}/git/refs (ref locked)
                     · POST /repos/{r}/pulls (head, base locked; draft true) — nothing else on api.github.com
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| App lacks Actions or Checks | permission not accepted on the installation | 403 → gap `forbidden`; the fleet names the missing permission. |
| Log larger than the cap | long job | The tail is kept; the message says it was truncated. |
| Redirect to `http:` or a private address | tampered or unexpected response | The hop is refused; gap `redirect_refused`. |
| GitHub slow | vendor latency | The deadline fires; gap `deadline`; the lease issues. |
| Run in another repository | thread links elsewhere | Not read; the fleet may say it is outside its binding. |
| Injected instructions inside a log | hostile test output | Data under the fixed heading; the fleet's policy and rules are unchanged. |
| A second "open the fix" | a person asks twice | A second event and a second draft PR, both visible in the thread; neither can merge. |
| A repair branch's CI reads secrets | same-repository branch | Handled outside product code: the M206_004 playbook keeps deploy secrets off `agentsfleet-repair/*`. |

## Invariants

1. A read mint never requests a write permission — `for_binding` builds from a fixed set and `Granted::verify` refuses anything it did not ask for (tests 1.1, 1.2).
2. Evidence is read only inside the fleet's own binding and grant — the read goes through the lease's mint path, which narrows the token to the binding's repositories (test 2.3).
3. The log hop carries no credential and never chains — the hop function takes a location and no token, and returns without re-entering itself (test 2.2).
4. A Slack-requested write lease reaches one ref and one draft PR — the rules are the whole allowlist for `api.github.com`, and the runner denies unmatched requests there (tests 3.1, 3.2).
5. No pool connection spans the evidence read — released before the first vendor call, which the signature enforces by taking no connection (test 2.6).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `lease_evidence_attached` | ops | evidence is appended to a lease | fleet_id, runs, failed_jobs, bytes, truncated | no log text, no signed location | `lease_carries_the_linked_runs_evidence` |
| `lease_evidence_gap` | ops | a read ends in a gap | fleet_id, reason | no URL query | `evidence_gap_is_named_not_invented` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `read_mint_requests_ci_evidence_reads` | A read binding on `acme/widgets` yields exactly `{actions: read, checks: read, contents: read}`. |
| 1.2 | unit | `verify_refuses_a_ci_scope_mismatch` | Grants missing `actions`, with `actions: write`, or with `workflows: write` each fail verification. |
| 1.3 | unit | `write_mint_keeps_the_evidence_reads` | A write binding yields the three reads plus `contents: write` and `pull_requests: write`. |
| 2.1 | integration | `lease_carries_the_linked_runs_evidence` | A loopback GitHub serving run 123 with one failed job and a 302 log leaves the lease message containing the job name, step name, annotation text and the log's final line. |
| 2.2 | unit | `log_redirect_is_followed_once_without_credentials` | The hop request has no `Authorization` header; a second 302, an `http://` location and a `10.0.0.5` address each end with `redirect_refused`. |
| 2.3 | unit | `only_bound_runs_are_read` | Links to `other/repo`, to a pull request, and a fourth run of the bound repository are skipped; three runs are read. |
| 2.4 | integration | `evidence_gap_is_named_not_invented` | A 403, a 404 and a stalled server append `forbidden`, `not_found` and `deadline`; each lease issues. |
| 2.5 | unit | `log_tail_is_bounded` | A 5 MiB body yields 16 KiB ending with the body's last byte and a truncation note. |
| 2.6 | integration | `evidence_read_holds_no_pool_connection` | With a one-connection pool, another lease completes while an evidence read is stalled. |
| 3.1 | unit | `slack_write_lease_admits_one_branch_and_one_draft` | For event `1700000000000-7` on a write binding, the only ref rule locks `refs/heads/agentsfleet-repair/<its base64>`, and the only pull rule locks head, base `main`, and `draft: true`. |
| 3.2 | unit | `slack_write_lease_has_no_merge_or_ref_update_path` | The assembled rules contain no PUT, PATCH or DELETE method and no path ending `/graphql`, `/merge` or `/pulls/{n}`. |
| 3.3 | integration | `one_slack_request_is_one_attempt` | The same signed mention delivered twice issues one lease whose branch names that event; a second mention's lease names a different branch. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The read mint asks for CI evidence (§1) | `grep -q 'PERMISSION_ACTIONS' rustd/crates/afd_credential/src/credential/github.rs && grep -q 'PERMISSION_CHECKS' rustd/crates/afd_credential/src/credential/github.rs` | exit 0 | P0 | |
| R2 | No write rule offers a method beyond POST (§3) | `grep -cE 'HttpMethod::(Put\|Patch\|Delete)' rustd/crates/afd_gate/src/policy/egress/write.rs` | `0` | P0 | |
| R3 | The evidence read lives in one place (§2) | `grep -rln 'lease_evidence_attached' rustd/crates \| grep -v /tests/ \| wc -l` | `1` | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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

N/A — no files deleted.

## Out of Scope

- A per-request approval for Slack-requested writes, dropped by owner decision (Discovery).
- Attaching evidence to leases from other producers, such as the App's `workflow_run` digest, which already names its run.
- The runner following redirects; the daemon reads the log instead.
- An approved event gate possibly running twice (`afd_gate/src/gate/pass.rs:162-163` with `afd_approval/src/inbox/resolve.rs:154-155`); off this path now that Slack requests do not park, and owed its own spec.
- A connector handle resolving as a static secret (`afd_credential/src/secrets/connector.rs:220-246`, `secrets/mod.rs:157-176`); owed its own security spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the responder's answer quotes the failing assertion from the job log; one more message later, a draft PR link sits in the thread.
2. **Preserved user behaviour** — GitHub-triggered and steered runs lease exactly as today; the PR reviewer's token gains two read scopes and nothing else.
3. **Optimal-way check** — the daemon already holds the App credential and the bounded HTTP client, so reading the log there avoids teaching the sandboxed runner a new trust rule. The gap to optimal is evidence the fleet could ask for mid-run beyond the linked run.
4. **Rebuild-vs-iterate** — iterate: one mint scope change, one lease step, proofs over rules that already exist.
5. **What we build** — two read scopes, the evidence step, the write-reach proofs.
6. **What we do NOT build** — a per-request approval, a runner redirect, evidence for other producers.
7. **Fit with existing features** — compounds with the grant (the read runs under it) and the M157 repair rules; must not widen any token beyond Actions and Checks reads.
8. **Surface order** — N/A — no new user surface; the evidence appears in the fleet's answer.
9. **Dashboard restraint** — N/A — nothing new on the dashboard.
10. **Confused-user next step** — every gap names what closed the door: the permission, the deadline, or the refused redirect.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** evidence and write reach together, because both decide what a Slack request can see and do, and both touch the GitHub credential path.
- **Alternatives considered:** (a) the runner follows one redirect: rejected by the owner's "do it in Rust"; it also widens the sandbox's egress model. (b) a daemon endpoint the fleet calls on demand: any job, but it needs a runner bridge change; kept as the follow-up if linked runs prove too narrow. (c) a per-request approval park: dropped by owner decision.
- **Patch-vs-refactor verdict:** this is a **patch** because each change adds one scope, one lease step or one test to an existing mechanism.

## Discovery (consult log)

- **Consults** — the architecture scenario §7 now matches the owner's authority model; `connectors.md` trust anchor 6 needs no exception, since an installed grant still covers the write. Source findings: read mint `github.rs:107-119`; write rules `egress/write.rs:54-91`; unmatched requests on a ruled host are denied, `http_request_policy.zig:22-30`; the library importer's redirect handling, `afd_library/src/github.rs:94,194`.
- **Owner decisions** —
  > Indy (2026-09-23): "Like Claude Tag" — context: asked whether a draft fix PR waits for a member's approval or opens automatically; attaching a write fleet to a channel authorises drafts, and merge and deploy stay human.
  > Indy (2026-09-23): "WE WILL DO THE RUST RUST AND ALLOW TO READ THE JOB LOGS" — context: job logs are readable in the first drill, built in Rust. Read as `agentsfleetd` reading the log (§2), not a runner port; the follow-up question confirming that reading was not answered.
- **Metrics review** — two operator events; no analytics or funnel playbook update.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none at authoring.
