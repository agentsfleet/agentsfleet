<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M194_001: The grant a human can actually give

**Prototype:** v2.0.0
**Milestone:** M194
**Workstream:** 001
**Date:** Sep 08, 2026
**Status:** PENDING
**Priority:** P0 — no fleet declaring a mintable credential can run in production. Every teammate in `library/*` declares one. The acceptance walk stopped here.
**Categories:** API, INFRA, UI
**Batch:** B1 — single workstream; the grant path is one change and its test is one lane.
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** pending — measured before the Pull Request as `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`), or `n/a — no code on this branch`
**Depends on:** M193_001 (its walk found the defect and its verdict file names the build; this spec is what lets that walk finish)
**Inherits from M193_001 under the scope-transfer clause (`docs/TEMPLATE.md:325`):** rubric row R3 (the seven playbook steps reach their observations) → R1 here · rubric row R4 (the verdict is recorded and accepted) → R2 here. M193_001's walk stopped at step 4 on this defect and cannot pass until it is fixed. Both specs record the mapping; M193_001 marks those rows `MOVED to M194_001`.
**Provenance:** agent-generated from a verified investigation — daemon logs on `agentsfleetd-dev`, source reads cited inline, and two adversarial review rounds over `docs/designs/incident-responder-wedge.md`
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §3, §8

---

## Overview

**Goal (testable):** a fleet declaring a mintable credential it has not been granted raises exactly one approval card a human can see and answer; approving it lets the parked delivery run on the next poll; and `playbooks/operations/acceptance/001_playbook.md` step 4 then reaches its stated observation.

**Problem:** `deliver.rs:62-70` parks such a delivery with `no_work("<credential> needs a grant for <integration>")` and writes nothing — no grant row, no approval gate, no terminal row. The Redis stream entry is never acknowledged, so it redelivers every second (`NO_WORK_RETRY_AFTER_MS = 1_000`, `afd_core/src/timing.rs:68`) while the dashboard reports `ACTIVE`. The grant it waits for cannot exist: every `INSERT INTO core.integration_grants` in the repository is in a test file, the tenant plane exposes only list and revoke (`afd_api_tenant/src/lib.rs:105-106`), and `KIND_INTEGRATION_GRANT` is declared at `afd_approval/src/inbox.rs:64`, read at `:201`, and written by nobody. The approve half is built and tested; the request half was never built.

