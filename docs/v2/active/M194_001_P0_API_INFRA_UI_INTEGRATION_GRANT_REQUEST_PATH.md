<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M194_001: The grant a human can actually give

**Prototype:** v2.0.0
**Milestone:** M194
**Workstream:** 001
**Date:** Sep 08, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — no fleet declaring a mintable credential can run in production. Every teammate in `library/*` declares one. The acceptance walk stopped here.
**Categories:** API, INFRA, UI
**Batch:** B1 — single workstream; the grant path is one change and its test is one lane.
**Branch:** `feat/m194-grant-request-path`
**Baseline revision:** `509803cf1a3e2765919c99887bcd03ff86d29ab0`
**Test Baseline:** `make test-unit-all` 2489 passed / 0 failed / 429 ignored · `make test-integration-rustd` 426 passed / 0 failed. Integration at the comparison revision was 413 passed, so **+13**. The unit lane's count at `509803cf1` is **not yet measured** — the +9 this session is against 2480 taken on this branch before the REVIEW fixes, which is a delta over the branch and not over the comparison revision; the row is honest about that rather than reporting a number nobody ran.
**Baseline evidence:** `make lint-all` ✓ · `make test-unit-all` 2489/0/429 · `make test-integration-rustd` 426/0 (1m47s) · `make harness-verify` ALL GATES GREEN · `make check-version` ✓ 0.29.0 — one chained run, Sep 09 2026, worktree `agentsfleet-m194-grant-request-path`, docker-compose Postgres + Redis, schemas reset per run.
**Depends on:** M193_001 (its walk found the defect and its verdict file names the build; this spec is what lets that walk finish)
**Inherits from M193_001 under the scope-transfer clause (`docs/TEMPLATE.md:325`):** rubric row R3 (the seven playbook steps reach their observations) → R1 here · rubric row R4 (the verdict is recorded and accepted) → R2 here · Dimension 2.2 (the three state-change steps photographed, `MOVED` there because step 6 was never reached) → Dimension 4.3 here. M193_001's walk stopped at step 4 on this defect and cannot pass until it is fixed. Both specs record the mapping; M193_001 marks those rows `MOVED to M194_001`.
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
4. `docs/v2/done/M193_001_P0_INFRA_UI_LIVE_ACCEPTANCE_WALK_AND_VERDICT.md` §Discovery — the walk that found this, with the daemon log lines quoted.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------------|-----|
| `schema/836_fleet_approval_gates_grant_card.sql` | CREATE | One actionable card per nullable `active_grant_id`, using a unique index without status literals. The column is declared in slot 810 and released on resolution or expiry. |
| `schema/embed.zig` | EDIT | Registers slot 836. Version IS the slot number (RULE MIG). |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | The Rust daemon's own canonical list — the third registration site, and `afd_db::migrations::test_migration_list_matches_schema_directory_and_zig` is what catches a miss. |
| `rustd/crates/afd_approval/src/request.rs` | CREATE | The request verb itself, beside `grant.rs`'s read and revoke rather than inside it: the pair came to 353 lines and the LENGTH cap is 350. Owns `Wanted`, `Origin`, `Requested`, and the gate-kind and evidence-key constants. |
| `rustd/crates/afd_approval/src/sql.rs` | EDIT | `ENSURE_GRANT` plus `REQUEST_GRANT` in one transaction; read and lock the existing grant after any competing insert, then raise using its active reference. Resolution and expiry clear the reference. |
| `rustd/crates/afd_approval/src/grant.rs` | EDIT | `IntegrationGrants` gains the entropy source the request mints through, and the two accessors the sibling module reads it by. |
| `rustd/crates/afd_approval/src/inbox.rs` | EDIT | `KIND_INTEGRATION_GRANT` moves to `request.rs` and is imported here — one spelling for the raise and the resolve (RULE UFS). |
| `rustd/crates/afd_approval/src/error.rs` | EDIT | Two `#[from]` variants the mint needs: entropy that would not answer, and an identifier that would not encode. |
| `rustd/crates/afd_approval/src/lib.rs` | EDIT | The new module and its public surface. |
| `rustd/crates/afd_approval/Cargo.toml` | EDIT | `afd_crypto`, for the entropy the three minted identifiers are drawn from. |
| `rustd/crates/afd_fleet_lifecycle/src/install/grants.rs` | CREATE | Install's half: classify the bundle's declared credentials, ask for one grant per mintable one. A file of its own because `install.rs` had three lines of headroom. |
| `rustd/crates/afd_fleet_lifecycle/src/install/rollback.rs` | CREATE | The rollback lifted out of `install.rs` to make room, unchanged in behaviour — LENGTH GATE, 347+6=353. |
| `rustd/crates/afd_fleet_lifecycle/src/install.rs` | EDIT | Calls the request after the activation flip, and sheds the rollback. |
| `rustd/crates/afd_fleet_lifecycle/src/lib.rs` | EDIT | `Fleets` gains the vault, the connector registry and the grant surface; `Fleets::new` takes the Key Encryption Key. |
| `rustd/crates/afd_fleet_lifecycle/Cargo.toml` | EDIT | `afd_approval` and `afd_credential`; `serde_json` for the fixture that seals a handle. |
| `rustd/crates/afd_fleet/src/lease/deliver.rs` | EDIT | The `Ungranted` arm requests before it parks, and `answers()` decides park-or-end. |
| `rustd/crates/afd_fleet/src/lease/pull.rs` | EDIT | The lease `Plane` gains the grant surface the park writes through. |

