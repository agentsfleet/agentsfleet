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

# M206_003: Slack-originated work stays read-only until a workspace member approves it, and one approval runs once

**Prototype:** v2.0.0
**Milestone:** M206
**Workstream:** 003
**Date:** Sep 23, 2026
**Status:** PENDING
**Priority:** P0 — a Slack channel member is not an authenticated `agentsfleet` principal, and without this a mention could drive any credential a subscribed fleet holds.
**Categories:** API
**Batch:** B1 for §1–§3, whose tests seed `slack:` events directly; `afd_approval/src/inbox/resolve.rs` is edited here and in M206_001, so this rebases onto it. §4 lands in B2, after M206_002 §6's notices. Ships in M206_002's Pull Request.
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M206_001 — the continuation inherits the reply destination its notices and answer use; M206_002 — the producer whose events this workstream bounds.
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 23, 2026) from source reads at `b1bc6f0c4`
**Canonical architecture:** `docs/architecture/scenarios/slack-incident-responder.md` §7

---

## Overview

**Goal (testable):** a lease for a `chat` event whose actor starts `slack:` carries a read-only network policy; that event, addressed to a fleet whose repository binding is `write`, parks before any lease and tells its thread how to approve; a workspace member's approval runs the request exactly once, as a continuation carrying the original message and reply destination, and that run alone may write.

**Problem:** M206_002 lets anyone in a channel start a fleet run, and anything posted in the thread becomes model input. Today the only brakes on a run's writes are the fleet's own `network.read_only`, off unless declared (`rustd/crates/afd_fleet_runtime/src/config/policy.rs:154-157`), and, for GitHub, the install-time grant, which since M202 covers repository writes with no per-event card (`afd_gate/src/gate/first.rs:8-17`; `docs/architecture/connectors.md` trust anchor 6). M202's reasoning holds for events from systems an operator configured; it does not hold for a sentence typed by any member of a Slack channel. Separately, an approved event gate appears to run twice: the parked delivery stays leasable and passes once its gate is approved (`afd_fleet/src/lease/admit/mod.rs:71-78`, `afd_gate/src/gate/pass.rs:162-163`), while the same approval admits a continuation (`afd_approval/src/inbox/resolve.rs:154-155,223-289`). unverified: whether both leases are issued in practice; settled by test 3.1 against real Postgres and Dragonfly.