**Solution summary:** create the pending grant and its approval card when a fleet is installed, from the bundle's declared mintable credentials — the provenance the codebase's own vocabulary already assumes (`"Declared by the fleet bundle at install"`). A delivery that still finds no grant raises the card as a backstop rather than parking silently. Route every write through `afd_approval`, which the code already names as the grant table's writer.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m194): a fleet can be granted the credential it declares`
- **Intent (one sentence):** make the human decision that unblocks a credentialed fleet possible to request, so a parked delivery is a question waiting for an answer rather than a silent loop.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/designs/incident-responder-wedge.md` §Appendix — the four Problem 2 findings, each verified at source: the `evidence->>'service'` contract, the untested resolve arm, the duplicate-run hazard, and the fact that denial terminates nothing.
2. `docs/AUTH.md` — this is a standing-authorisation write on a security boundary; the auth chain applies.
3. `rustd/crates/afd_approval/src/grant.rs` — its module note states this crate was already the grant table's writer. The insert belongs here, not on the lease path.
4. `docs/v2/active/M193_001_P0_INFRA_UI_LIVE_ACCEPTANCE_WALK_AND_VERDICT.md` §Discovery — the walk that found this, with the daemon log lines quoted.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_approval/src/grant.rs` | EDIT | Gains the request write, beside `page()` and `revoke()`, in the crate the code already names as the table's writer. |
| `rustd/crates/afd_approval/src/sql.rs` | EDIT | The `INSERT … ON CONFLICT DO NOTHING` over `core.integration_grants`, and the gate raise carrying `evidence->>'service'`. |
| `rustd/crates/afd_fleet_lifecycle/src/` | EDIT | Install raises the grants a bundle declares mintable. |
| `rustd/crates/afd_fleet/src/lease/deliver.rs` | EDIT | The `Ungranted` arm requests before it parks — the backstop for a fleet installed before this spec. |
| `rustd/crates/afd_approval/tests/support/gate_lane.rs` | EDIT | Its comment excludes `integration_grant` deliberately; that exclusion ends here. |
| `rustd/crates/afd_approval/tests/integration_grants.rs` | EDIT | Covers the request and the approve arm, not only list and revoke. |
| `rustd/crates/afd_fleet/tests/integration_gate_grants.rs` | EDIT | The park raises one card, and a redelivering park raises no second one. |
| `docs/v2/active/M193_001_P0_INFRA_UI_LIVE_ACCEPTANCE_WALK_AND_VERDICT.md` | EDIT | R3 and R4 marked `MOVED to M194_001` once this spec carries them. |
| `~/Projects/docs/changelog.mdx` | EDIT | A user-visible change: a fleet that needs a grant now asks for one. Own branch, per Operational defaults. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (the `service` key and the gate kind are each spelled once), NDC (no dead code at write time), NLG (no legacy framing pre-`2.0.0`).
- `docs/RUST_ERROR_STANDARD.md` — every new fallible signature under `rustd/` follows it; compose with `#[from]`, never `map_err(|e| Mine(e.to_string()))`.
- `dispatch/write_rust.md` — ownership, preserved error variants, deterministic concurrency tests.
- `dispatch/write_sql.md` — the insert is a schema-adjacent write; STS/NSQ/SGR apply, and no static strings enter the schema.
- `dispatch/write_auth.md` → the repository's `docs/AUTH.md` — a standing human authorisation is an auth-flow surface.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | Authored from `docs/TEMPLATE.md`; `audits/spec-template.sh --staged` clean before commit. |
| ERROR REGISTRY | yes — new refusal paths | Any new code is registered; the existing `UZ-CONN-*` family is reused where it already says the right thing. |
| LENGTH (≤350/≤50/≤70) | yes — Rust edits | Split before crossing; `make harness-verify` reports it. |
| LOGGING | yes — the request emits an event | `docs/LOGGING_STANDARD.md` §3 `snake_case` `verb_noun`, declared once (RULE UFS). |
| SCHEMA GUARD | no | `core.integration_grants` and `core.fleet_approval_gates` already exist; no `DROP`, no `ALTER`, no migration-array edit. |
| UI GATE / DESIGN TOKEN | only if the Approvals card needs new surface | The existing Approvals page renders gates already; prefer no new component. |
| ZIG | no | No `*.zig` in scope. |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_approval/src/sql.rs` `RESOLVE_GATE` — the approve half, already written, whose `granted` CTE moves the grant row. The request must produce exactly the row shape that statement expects, including `evidence->>'service'`.
- **Reference:** `rustd/crates/afd_gate/src/gate/sql.rs` `INSERT_GATE` and its `record_row` caller — the existing, working way a gate is raised with its pending count on one snapshot. The request raises its card the same way rather than inventing a second path.
- **Reference:** `afd_wire/src/grant.rs` and `afd_approval/tests/integration_grants.rs` — both carry the reason string `"Declared by the fleet bundle at install"`. **Both are test-only** (`mod tests` opens at `grant.rs:87`); they establish the intended vocabulary and provenance, not an existing production writer.

## Sections (implementation slices)

### §1 — Install requests what the bundle declares

The provenance the codebase already assumes. A fleet installed from a bundle declaring mintable credentials leaves install with a pending grant and a card per credential, so the first delivery finds a decision already waiting rather than discovering the need at run time.

- **Dimension 1.1** — installing a fleet whose bundle declares a mintable credential writes one `core.integration_grants` row per credential with status `pending` and the reason `Declared by the fleet bundle at install` → Test `install requests a grant per declared mintable credential`
- **Dimension 1.2** — a non-mintable declaration (a `static` handle, or any name absent from `DECLARED`) requests nothing, because `mintable()` short-circuits and there is no grant to want → Test `a non-mintable declaration requests no grant`
- **Dimension 1.3** — installing the same bundle twice writes one row per (fleet, service), carried by the table's `UNIQUE (fleet_id, service)` constraint rather than by a read-then-write → Test `a repeated install does not duplicate a grant`

### §2 — The card carries what the approve statement reads

`RESOLVE_GATE` matches `g.service = r.evidence->>'service'` (`afd_approval/src/sql.rs:98`). A card missing that key resolves cleanly and leaves the grant `pending` — a failure with no error, which is the class this milestone exists to end.

- **Dimension 2.1** — the raised gate carries `gate_kind = "integration_grant"` and `evidence` containing a `service` key spelled exactly as `Connector::name()` → Test `the raised card names the service the approve statement matches`
- **Dimension 2.2** — approving the card moves the grant to `approved` with `approved_at` set, in the same statement that resolves the gate → Test `approving the card grants the integration`
- **Dimension 2.3** — the gate is raised WITHOUT an `event_id`, so `Inbox::won` lands no continuation event and the still-leasable original delivery is what runs; the work runs once, not twice → Test `approval does not run the work twice`

### §3 — A park is a question, never a silent loop

The backstop for any fleet installed before §1, and the invariant that makes the failure mode impossible to reintroduce.

- **Dimension 3.1** — a delivery reaching `Assembled::Ungranted` requests the grant and raises the card before returning `no_work` → Test `an ungranted park raises a card`
- **Dimension 3.2** — a delivery that redelivers every second raises exactly one card, guarded by a pre-insert lookup on (fleet, event, gate kind) rather than by a rate limit → Test `a redelivering park raises one card, not one per second`
- **Dimension 3.3** — a denied grant ends the parked event with a terminal row naming the denial, so it stops redelivering; today `RESOLVE_GATE`'s `granted` arm moves only the grant's status and the event re-parks forever → Test `a denied grant ends its parked event`

### §4 — The walk finishes

The rows inherited from M193_001. They are graded by re-running the acceptance walk against a build carrying §1–§3, not by unit tests.

- **Dimension 4.1** — `playbooks/operations/acceptance/001_playbook.md` steps 1 through 7 each reach their stated observation against the deployed build → Test `every playbook step reaches its stated observation`
- **Dimension 4.2** — a verdict file for that build records `pass` and `01_verdict_check.sh` accepts it → Test `the verdict check accepts the recorded verdict`

## Interfaces

```
No new public endpoint. The surfaces this changes, all pre-existing:

  Write     core.integration_grants        status pending, on install and on park
            core.fleet_approval_gates      gate_kind "integration_grant"
  Read      GET  /v1/workspaces/{ws}/fleets/{id}/integration-grants   (unchanged)
  Resolve   the existing approvals inbox   (unchanged statement, first real caller)
  Dashboard /w/{ws}/approvals              renders the card it already knows how to
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| The card names no service | `evidence` written without the `service` key | The insert refuses rather than writing a card the approve statement cannot match; the operator sees a registered error, not a silent no-op |
| Two cards for one park | The redelivery raises a second gate | The pre-insert lookup on (fleet, event, gate kind) finds the first; the second raise is a no-op and the poll answers no-work as before |
| The work runs twice | The gate carried an `event_id`, so approval lands a continuation beside the still-leasable original | The gate is raised without one (Dimension 2.3); a continuation-bearing gate for this kind is a test failure |
| A denied grant loops forever | Denial moves the grant's status and leaves the event leasable | The denial writes the event's terminal row; the delivery stops and the operator reads why |
| The grant row exists but no card does | A crash between the two writes | Both writes land in one transaction; a park with a grant and no card is impossible rather than merely unlikely |
| An install-time request for a credential the workspace never connects | The bundle declares it; nobody connects it | The card stays pending and visible. This is correct: the fleet genuinely cannot run, and now says so |