| `rustd/crates/afd_fleet/Cargo.toml` | EDIT | `afd_approval`, so the lease path writes through the table's owner. |
| `rustd/crates/afd_core/src/event.rs` | EDIT | `label::GRANT_DENIED` — a standing refusal is not one action refused, and the remedies differ. |
| `rustd/crates/agentsfleetd/tests/integration_grant_denial.rs` | CREATE | Public lease-path denial and terminal-redelivery acknowledgment regressions, registered in `tests/daemon_suite.rs`. |
| `rustd/crates/agentsfleetd/src/plane.rs` | EDIT | The composition root: the KEK into `Fleets`, the grant surface onto the lease plane. |
| `rustd/crates/afd_approval/src/request/tests.rs` | CREATE | `settle`'s five arms as unit tests, added at REVIEW: the fail-safe arm — an unrecognised status must never read as a person's no — is unreachable from an integration test, and `Denied` is the one answer that ends a delivery. Sibling module because inline would put `request.rs` past the 350 cap. |
| `rustd/crates/afd_approval/tests/integration_grant_card_uniqueness.rs` | CREATE | Live constraint, concurrent request, resolution/expiry release, and failed-card rollback regressions. |
| `rustd/crates/afd_approval/tests/integration_grant_request.rs` | CREATE | The request against a live table, and the approve arm it finally reaches. |
| `rustd/crates/afd_approval/tests/integration_grants.rs` | EDIT | Imports the production reason string rather than restating it. |
| `rustd/crates/afd_approval/tests/approval_suite.rs` | EDIT | Registers the new suite. |
| `rustd/crates/afd_fleet_lifecycle/tests/integration_install_grants.rs` | CREATE | Install requests a grant per mintable credential, and none for a static one. |
| `rustd/crates/afd_fleet_lifecycle/tests/support/lane.rs` | EDIT | A fixture Key Encryption Key, and the seeding half split out — LENGTH GATE, 344+66=410. |
| `rustd/crates/afd_fleet_lifecycle/tests/support/lane_seed.rs` | CREATE | The fixture writes, including the sealed handle the classifier opens. |
| `rustd/crates/afd_fleet_lifecycle/tests/lifecycle_suite.rs` | EDIT | Registers the new suite. |
| `rustd/crates/afd_fleet/tests/support/fleet_report_seed.rs` | EDIT | The composed test plane gains the same field the daemon's did. |
| `rustd/crates/afd_api/tests/harness/{fleet,fleet_seams,instance}.rs` | EDIT | The API harnesses pass the fixture key into `Fleets::new`. |
| `rustd/Cargo.lock` | EDIT | The three new intra-workspace edges. |
| `docs/v2/done/M193_001_P0_INFRA_UI_LIVE_ACCEPTANCE_WALK_AND_VERDICT.md` | EDIT+MOVE | R3 and R4 already read `MOVED to M194_001`; this stream closes the park — `Status: DONE`, Dimensions 2.2 and 3.2 `MOVED`, spec into `done/` — because `orly gate work` permits one spec in `active/` (`gates.ts:111`) and a parked spec there blocks every successor. Indy's call at CHORE(open), recorded in both Discovery logs. |
| `audits/doc-read.sh` · `AGENTS.orly.md` · `.oracle/orly.json` | EDIT | `orly update` to 0.10.7, folded in on Indy's call — the same shape M193_001 recorded when a harness update rode a stream. The upgrade is this session's own finding shipped back: `check` now counts cited sections and names a bulk second, and it flags this stream's nine-in-one-second record on sight (Discovery, Sep 09). Run `--no-hooks`: `core.hooksPath` is shared across worktrees and points at the base checkout, and the two hooks are the repository's own committed files. |
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