**Solution summary:** policy assembly forces `read_only` for any lease whose event is `chat` with a `slack:` actor, keeping the fleet's declared `read_post_paths`. The first-encounter gate pass parks such an event, before the rules walk, when the fleet's repository binding is `write`, with a gate kind reserved for it and a card naming the repository, base, requesting Slack user and thread. A workspace member approves through the existing dashboard, API or command-line interface (CLI) `agentsfleet approvals approve` routes; no Slack identity can resolve it. On any resolution the parked delivery ends as `gate_blocked`, and only the continuation runs, with the parked event's message and destination. The thread receives a notice when the request parks and when it is denied or expires; the approved run's answer, a draft Pull Request (PR) link, arrives by ordinary delivery.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(gate): Slack-originated work is read-only until a workspace member approves it`
- **Intent (one sentence):** a person in Slack can ask for a fix, but only a workspace member can let the fleet write, and one approval produces one attempt.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_gate/src/gate/first.rs` — the first-encounter pass and the note on why M202 removed the unconditional write park; the Slack park sits before `match_rule` and keeps continuations out.
2. `rustd/crates/afd_gate/src/policy/shape.rs` and `rustd/crates/afd_gate/src/policy/build.rs` — where a stored network policy becomes the lease's; the forced read-only belongs in the same conversion.
3. `rustd/crates/afd_approval/src/inbox/resolve.rs` — the resolve, the continuation, and the runless wake; the exactly-once rule lands here and in the lease pass.
4. `rustd/crates/afd_gate/src/gate/pass.rs` — `resolve_recorded`, which lets a parked delivery pass once approved.
5. `docs/architecture/scenarios/slack-incident-responder.md` — §7, the four stages and who authorises each.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_gate/src/policy/{build.rs,shape.rs,egress/mod.rs}` | EDIT | Assembly takes the event's type and actor; `slack:` chat forces `read_only` and emits read repository rules only. |
| `rustd/crates/afd_gate/src/gate/{first.rs,park.rs,detail.rs}` | EDIT | The Slack write park before the rules walk; its card detail. |
| `rustd/crates/afd_gate/src/gate/slack.rs` | CREATE | The origin predicate and the reserved gate kind, in one place both callers import. |
| `rustd/crates/afd_gate/src/gate/pass.rs` | EDIT | An event gate resolved either way ends the parked delivery. |
| `rustd/crates/afd_fleet_runtime/src/config/raw/predicate.rs` | EDIT | The new gate kind is reserved, so a fleet cannot author it. |
| `rustd/crates/afd_approval/src/inbox/resolve.rs` · `inbox/announce.rs` | EDIT | Deny and expiry owe a notice; the continuation reads the parked message. |
| `rustd/crates/afd_fleet/src/lease/{pull.rs,pull/refuse.rs,deliver.rs}` | EDIT | The parked delivery is acknowledged and closed once its gate resolves; the continuation's lease carries the parked message. |
| `rustd/crates/afd_ingress/src/slack/notice.rs` | EDIT | Park, deny and expiry notice texts beside M206_002's. |
| `rustd/crates/afd_gate/tests/integration_slack_write_park.rs` · `rustd/crates/afd_approval/tests/integration_one_run_per_approval.rs` · `rustd/crates/afd_gate/src/policy/shape/tests.rs` | CREATE | The proofs below. |
| `docs/architecture/connectors.md` · `docs/architecture/capabilities.md` · `docs/architecture/scenarios/slack-incident-responder.md` | EDIT | Trust anchor 6 names its Slack exception; the approval row and §10 statuses. |

Cross-repository, on its own branch: `~/Projects/docs/fleets/approvals.mdx` gains the Slack-requested approval.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — PRI (thread text cannot widen policy), IDMP (a retried resolve and a replayed delivery run once), ECL, TGU (the park reason is a variant, not a flag), UFS, RSP (the reserved kind is refused at parse), OBS (park, resolve and forced read-only are logged), WAUTH (only the authenticated resolve routes answer), LOG, ERR-RS, FLL, TST-NAM.
- `docs/RUST_ERROR_STANDARD.md`.
- `docs/LOGGING_STANDARD.md` — no request text in any event; the card holds it, behind workspace authorisation.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| LOGGING / UFS | yes | `gate_slack_write_parked`, `lease_read_only_forced` and the gate kind are constants. |
| MILESTONE-ID | yes | No milestone identifiers in source or test names. |
| File & Function Length (≤350/≤50/≤70) | yes — `resolve.rs` is past 280 lines | Notice ownership lives in `afd_ingress/src/slack/notice.rs`; `resolve.rs` calls it. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_gate/src/gate/first.rs` before M202 (`git log -p` on that file) — the unconditional repository-write park. Divergence, on purpose: this park applies only to `chat` events with a `slack:` actor, so an approval's continuation, typed `continuation`, never re-parks, which is the treadmill M202 removed.
- **Reference:** `docs/architecture/high_level.md` §6.2 — Rung 1 names a Slack-user-to-approver allowlist; this workstream keeps approval with authenticated members and leaves that allowlist to a later milestone.

## Sections (implementation slices)

### §1 — An unapproved Slack run is read-only

Policy assembly receives the event's type and actor. For `chat` with an actor starting `slack:`, the lease's `network.read_only` is `true` whatever the fleet declared, and its declared `read_post_paths` stay, so a Grafana query endpoint the fleet named still answers. The lease's repository rules are the read set even when the binding is `write`: under `read_only` the runner still admits a POST that matches a daemon-authored rule (`src/runner/engine/runtime/policy_http_request.zig:180-187`, the `.allowed` arm at 183), so forcing the flag alone would leave the write rules open. A continuation is typed `continuation` and gets the fleet's declared policy.