## Invariants

1. A delivery that parks on a missing grant has a corresponding approval card — enforced by the two writes sharing one transaction, and by Dimension 3.1's test over the park path.
2. One park raises one card — enforced by the pre-insert lookup, not by a rate limit, and asserted at the one-second redelivery cadence.
3. A grant row is unique per (fleet, service) — enforced by the table's existing `UNIQUE (fleet_id, service)` constraint, not by application logic.
4. Only `afd_approval` writes `core.integration_grants` — enforced by the insert living in that crate; `afd_gate` continues to read only.
5. A gate of kind `integration_grant` carries no `event_id` — enforced by the raise site and asserted by Dimension 2.3, because a continuation event beside a leasable delivery runs the work twice.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `grant_requested` | daemon | a pending grant and its card are written | `fleet_id`, `service`, `origin` (`install` or `park`) | no credential material; the service is a connector name, never a handle | `install requests a grant per declared mintable credential` |
| `grant_request_suppressed` | daemon | a redelivering park finds a card already open | `fleet_id`, `service` | as above | `a redelivering park raises one card, not one per second` |
| `gate_opened` / `gate_resolved` | daemon | unchanged — the existing gate frames now carry this kind | unchanged | unchanged | `approving the card grants the integration` |

Analytics or funnel playbook update: none. This adds no product funnel step; it makes an existing one completable.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `install requests a grant per declared mintable credential` | Install a bundle declaring `github` → one `integration_grants` row, status `pending`, reason `Declared by the fleet bundle at install` |
| 1.2 | unit | `a non-mintable declaration requests no grant` | A `static` handle, and a name absent from `DECLARED` → zero rows written |
| 1.3 | integration | `a repeated install does not duplicate a grant` | Install twice → exactly one row for (fleet, `github`) |
| 2.1 | integration | `the raised card names the service the approve statement matches` | The gate row → `gate_kind = "integration_grant"`, `evidence->>'service' = "github"` |
| 2.2 | integration | `approving the card grants the integration` | Approve → grant status `approved`, `approved_at` set, in the statement that resolved the gate |
| 2.3 | integration | `approval does not run the work twice` | Approve a park's card → exactly one lease issued for the event, no continuation row |
| 3.1 | integration | `an ungranted park raises a card` | A fleet with a mintable credential and no grant polls → a pending card exists and the poll answered no-work |
| 3.2 | integration | `a redelivering park raises one card, not one per second` | Ten consecutive polls → one card, and `grant_request_suppressed` on the rest |
| 3.3 | integration | `a denied grant ends its parked event` | Deny → the event holds a terminal row and the next poll does not re-park it |
| 4.1 | e2e | `every playbook step reaches its stated observation` | Walk `001_playbook.md` steps 1–7 on the deployed build → each "you must see" observed |
| 4.2 | integration | `the verdict check accepts the recorded verdict` | `01_verdict_check.sh {sha}` → exit 0 and the `✓ verdict for …: pass` line |