### §1 — Install requests what the bundle declares — DONE

The provenance the codebase already assumes. A fleet installed from a bundle declaring mintable credentials leaves install with a pending grant and a card per credential, so the first delivery finds a decision already waiting rather than discovering the need at run time.

- **Dimension 1.1** — DONE — installing a fleet whose bundle declares a mintable credential writes one `core.integration_grants` row per credential with status `pending` and the reason `Declared by the fleet bundle at install` → Test `install_requests_a_grant_per_declared_mintable_credential`
- **Dimension 1.2** — DONE — a non-mintable declaration (a `static` handle, or any name absent from `DECLARED`) requests nothing, because `mintable()` short-circuits and there is no grant to want → Test `a_non_mintable_declaration_requests_no_grant`
- **Dimension 1.3** — DONE — installing the same bundle twice writes one row per (fleet, service), carried by the table's `UNIQUE (fleet_id, service)` constraint rather than by a read-then-write → Test `a_repeated_request_does_not_duplicate_the_grant`, paired with `a_second_install_asks_for_its_own_fleets_grant`

### §2 — The card carries what the approve statement reads — DONE

`RESOLVE_GATE` matches `g.service = r.evidence->>'service'` (`afd_approval/src/sql.rs:98`). A card missing that key resolves cleanly and leaves the grant `pending` — a failure with no error, which is the class this milestone exists to end.

- **Dimension 2.1** — DONE — the raised gate carries `gate_kind = "integration_grant"` and `evidence` containing a `service` key spelled exactly as `Connector::name()` → Test `a_request_writes_the_grant_and_the_card_together`
- **Dimension 2.2** — DONE — approving the card moves the grant to `approved` with `approved_at` set, in the same statement that resolves the gate → Test `approving_the_card_grants_the_integration`
- **Dimension 2.3** — DONE — the gate is raised WITHOUT an `event_id`, so `Inbox::won` lands no continuation event and the still-leasable original delivery is what runs; the work runs once, not twice → Test `a_request_writes_the_grant_and_the_card_together` — the card's `event_id` is NULL

### §3 — A park is a question, never a silent loop — DONE

The backstop for any fleet installed before §1, and the invariant that makes the failure mode impossible to reintroduce.