- **Dimension 1.1** — a `slack:` chat lease is read-only for a fleet declaring `read_only: false` → Test `slack_chat_lease_is_read_only`
- **Dimension 1.2** — steer, webhook, cron and continuation leases keep the declared value → Test `other_origins_keep_their_declared_policy`
- **Dimension 1.3** — declared `read_post_paths` survive the forcing → Test `forced_read_only_keeps_declared_read_posts`
- **Dimension 1.4** — a `slack:` chat lease on a write-bound fleet carries no write rule, so a POST to the bound repository is refused → Test `slack_chat_lease_carries_read_rules_only`

### §2 — A Slack request to a write-bound fleet parks first

In `judge_first_encounter`, before `match_rule`: a `chat` event with a `slack:` actor on a fleet whose binding is `write` parks with the reserved kind `slack_write_request`. The card states the repository, trusted base, requesting Slack user and channel identifiers, thread timestamp, and the request text capped at 2,000 characters. Timeout follows the fleet's gate policy default. Resolution uses only the existing authenticated routes.

- **Dimension 2.1** — the event parks before any lease and no token is minted → Test `slack_request_to_write_fleet_parks_before_lease`
- **Dimension 2.2** — the same request from a steer or a webhook does not park, preserving M202 → Test `other_origins_to_write_fleets_do_not_park`
- **Dimension 2.3** — a fleet document naming the reserved kind is refused at parse → Test `reserved_gate_kind_cannot_be_authored`
- **Dimension 2.4** — the Slack events route has no path to `Inbox::resolve` → Test `no_slack_delivery_resolves_a_gate`

### §3 — One approval, one run

When an event gate resolves, approved or not, the parked delivery ends as `gate_blocked` and is acknowledged; on approval the continuation is the only run. The continuation's lease carries the parked event's `message`, and its answer is owed to the parked event's destination through M206_001's inheritance. Its repair branch is derived from the continuation's own event identifier, as `afd_gate/src/policy/repair.rs` already does.

- **Dimension 3.1** — approving a parked Slack request issues exactly one lease, for the continuation → Test `one_approval_issues_one_lease`
- **Dimension 3.2** — the continuation's lease message equals the parked event's → Test `continuation_runs_with_the_parked_message`
- **Dimension 3.3** — a denied or expired gate issues no lease and ends the parked delivery → Test `refused_gate_ends_the_parked_delivery`
- **Dimension 3.4** — two members approving at once, or one resolve retried, still yield one lease → Test `racing_approvals_run_once`

### §4 — The thread hears about the gate

Parking owes a notice to the event's thread: the fleet's name, what it asked to do, the dashboard route `/w/<workspace_id>/approvals`, and `agentsfleet approvals approve <gate_id>`. Denial and expiry owe one notice each. Notices follow M206_002 §6: fixed text, keyed by the event and gate state, owed once.

- **Dimension 4.1** — a park owes one notice carrying the approval route and command → Test `park_owes_one_approval_notice`
- **Dimension 4.2** — deny and expiry each owe one notice; an approval owes none of its own → Test `resolution_notices_are_owed_once`

## Interfaces