Regression rows: `a non-mintable declaration requests no grant` is the regression guard — it fails if the request path ever widens past `declared.mintable()`, which would raise cards nobody can act on for `grafana`, `datadog` and `fly`. Idempotency rows: `a repeated install does not duplicate a grant` and `a redelivering park raises one card` are the idempotency pair, one per write site.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The seven playbook steps reach their observations (§4) — inherited from M193_001 R3 | walk `playbooks/operations/acceptance/001_playbook.md` steps 1–7 | every "you must see" observed | P0 | |
| R2 | The verdict is recorded and accepted (§4) — inherited from M193_001 R4 | `./playbooks/operations/acceptance/01_verdict_check.sh {merge_sha}` | exit 0, `✓ verdict for {merge_sha}: pass — <reviewer> on <date>` | P0 | |
| R3 | A declared mintable credential leaves install with a pending card (§1) | `make test-integration-rustd` | `install requests a grant per declared mintable credential` passes | P0 | |
| R4 | Approving the card grants the integration and the work runs once (§2) | `make test-integration-rustd` | `approving the card grants the integration` and `approval does not run the work twice` both pass | P0 | |
| R5 | A park is a question, not a loop (§3) | `make test-integration-rustd` | `an ungranted park raises a card` and `a redelivering park raises one card, not one per second` both pass | P0 | |
| R6 | A denied grant stops redelivering (§3) | `make test-integration-rustd` | `a denied grant ends its parked event` passes | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration suite passes | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every S-row Verify command is copied **verbatim from `.oracle/orly.json`** (`conform`, `verify.*`) — the same set `orly gate` runs, so the rubric and the mechanical PR gate grade one boundary.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

`KIND_INTEGRATION_GRANT` (`afd_approval/src/inbox.rs:64`) and `RESOLVE_GATE`'s `granted` CTE (`sql.rs:92-101`) are currently unreachable in production — declared and read, written by nobody. This spec makes them live rather than removing them, which is the correct disposition: the approve half was always right and had no caller. Nothing is deleted; the sweep records that the dead half becomes reachable and its exclusion in `tests/support/gate_lane.rs:99-102` ends.

## Out of Scope

- The `runner-triage` responder wedge — `docs/designs/incident-responder-wedge.md` Approach C. It declares a `static` credential, never produces `Ungranted`, and needs nothing from this spec. It is the recommended thing to build FIRST; it is simply not this spec.
- The readiness contract — a fleet refusing to report `ACTIVE` while it cannot fire. That is the design's Approach B and covers all eleven `no_work` call sites in `rustd/crates/afd_fleet/src/lease/`; this spec touches one.
- The library card's `NEEDS: GITHUB` badge, which states one prerequisite where there are three.
- Repository subscription (`repositories:` in `TRIGGER.md`) — `github-pr-reviewer` declares none and so receives no App traffic. Configuration, editable in the dashboard, not code.
- The other known-open items M193_001 recorded: the flaky `afd_fleet` fence test, the epoch-versus-age runner alert, and the avatar.