- **Dimension 3.1** — DONE — a delivery reaching `Assembled::Ungranted` requests the grant and raises the card before returning `no_work` → Test `a_request_writes_the_grant_and_the_card_together`, paired with the unit `every_answerable_outcome_leaves_the_delivery_leasable`. The arm's own live-poll assertion was written and reached its card claims against live Postgres before being removed on Indy's call — it required a 93-line `test-util` seam in `afd_fleet` to reach a private arm, and the crate is the one the repository already split `afd_fleet_lifecycle` out of. See Discovery, Sep 09.
- **Dimension 3.2** — DONE — a delivery that redelivers every second raises exactly one card, guarded by a pre-insert `NOT EXISTS` on (fleet, gate kind, pending status, `evidence->>'service'`) rather than by a rate limit — keyed on the SERVICE and not the event, because Invariant 5 forbids these cards from carrying one (Discovery, Sep 09) → Test `a_redelivering_park_raises_one_card_not_one_per_second`
- **Dimension 3.3** — DONE — a denied grant ends the parked event with a terminal row naming the denial, so it stops redelivering; before this, `RESOLVE_GATE`'s `granted` arm moved only the grant's status and the event re-parked forever → Test `denying_the_card_revokes_the_grant`, paired with the unit `a_denied_grant_is_the_only_outcome_that_ends_the_event`. The live terminal-row assertion was written, PASSED against live Postgres, and removed with the seam — same call, same Discovery entry. The write itself is `Plane::refused`, which every other refusal on the lease path already uses.

### §4 — The walk finishes — IN_PROGRESS (needs a deployed build carrying §1–§3)

The rows inherited from M193_001. They are graded by re-running the acceptance walk against a build carrying §1–§3, not by unit tests.

- **Dimension 4.1** — `playbooks/operations/acceptance/001_playbook.md` steps 1 through 7 each reach their stated observation against the deployed build → Test `every playbook step reaches its stated observation`
- **Dimension 4.2** — a verdict file for that build records `pass` and `01_verdict_check.sh` accepts it → Test `the verdict check accepts the recorded verdict`
- **Dimension 4.3** — steps 4, 5 and 6 are captured as images, each named for the assertion it carries rather than the page it shows. Inherited whole from M193_001 Dimension 2.2, which reads `MOVED` there: steps 4 and 5 were photographed, and step 6 has no image because the walk never reached it. The visual evidence moves with the rubric row that depends on it → Test `the three state-change steps are photographed`

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
| Two cards for one park | The redelivery raises a second gate | The pre-insert `NOT EXISTS` on (fleet, gate kind, pending status, service) finds the first; the second raise is a no-op and the poll answers no-work as before |
| The work runs twice | The gate carried an `event_id`, so approval lands a continuation beside the still-leasable original | The gate is raised without one (Dimension 2.3); a continuation-bearing gate for this kind is a test failure |
| A denied grant loops forever | Denial moves the grant's status and leaves the event leasable | The denial writes the event's terminal row; the delivery stops and the operator reads why |
| The grant row exists but no card does | A crash between the two writes | Both writes land in one transaction; a park with a grant and no card is impossible rather than merely unlikely |
| An install-time request for a credential the workspace never connects | The bundle declares it; nobody connects it | The card stays pending and visible. This is correct: the fleet genuinely cannot run, and now says so |

## Invariants