```
lease ExecutionPolicy.network.read_only = declared || (event_type == chat && actor.starts_with("slack:"))
lease origin rules for that event      = read rules only, whatever the binding's access
gate kind (reserved)       slack_write_request
card detail                {repository, base_branch, slack_user_id, channel_id, thread_ts, request_text ≤ 2,000 chars}
approve / deny             POST /v1/workspaces/{workspace_id}/approvals/{gate_id}/{approve|deny}   (unchanged)
                           agentsfleet approvals approve|deny <gate_id>                             (unchanged)
resolution effect          parked delivery → gate_blocked + ack; approve → one continuation lease
notice keys                <event_id>:gate:parked · <event_id>:gate:denied · <event_id>:gate:expired
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Injected write in a thread | hostile text asks a read-only fleet to POST | The runner answers `method_not_allowed`; the fleet reports the refusal. |
| Slack user tries to approve | a mention saying "approve" | A mention is only ever an event; no Slack path resolves a gate. |
| Park write fails | Postgres unavailable | Fail closed: no-work answer, retried next poll; nothing leases. |
| Two approvals race | two members, or a retried click | One resolve wins (`afd_approval/src/sql.rs:148-179`); continuation keyed by the gate action; one lease. |
| Approval after expiry | slow reviewer | Expired stands; the thread already holds the expiry notice; the requester asks again. |
| Continuation lost in the queue | Dragonfly failover | Admission row exists; replay sweeper appends; one lease. |
| Parked delivery redelivered after resolution | lost acknowledgement | Row is `gate_blocked`; the redelivery is closed as already ended. |
| Write-bound fleet paused while parked | operator | Approval lands the continuation; the paused fleet does not lease it until resumed. |

## Invariants

1. A `slack:` chat lease is read-only — assembly computes `read_only` from the event and emits only read origin rules, and the runner enforces both (tests 1.1, 1.3, 1.4).
2. A Slack request never leases on a write-bound fleet before approval — the park precedes the rules walk and returns before any mint (test 2.1).
3. A continuation never parks on this rule — the predicate requires type `chat` (test 2.2 covers the continuation case).
4. One approval, one lease — the parked delivery is closed on resolution, and the continuation's admission is keyed by the gate action (tests 3.1, 3.4).
5. Only an authenticated workspace member resolves a gate — resolution is reachable only from the tenant and signed-callback routes that exist today (test 2.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `lease_read_only_forced` | ops | a `slack:` chat lease is assembled for a fleet that declared writes | fleet_id, agentsfleet_event_id | no request text | `slack_chat_lease_is_read_only` |
| `gate_slack_write_parked` | ops | a Slack request parks on a write-bound fleet | fleet_id, gate_id | no request text | `slack_request_to_write_fleet_parks_before_lease` |
| `gate_parked_delivery_closed` | ops | a resolved gate ends its parked delivery | fleet_id, gate_id, decision | none | `refused_gate_ends_the_parked_delivery` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `slack_chat_lease_is_read_only` | A config with `read_only: false` and event (`chat`, `slack:U01`) assembles `read_only: true`. |
| 1.2 | unit | `other_origins_keep_their_declared_policy` | The same config with actors `steer:u`, `github-app`, `cron:s` and type `continuation` assembles `read_only: false`. |
| 1.3 | unit | `forced_read_only_keeps_declared_read_posts` | Declared `read_post_paths: [/api/ds/query]` survives into the forced policy. |
| 1.4 | unit | `slack_chat_lease_carries_read_rules_only` | A write binding on `acme/widgets` with event (`chat`, `slack:U01`) assembles GET and HEAD rules only; no `POST …/git/refs` or `POST …/pulls` rule is present. |
| 2.1 | integration | `slack_request_to_write_fleet_parks_before_lease` | A `slack_mention` event on a write-bound fleet leaves one `slack_write_request` gate, no lease row, zero mint calls. |
| 2.2 | integration | `other_origins_to_write_fleets_do_not_park` | A steer and a continuation on the same fleet lease without a gate. |
| 2.3 | unit | `reserved_gate_kind_cannot_be_authored` | `gates.rules[0].gate_kind: slack_write_request` is refused naming the key. |
| 2.4 | unit | `no_slack_delivery_resolves_a_gate` | The events route's handler graph holds no `Inbox`; a mention whose text is `approve <gate_id>` becomes an event and the gate stays pending. |
| 3.1 | integration | `one_approval_issues_one_lease` | Approve, then poll repeatedly: `fleet.runner_leases` holds one row, for the continuation; the parked event is `gate_blocked`. |
| 3.2 | integration | `continuation_runs_with_the_parked_message` | The continuation lease's request `message` equals the parked admission's. |
| 3.3 | integration | `refused_gate_ends_the_parked_delivery` | Deny, and separately expire: no lease; the parked event is `gate_blocked`; its stream entry is acknowledged. |
| 3.4 | integration | `racing_approvals_run_once` | Two concurrent approve calls and one retried resolve leave one continuation and one lease. |
| 4.1 | integration | `park_owes_one_approval_notice` | Parking owes one obligation to the thread whose text contains `/approvals` and `agentsfleet approvals approve`. |
| 4.2 | integration | `resolution_notices_are_owed_once` | Deny owes one notice; a repeated deny owes none; approve owes none of its own. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The reserved kind is declared once and reserved (§2) | `grep -rn '"slack_write_request"' rustd/crates \| grep -v /tests/ \| wc -l` | `1` | P0 | |
| R2 | The read-only rule has one home (§1) | `grep -rln 'lease_read_only_forced' rustd/crates \| grep -v /tests/ \| wc -l` | `1` | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
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

- Approving from Slack: interactive buttons, a Slack-user-to-approver allowlist, and a relay onto `/v1/webhooks/{fleet_id}/approval`.
- Gating individual tool calls inside a run; gates stay per event.
- Narrowing the minted GitHub token for Slack leases; a read-only network already refuses every non-read request.
- A continuation inheriting a user-authored gate decision beyond this rule, which M202 left open for its own spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — in the incident thread someone writes `@agentsfleet ci-dev-repairer open the fix`; the thread says it is waiting for approval with a link; a workspace member approves; the next message in the thread is one draft PR link.
2. **Preserved user behaviour** — GitHub-triggered and steered writes still need no card (M202); `grant delete` still stops a fleet; every existing approval route answers as before.
3. **Optimal-way check** — approving from the thread itself is the unconstrained optimum; it needs a verified Slack-to-member identity link, which does not exist, so the approval stays one click away in the dashboard.
4. **Rebuild-vs-iterate** — iterate on the shipped gate: one predicate before the rules walk, and a resolution rule that closes the parked delivery.
5. **What we build** — forced read-only for Slack leases, the Slack write park, one-run-per-approval, three notices.
6. **What we do NOT build** — Slack approvals, per-tool gates, a new approvals page.
7. **Fit with existing features** — compounds with the approvals inbox and M206_001's destination inheritance; must not bring back M202's treadmill, which is why continuations are exempt by type.
8. **Surface order** — the existing CLI verb and dashboard page; the only new surface is the notice text in Slack.
9. **Dashboard restraint** — no Slack badge on cards and no request preview beyond the capped text the card already shows.
10. **Confused-user next step** — the park notice carries the exact approval command and page; a denied request's notice names who denied it.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream for the Slack trust boundary, so read-only, the park and exactly-once review together as one security change.
- **Alternatives considered:** (a) Slack never reaches a write-bound fleet, and the fix is started with an authenticated `agentsfleet steer` — rejected as the default: the thread loses the request and its outcome, though it is the smaller build and stays the fallback. (b) Restore M202's write park for every origin — rejected: it re-parks every GitHub-triggered review. (c) Approve from a Slack button — deferred: no identity link.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds one origin predicate at two existing seams and closes a lifecycle gap in resolution, leaving the gate and inbox shapes as they are.

## Discovery (consult log)

- **Consults** — `ARCH: grounded in connectors.md trust anchor 6 and afd_gate/src/gate/first.rs:8-17 | proposal: park Slack-originated chat events for write-bound fleets | status: conflicts — both state a repository write raises no per-event card | landing: a, after Indy decides`. Source findings: parking leaves the delivery leasable (`lease/admit/mod.rs:71-78`, `lease/pull.rs:270-272`); an approved recorded gate passes (`gate/pass.rs:162-163`); approval also admits a continuation keyed by the gate action (`inbox/resolve.rs:154-155,223-245`); the park makes no Slack call (`gate/park.rs:17-35`).
- **Decisions pending with Indy** — recommended, not approved: Slack requests and a member approves in the dashboard or CLI (this spec) over authenticated steer only; the narrow park for Slack-originated chat events is the one exception to M202's no-card rule. Claude Tag opens draft PRs with no approval step once GitHub is attached to a channel (`claude.com/docs/claude-tag/users/use-cases/fix-bugs.md`); this spec keeps the brief's approval-first rule because a branch pushed into the same repository runs that repository's CI, secrets included, before anyone reads the diff (general GitHub Actions behaviour; which `agentsfleet` workflows expose secrets on non-`main` branches is unverified).
- **Metrics review** — three operator events; no analytics or funnel playbook update.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none at authoring.