## Product Clarity (authoring record)

1. **Successful user moment** — a person installs a fleet, sees one card saying it needs GitHub, presses approve, and the fleet runs. The decision is a decision, not a dead end.
2. **Preserved user behaviour** — everything. A fleet with its grants already approved behaves exactly as before; a fleet with none stops lying about it.
3. **Optimal-way check** — the optimal shape is asking at install, when the person is already there and the bundle already says what it needs. The park-time request is the backstop, not the design. A lazy-only request was considered and rejected: it puts a write on the lease hot path and asks at the one moment nobody is looking.
4. **Rebuild-vs-iterate** — iterate. The table, the gate plane, the approve statement and the dashboard card all exist. One third of a three-part verb is missing.
5. **What we build** — the request write, its card, the guard that keeps one park to one card, and the denial that ends an event.
6. **What we do NOT build** — no auto-approval, no widening past `declared.mintable()`, no new endpoint, no second writer of the grant table.
7. **Fit with existing features** — it completes request/approve/revoke. The Approvals page already renders gates; this gives it its first real one.
8. **Surface order** — daemon first, dashboard second. The card must exist before a page can show it, and the existing Approvals page needs no change to render it.
9. **Dashboard restraint** — no new surface. If the existing card renders the kind acceptably, nothing in `ui/` changes.
10. **Confused-user next step** — the card names the fleet and the integration and offers approve or deny. A person who does not want to grant it denies, and the fleet's event ends with a reason instead of spinning.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** install-time request (§1) with a park-time backstop (§3), both writing through `afd_approval`. Install-time matches the provenance the codebase's own reason string assumes; the backstop covers fleets installed before this lands and makes the invariant unconditional.
- **Alternatives considered:** park-time only — rejected, it asks at the moment nobody is watching and puts a write on the lease hot path. Auto-granting when the workspace has the connector connected — rejected, it converts a standing human authorisation into a side effect of an unrelated action, which is the opposite of what the gate exists for. Widening `mintable()` so api-key connectors also require grants — rejected, `connector.rs:215-219` is explicit that their key never reaches a broker, so the row would be one nothing dispatches on.
- **Patch-vs-refactor verdict:** a **patch**. The larger question it implies — that a fleet can report `ACTIVE` while incapable of acting, and eleven `no_work` sites can park with no card — is real, is named in Out of Scope, and belongs to the readiness contract rather than being mud-patched here.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.

| Date | Consult | Outcome |
|---|---|---|
| Sep 08, 2026 | Indy — fix step 4, then design first | Asked whether to record the walk's defect and spec it elsewhere, check the workspace model, or fix it inside M193: "Yes fix 4 in this PR, and we push the PR, and upon merge we continue on a new session to start on verifying step 4." Then, after the value question reframed the work: "I want you to write the design first." This spec is that design's Problem 2, authored rather than implemented. |
| Sep 08, 2026 | Gate-flag triage — the grant write is a security boundary | Surfaced before any code: 📟 `core.integration_grants` write · 🔦 new write in `afd_approval` plus its call sites and tests · 📈 a fleet declaring any credential becomes usable at all · 💥 every credentialed teammate parks forever, invisibly · ☠️ do it, and note the repo rule that security-boundary work takes its own spec and AUTH review. This spec IS that separate spec, which is the rule being followed rather than overridden. |
| Sep 08, 2026 | Two adversarial review rounds over the design | 6/10 both rounds, 37 issues, 35 fixed. Four were load-bearing errors in the analysis feeding this spec: the wedge does not force this fix (a `static` credential never produces `Ungranted`); `GET /v1/fleets/runners/{runner_id}/leases` exists; `afd_wire/src/grant.rs:124` is `#[cfg(test)]`, not production; the fleet event tail is not writable. All four are corrected in `docs/designs/incident-responder-wedge.md` and reflected here. |

- **Metrics review** — `grant_requested` and `grant_request_suppressed` are new operator signals, declared above with their properties and privacy guard. No analytics or funnel playbook update: no product funnel step is added.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results, populated as the work proceeds.
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