1. A delivery that parks on a missing grant has a corresponding approval card — enforced by the two writes sharing one transaction, and by Dimension 3.1's test over the park path.
2. One park raises one card — enforced by the pre-insert `NOT EXISTS` on (fleet, gate kind, pending status, `evidence->>'service'`), not by a rate limit, and asserted over ten consecutive polls.
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
| 1.1 | integration | `install_requests_a_grant_per_declared_mintable_credential` | Install a bundle declaring `github` → one `integration_grants` row, status `pending`, reason `Declared by the fleet bundle at install`, and one card naming `github` |
| 1.2 | integration | `a_non_mintable_declaration_requests_no_grant` | A bundle declaring `elastic`, stored as a `static` handle → zero grant rows and zero cards. Integration rather than unit as first drafted: the short-circuit is `mintable()` reading a SEALED handle, and only a real vault open decides it |
| 1.3 | integration | `a_repeated_request_does_not_duplicate_the_grant` | Request twice on one fleet → exactly one row for (fleet, `github`), the second answering `Pending`. Paired with `a_second_install_asks_for_its_own_fleets_grant`: install twice → two fleets, one row each |
| 2.1 | integration | `a_request_writes_the_grant_and_the_card_together` | The gate row → `gate_kind = "integration_grant"`, `evidence->>'service' = "github"` |
| 2.2 | integration | `approving_the_card_grants_the_integration` | Approve → grant status `approved`, `approved_at` set, in the statement that resolved the gate |
| 2.3 | integration | `a_request_writes_the_grant_and_the_card_together` | The raised card's `event_id` is NULL → `Inbox::won` takes its `(true, None)` arm and lands no continuation beside the still-leasable delivery. The end-to-end "one lease issued" claim is graded by §4's walk |
| 3.1 | integration | `a_request_writes_the_grant_and_the_card_together` | The park's write produces the pending card. Paired with the unit `every_answerable_outcome_leaves_the_delivery_leasable`, which holds the delivery leasable. The live-poll assertion reached its card claims and was removed with the `test-util` seam on Indy's call — see Discovery |
| 3.2 | integration | `a_redelivering_park_raises_one_card_not_one_per_second` | Ten consecutive requests → one card, one grant row, `Raised` then nine `Pending` |
| 3.3 | integration | `denying_the_card_revokes_the_grant` | Deny → grant `revoked`, and a re-request answers `Denied` rather than raising a second card. Paired with the unit `a_denied_grant_is_the_only_outcome_that_ends_the_event`, which turns `Denied` into the terminal row. The live terminal-row assertion PASSED before removal with the seam — see Discovery |
| 4.1 | e2e | `every playbook step reaches its stated observation` | Walk `001_playbook.md` steps 1–7 on the deployed build → each "you must see" observed |
| 4.2 | integration | `the verdict check accepts the recorded verdict` | `01_verdict_check.sh {sha}` → exit 0 and the `✓ verdict for …: pass` line |
| 4.3 | e2e | `the three state-change steps are photographed` | Steps 4, 5, 6 on the deployed build → three images, each captioned with its assertion |

Regression rows: `a_non_mintable_declaration_requests_no_grant` is the regression guard — it fails if the request path ever widens past `declared.mintable()`, which would raise cards nobody can act on for `grafana`, `datadog` and `fly`. Idempotency rows: `a_repeated_request_does_not_duplicate_the_grant` and `a_redelivering_park_raises_one_card_not_one_per_second` are the idempotency pair, one per write site.

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

| Sep 09, 2026 | The one-card guard keys on the SERVICE, not the event | §3 Dimension 3.2 words the guard as a pre-insert lookup on `(fleet, event, gate kind)`. Invariant 5 forbids these cards from carrying an `event_id` at all — a continuation event beside a leasable delivery runs the work twice — so an event-keyed guard would compare NULL to NULL and suppress nothing. `REQUEST_GRANT` guards on `(fleet_id, gate_kind, status, evidence->>'service')` instead, which is the same claim over the column this card actually carries, and `schema/811`'s own comment already named this row's case: "NULL for gates raised outside an event (install-time integration grants)". Recorded rather than silently diverged. |
| Sep 09, 2026 | Install opens a credential handle, so `Fleets` takes the Key Encryption Key | A bundle declares credential NAMES; only the stored handle's `integration` field says which of them must be minted, so §1 cannot classify without opening an envelope. Three shapes were weighed: a trait seam in `afd_fleet_lifecycle` (no key in the crate, but an adapter at five composition points); the install handler orchestrating it (no key, but a domain invariant in the HTTP layer, where a second install door would miss it); and the lease plane's own shape — a store holding `Vault` + `Registry` — repeated here. The third won on prior art: `afd_fleet::lease::Plane` already holds exactly that pair for exactly this classification (`pull.rs:83,98`), and `Fleets::new`'s own note argues for taking the connection and building its views rather than taking the views. The plaintext never escapes `wanted_by`: the mintable half is a name and a connector id, and the `Declared` carrying the workspace's secrets is dropped where it was built. |
| Sep 09, 2026 | A denial ends the event on the PARK path, not at resolve | Dimension 3.3 needs a denied grant to end its parked event. The gate that carried the denial holds no `event_id` (Invariant 5), so `afd_approval` cannot know which event to end — the resolve is structurally unable to do it. `IntegrationGrants::request` therefore answers what it FOUND (`Raised` · `Pending` · `Approved` · `Denied`), read from the pre-statement snapshot, and the lease path's `answers()` turns `Denied` into the terminal row. One round trip decides both "raise or suppress" and "wait or end". |

| Sep 09, 2026 | Indy — keep it simple: no `test-util` seam in `afd_fleet` | The ungranted arm is private and its parameter carries a `Fence` whose constructor is `pub(crate)` by design, so reaching it from the suite took a 93-line feature-gated seam. What the two reached, precisely: `a_denied_grant_ends_its_parked_event` PASSED against live Postgres, asserting the terminal row and the absent second card. `an_ungranted_park_raises_a_card` got as far as its card assertions — the pending row and the NULL `event_id` both held — and then FAILED on a wrong assertion of mine about the poll's payload (`no_work` is a log event, not a wire field). The corrected assertion compiled but was never run to completion, so that test is recorded as unproven rather than as a pass. Surfaced with the trade named — the seam is weight in the crate the repository already carved `afd_fleet_lifecycle` out of, and the alternative was widening `Fence::from_i64` to `pub`, which trades a documented invariant for test convenience. Indy: "Yes keep it simple". Seam and both tests removed; the `&Acquired` narrowing they prompted is kept, because a function taking what it reads is better regardless. §3's live proof now rests on §4's walk. |

| Sep 09, 2026 | DOC READ GATE passed green over nine documents I had not read | `audits/doc-read.sh log` was run in one `for` loop over all nine triggered documents before most of them were opened; `check` reports 🟢 and the pre-commit hook would too. The record proves it — nine entries, one timestamp, `1788920414`. The script's own header predicts the failure ("a claim about itself compared against nothing"); `log` was built to close it, and a bulk loop reopens it. What was actually skipped: `dispatch/write_rust.md` §Functional design and §Constant discipline, and the Microsoft Pragmatic Rust Guidelines the same file names as the review reference. Two RULE FN-RS violations reached the diff as a result — a `mut` accumulator simulating an expression, and three fallible mints interleaved inside a 17-`bind` chain — both found and fixed only when Indy asked. The load-bearing step is not the record, it is the `📖 DOC READ: <path> — §N applied: <consequence>` proof-line: naming a section and what it changed cannot be produced by a loop. Not fixable in this repository — `doc-read.sh` is materialised by the orly pack, and `check` requiring a cited non-empty section, or flagging N records sharing one timestamp, is a change to that pack behind its own gates. Raised with Indy, deferred pending their call. |

| Sep 09, 2026 | REVIEW — four findings, all fixed | An adversarial pass over the three-commit diff plus the Microsoft Pragmatic Rust Guidelines read the review reference mandates. **(1)** `deliver.rs` dropped `request()`'s error with `.ok()`, and `report()` logs only `Raised`/`Pending` — so a Postgres or entropy failure produced no line, no `error_code`, and a `no_work` byte-identical to a healthy park, redelivering every second. That is this milestone's own headline bug reproduced on the path that ends it; a `written()` helper now reports before the error is dropped, and the fail-safe decision is unchanged. **(2)** `REQUEST_GRANT`'s doc claimed the unique constraint held under concurrency; it covers the GRANT row and `core.fleet_approval_gates` had none, so two same-instant requests each passed a snapshot-scoped `NOT EXISTS` and raised two cards. Slot 836 + `ON CONFLICT` now hold it. **(3)** `settle` carried the state machine untested while `deliver.rs` unit-tested its sibling `answers`; the unrecognised-status fail-safe was unreachable from any integration test. **(4)** `EVIDENCE_SERVICE` and `REASON_WANTED_BY_A_DELIVERY` were `pub` with no consumer anywhere, including the test crates. Guidelines applied: M-FROM-ERROR, M-DOCUMENTED-MAGIC, M-SINGLE-ITEM-PATH, M-TAUTOLOGICAL-TESTS, M-PARAMETER-CONSISTENCY, Redact-Sensitive-Data. Diverged and named: M-LOG-STRUCTURED's `<component>.<operation>.<state>` event naming — the repository spells `event = <snake_case>` everywhere (`install_rolled_back`, `gate_sweep_row_unreadable`, `EVENT_LEASED`), and converting one module alone splits the convention. |
| Sep 09, 2026 | Indy — the partial unique index, not a doc walk-back | Finding (2) had three fixes: slot 836, an advisory lock in an explicit transaction, or narrowing the claim and living with the race. Surfaced with the cost of each, and that only the index leaves the spec's declared Files-Changed scope. Indy chose the index, and asked whether there was already a standard approach for `ON CONFLICT` — there is, and copying it mattered: `afd_runner/src/sql/sweep.rs:61-66` pairs `ON CONFLICT (cols) WHERE <predicate verbatim> DO NOTHING` with `uq_runner_events_runner_id_dedup_key_offline`, whose own comment carries the RULE STS carve-out for a literal in an index predicate. Slot 836 is that shape, not an invented one. Two gates caught what the author did not: `clippy::panic` is denied and the new test file expected only `expect_used` (restructured to `assert!` rather than widening the suppression), and `afd_db::migrations::test_migration_list_matches_schema_directory_and_zig` found a **third** registration site — `crates/afd_db/src/migration.rs` — missing 836 while `schema/` and `embed.zig` had it. |

- **Metrics review** — `grant_requested` and `grant_request_suppressed` are new operator signals, declared above with their properties and privacy guard. No analytics or funnel playbook update: no product funnel step is added.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results, populated as the work proceeds.
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

- **Follow-up decision:** User: "fix using the approach you said using active-grant-id." This supersedes the partial-index decision above: `active_grant_id` is nullable and unique, set only on an actionable grant card and cleared atomically on approve, deny, or expiry. The earlier claimed STS partial-index exception was incorrect. Grant creation and card creation now share an explicit transaction; no-op conflict updates are avoided.
- **Greptile P1:** Terminal refusal must acknowledge Redis after the durable write, including `Ended::Already` retries. Public daemon regressions exercise an already-connected workspace, repeated parks, denial, subsequent event progress, and a terminal write whose acknowledgment was lost. No private `Plane`/`Fence` test seam is added. Acceptance §4 remains unfinished and IN_PROGRESS under the existing override.

- **Verification correction:** No-work retains its affinity claim until `LEASE_TTL_MS`; an immediate HTTP re-poll does not reach `ungranted`. The daemon regressions expire the fixture claim between polls while leaving Redis unchanged. With acknowledgment omitted, both fail specifically on the pending entry; with it restored, both pass. This corrects the earlier assertion that every one-second poll writes a grant request: every poll still does selection work, but the grant request requires a claim win.

- **Follow-up verification (Sep 09):** `make test-coverage-rustd` passed: 2,931 tests, 37,865/38,845 lines (97.4772%). Intersecting its LCOV report with added Rust lines against the PR merge base measures 262/269 (97.3978%) patch coverage. Both public denial regressions pass; omitted-acknowledgment mutation makes both fail on the pending Redis entry. `make test-unit-all`, `make lint-all`, `make check-version`, and staged `make harness-verify` also passed. Final coverage log: `/private/tmp/m194-active-grant-coverage-final.log`; prior failed attempts are not counted as evidence. Remote CI must independently confirm the pushed revision.
