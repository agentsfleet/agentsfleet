<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M219_001: The sandbox, fence, connector, egress, Slack and token boundaries hold in code

**Prototype:** v2.0.0
**Milestone:** M219
**Workstream:** 001
**Date:** Oct 09, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — boundaries the architecture pages promise to operators and tenants, read against code and found open
**Categories:** API (the Rust daemon and runner crates), DOCS (the architecture pages whose claims these fixes make true)
**Batch:** B1 — every Section is independent of the others; §7 and §8 share one test file
**Branch:** docs/event-runtime-positioning
**Baseline revision:** 2bf456efdf7be559b6c67a2e254dc92600d257b2
**Test Baseline:** unit=4485 integration=899 — Rust unit 4485 passed, 0 failed, 924 ignored; Rust integration 897 + 2 exclusive passed at `2bf456efd`; the TypeScript lanes and the branch counts are in the evidence report
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M219_001-2bf456efd.md`
**Depends on:** none. It ships in the `docs/event-runtime-positioning` Pull Request (PR) beside the positioning work, at Indy's direction; that branch's first commit predates this spec, so `spec.ordering` needs Indy's override at the PR gate.
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 09, 2026) from the `/review` of `docs/event-runtime-positioning` at `bbf220b27`: the adversarial and red-team passes, then five verification agents that read each finding's code path at that revision.
**Canonical architecture:** `docs/architecture/runner_execution.md` §Isolation, `docs/architecture/connectors.md`, `docs/AUTH.md`

---

## Overview

**Goal (testable):** Each boundary an architecture page states holds against a caller that obeys the wire format but not the happy path: a superseded runner, a racing Disconnect, an unlisted request field, model-written markup, an unrequested token permission.
**Problem:** Six boundaries are weaker in code than on the page. A host process sharing the sandbox user can read the runner's credential from the launcher. A runner that lost a fleet can still reach its memory and mint its tokens. A Disconnect racing a reconnect can strand routing rows or a live handle. Locked GitHub write rules admit fields and query strings they never name. A final Slack answer renders model-written markup. Token verification accepts any unrequested read.
**Solution summary:** Six local fixes, one per boundary, each with a test that fails on today's code, plus two money fixes: a stopped fleet's run keeps its ceiling (§7), and a catalogue fault charges tokens late instead of never (§8). No schema change, no endpoint change; one additive wire field (§4).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: position agentsfleet as an event runtime, and close the boundaries its docs claim
- **Intent (one sentence):** What the docs promise about isolation, fencing, connectors, egress, Slack and tokens becomes what the code enforces, each proven by a test that fails today.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/standing.rs` — `fence_holds`, the one fence rule report and renew already use; §2 routes memory and minting through it.
2. `rustd/crates/afd_vault/src/write.rs` — `create_in` on a caller's transaction and the note that vault and connector rows share one Postgres; §3 mirrors it for delete.
3. `rustd/crates/afd_gate/src/policy/egress/write.rs` — the module note on why objects are open and refs are locked; §4 narrows "open" for commits and records why.
4. `rustd/crates/afd_outbound/src/interim.rs` — `literal` and `SLACK_ENTITIES`, the escaping §5 moves to the poster.
5. `docs/AUTH.md` — token minting and credential verification invariants; §2 and §6 change both.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` | EDIT | The launcher starts with an empty environment |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/tests/{prepare.rs,support.rs}` | EDIT | Empty-environment test, over a fake launcher that reports its inherited environment |
| `rustd/crates/afd_fleet/src/lease/fence.rs` | EDIT | Fence reads return the lease's own token beside the live sequence |
| `rustd/crates/afd_fleet/src/lease/memory.rs`, `rustd/crates/afd_fleet/src/error/{detail.rs,refuse.rs,mod.rs}` | EDIT | Capture, recall and hydrate decide through `fence_holds`; the stale-fence sentence and fenced event name every verb |
| `rustd/crates/afd_fleet/src/lease/sql/lease.rs` | EDIT | The mint scope read requires the lease to hold the live sequence |
| `rustd/crates/afd_fleet/tests/{integration_memory_capture.rs,integration_memory_hydrate_order.rs,fleet_suite.rs}` | EDIT, CREATE | The test admitting a token above the live sequence is inverted; superseded-lease cases; hydrate trusts the higher token over the clock |
| `rustd/crates/afd_fleet/tests/{integration_credential_mint.rs,integration_credential_mint/cases.rs}` | EDIT | A superseded active lease mints nothing; the fixture lease's token is a named constant, and a helper moves the sequence past it |
| `rustd/crates/afd_fleet/src/lease/fence/tests.rs` | CREATE | The fence rule and its corrupt-column refusal, without a database |
| `rustd/crates/afd_vault/src/{delete.rs,error.rs}` | EDIT | `delete_in` on a caller's transaction; `delete` wraps it; a referenced-refusal sample behind `test-util` |
| `rustd/crates/afd_connector/src/{grant/holding.rs,error.rs,error/raise.rs}`, `rustd/crates/afd_connector/{Cargo.toml,tests/error_surface.rs}` | EDIT | `forget` is one transaction; its "different stores" note is corrected; a referenced refusal carries its count |
| `rustd/crates/afd_connector/src/grant.rs` | EDIT | `land` writes the vault row before routing, as its module note already says |
| `rustd/crates/afd_connector/tests/{integration_connect_roundtrip.rs,integration_connect_roundtrip/disconnect.rs}` | EDIT, CREATE | Refused-delete and racing-reconnect cases, holding the production workspace lock; the fixture exposes its tenant and a grant store |
| `rustd/crates/afd_api_tenant/src/handler/{connector/status.rs,secret.rs}`, `rustd/crates/afd_api/tests/{integration_connector_status.rs,integration_connector_status/checks.rs}`, `rustd/crates/afd_api_runner/src/handler/runner/credential.rs`, `rustd/crates/afd_core/src/problem/request.rs` | EDIT | A referenced Disconnect answers 409 with `current_state`; Disconnect, mint and vault delete document their 409s; the `UZ-VAULT-004` sentence fits both verbs |
| `rustd/crates/afd_wire/src/{policy.rs,policy/repository.rs}` | EDIT | `HttpRequestRule` gains serde-defaulted `permitted_fields`; named field constants and the commits path |
| `rustd/crates/afr_egress/src/{origin.rs,origin/tests.rs,origin/tests/closed.rs,admission.rs,admission/tests.rs,fixture.rs}` | EDIT, CREATE | A closed rule admits only its keys and no query, and names the key it refused, escaped and capped; the fixture permits what the gate does |
| `rustd/crates/afd_gate/src/policy/egress/{write.rs,read.rs,tests.rs}` | EDIT | Permitted fields per endpoint, commits name theirs, read rules name none; the open set is blobs and trees |
| `rustd/crates/afr_agent/src/{loop/history_tests.rs,prompt/tests.rs}` | EDIT | Rule literals carry the new field |
| `public/openapi.json` | EDIT | `HttpRequestRule` publishes `permitted_fields`; regenerated by `agentsfleetd openapi` |
| `rustd/crates/afd_outbound/{src/slack.rs,src/interim.rs,tests/integration_slack_poster.rs}` | EDIT | The poster escapes every answer; interim lines hand raw text to it; an answer naming the channel posts as text |
| `rustd/crates/afd_outbound/src/{slack/escape.rs,slack/escape/tests.rs,interim/tests.rs}` | CREATE, DELETE | `literal` and `SLACK_ENTITIES` move under the poster with their unit tests |
| `rustd/crates/afd_credential/src/credential/{github.rs,github/tests.rs,github/tests/unrequested.rs}` | EDIT, CREATE | Only `metadata: read` may arrive unrequested; unrequested-read and `metadata` level cases |
| `rustd/crates/afd_fleet/src/lease/{coverage.rs,installed.rs,sql/fleet.rs}` | EDIT | §7: the ceiling read is a stored-config read with no status filter |
| `rustd/crates/afd_fleet/src/lease/{renew.rs,renew/tests.rs}` | EDIT, CREATE | §8: a catalogue fault meters zero tokens and logs the held counts |
| `rustd/crates/afd_fleet/tests/{integration_renew_coverage.rs,integration_held_sandbox.rs,integration_held_sandbox/report.rs}` | EDIT | §7: killed-fleet ceiling cases replace the stopped-fleet case; §8: the late-charge case and the report-time uncharged-token line |
| `rustd/crates/afd_fleet/tests/support/{fleet_report_seed.rs,fleet_fixtures.rs}` | EDIT | §8: `held_in` over a private database, which can take its catalogue offline |
| `scripts/model-library-allowlist.json` | EDIT | §9: current lineups and first-party rates, verified Oct 09, 2026 |
| `make/bench.mk` | EDIT | Bench lanes reset and migrate only a rig whose `BENCH_TARGET_OWNED` is exactly `owned` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/known-models.ts`, `ui/packages/app/tests/{provider-model-select,models-registry-edit-remove}.test.tsx` | EDIT | §9: the dashboard's fallback list names current models |
| `docs/architecture/connectors.md` | EDIT | Disconnect is one transaction |
| `docs/architecture/{runner_fleet.md,lease_flow.md}` | EDIT | §7: kill keeps the ceiling on a run in flight; §4: the GitHub write set names its fields |
| `docs/architecture/scenarios/{github-pr-reviewer.md,production-deploy-repair.md}` | EDIT | Token verification claim matches §6; the repair rules name their fields (§4) |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | §7: the ceiling read ignores status; §8: renewal pricing during a catalogue fault |
| `docs/v2/active/M219_001_P1_API_DOCS_CLAIMED_BOUNDARIES_HOLD_IN_CODE.md`, `playbooks/operations/acceptance/baselines/M219_001-2bf456efd.md` | CREATE | This spec, and its test baseline and delta report |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — One owner per resource (OWN): the vault row orders Connect against Disconnect. Distinguish error classes (ECL): a catalogue fault is not an unpriced model. Escape control characters in emission (ESC) and prompt-injection resistance from user input (PRI): §5. A test that cannot fail is not a test (TCF): every inverted pinning test is red on today's code. Test naming (TST-NAM), literals as named constants (UFS: field names in §4, the event in §8), No Legacy Retained (NLR: wrong comments corrected in the commit that fixes the code), orphan sweep (ORP).
- `.orly/dispatch/write_rust.md` — ERR-RS and UFS fire on every `*.rs` edit; reviews cite `M-STRONG-TYPES-GUARD` (§4's field sets) and `M-MOCKABLE-SYSCALLS` (§1's launcher seam).
- `.orly/dispatch/write_sql.md` — §2 changes two fence reads and the mint scope read; §3 moves a delete into a transaction.
- `docs/AUTH.md` — §2 and §6 change who may mint and what a minted token may carry.
- `.orly/docs/LOGGING_STANDARD.md` §8A — §8's warning is a literal `tracing::warn!` with a named event.
- `docs/RUST_ERROR_STANDARD.md` — no new error type; refusals reuse existing codes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | Yes | `holding.rs` and `origin.rs` grow; a file nearing 350 lines splits by concern |
| UFS / LOGGING / RUST ERR | Yes | Permitted field names are constants beside the locked ones; one event constant for §8 |
| MILESTONE-ID | Yes | No milestone identifiers in code or test names |
| SCHEMA | No | No migration; §3 reorders writes inside existing tables |
| Auth (`write_auth`) | Yes, §2 and §6 | `docs/AUTH.md` read before the edit; its minting and verification text updated in the same commit |
| SPEC TEMPLATE | Yes | `bash .orly/audits/spec-template.sh --staged` on every spec edit |

## Prior-Art / Reference Implementations

- **Fence:** `sql/report.rs` and `standing.rs` — the stored token must equal the presented one and be at or above the live sequence; §2 makes memory and minting say the same thing.
- **Transactional vault write:** `afd_vault/src/write.rs` `create_in`, already used by `land`; §3 adds its delete twin.
- **Escaping:** `afd_outbound/src/interim.rs` `literal`, already proven by `an_interim_line_naming_the_channel_posts_as_text`.
- **Environment discipline:** `afr_executor/src/server/launch.rs`, which clears the environment for every tool spawn; §1 applies it one level up.

## Sections (implementation slices)

### §1 — The sandbox launcher inherits no environment — DONE

bubblewrap starts from the supervisor's environment today, so the host-side monitor's `/proc/<pid>/environ` holds what the root runner was started with. `Parts::spawn` clears the environment before exec. bubblewrap needs none: the entry is an absolute path, and the log level already travels by `--setenv`. **Implementation default:** clear in `Parts::spawn` rather than per caller, because it is the one place every engine spawn passes through.

- **Dimension 1.1** — The launcher sees no inherited variable → Test `test_bubblewrap_starts_with_an_empty_environment` — DONE (`afr_sandbox/src/bubblewrap_engine/tests/prepare.rs`, Linux)
- **Dimension 1.2** — A configured log level still reaches the entry → Test `test_the_command_is_the_bound_runner_told_to_serve` — DONE (`afr_sandbox/src/bubblewrap/tests.rs`, existing)

### §2 — Memory and minting accept only the holder's own token — DONE

Report and renew admit a request only when the lease's stored token equals the presented one and is at or above the fleet's live sequence (`fence_holds`). Capture and recall check only `presented < live`, hydrate checks only that the lease exists, and minting checks neither. The fence reads return the lease's own token; every memory verb decides through `fence_holds`; the mint scope read joins `fleet.runner_affinity` and requires the lease's token to be at or above `fencing_seq`. The affinity row stays the authority. No wire change: the runner already presents its token.

- **Dimension 2.1** — Capture refuses a token above the live sequence → Test `test_memory_capture_refuses_a_token_that_is_not_the_holders` — DONE (`afd_fleet/tests/integration_memory_capture.rs`, live)
- **Dimension 2.2** — Capture, recall and hydrate refuse an active lease the fleet has moved past → Test `test_memory_routes_refuse_a_superseded_active_lease` — DONE (`afd_fleet/tests/integration_memory_capture.rs`, live)
- **Dimension 2.3** — Minting refuses an active lease the fleet has moved past → Test `test_mint_refuses_a_superseded_active_lease` — DONE (`afd_fleet/tests/integration_credential_mint/cases.rs`, live)
- **Dimension 2.4** — A fence read with a negative column is a corrupt sequence, never a fence → Test `a_negative_column_is_a_corrupt_sequence` — DONE (`afd_fleet/src/lease/fence/tests.rs`)

### §3 — Disconnect commits both stores or neither — DONE

`forget` deletes routing rows in one commit and the vault handle in another; `land` writes routing before the vault row. Both stores are one Postgres. `afd_vault` gains `Directory::delete_in` on a caller's transaction. `forget` runs one transaction: vault delete first, routing rows second, then commit. `land` writes the vault row before routing. Both paths first lock the workspace row (`core.workspaces`, `FOR NO KEY UPDATE`), so they take turns even on a first connect, when no vault row exists yet to lock. **Implementation default:** no advisory lock (Indy, M187_001: "Why do you need the advisory lock"); the workspace row exists before either write.

- **Dimension 3.1** — A refused vault delete leaves the routing rows in place → Test `a_disconnect_whose_vault_delete_is_refused_keeps_its_routing_rows` — DONE (`afd_connector/tests/integration_connect_roundtrip/disconnect.rs`, live)
- **Dimension 3.2** — A reconnect or a first connect racing a Disconnect waits its turn, then ends with routing rows exactly when a handle exists → Test `a_reconnect_racing_a_disconnect_leaves_both_rows_or_neither` with `a_disconnect_racing_a_first_connect_waits_its_turn` — DONE (`afd_connector/tests/integration_connect_roundtrip/disconnect.rs`, live)

### §4 — A locked GitHub write rule admits only what it names — DONE

The matcher compares method, path and locked fields, and passes any other body key and any query string. `HttpRequestRule` gains `permitted_fields`. A rule that names fields admits a body whose top-level keys are all locked or permitted, and a URL with no query. `/pulls` permits `title`, `body` and `maintainer_can_modify` beside its three locked fields; `/git/refs` permits `sha` beside `ref`; `/git/commits` permits `message`, `tree` and `parents`, so GitHub attributes every commit to the App. **Implementation default:** commits leave the "objects are open" set, because a ref publishes the commit's stated identity. An older runner ignores the new field and stays as open as today; runners deploy with the daemon. A newer runner reading a lease minted before the field existed checks only that lease's locked fields, as that daemon meant.

- **Dimension 4.1** — An unlisted key or a query string under a locked rule is refused → Test `should_refuse_an_unlisted_key_or_a_query_under_a_locked_rule` — DONE (`afr_egress/src/origin/tests/closed.rs`)
- **Dimension 4.2** — A commit naming its own author, committer or signature is refused → Test `should_refuse_a_commit_that_names_its_own_identity` — DONE (`afr_egress/src/origin/tests/closed.rs`)
- **Dimension 4.3** — The draft Pull Request a binding authorises is still admitted → Test `should_admit_the_draft_pull_request_a_binding_authorises` — DONE (`afr_egress/src/origin/tests/closed.rs`)
- **Dimension 4.4** — The gate authors exactly the fields each locked rule permits, and only blobs and trees stay open → Test `a_rule_that_names_a_field_lists_every_field_a_run_may_send` — DONE (`afd_gate/src/policy/egress/tests.rs`)

### §5 — A final Slack answer posts as literal text — DONE

Interim lines are escaped; final answers go out as written, so a model can notify a channel or mask a link's target. `literal` and `SLACK_ENTITIES` move into `slack/escape.rs`, a submodule of the poster, because `slack.rs` has no headroom under the length cap for them; `SlackPoster::post` escapes every answer; interim delivery hands raw text over so nothing is escaped twice. The stored copy the dashboard shows stays raw. Indy chose to fold this in on Oct 09, 2026 (Discovery). Known limit, from review cycle 2: Slack delivers inbound text with `&`, `<` and `>` already escaped and ingress hands it to the model as written, so an answer echoing thread text is escaped twice and shows a literal `&lt;`. Indy, Oct 09, 2026: "Record as a known limit saying Indy will test eyeball and then ask for a fix".

- **Dimension 5.1** — An answer carrying a channel mention and a masked link posts as entities → Test `an_answer_naming_the_channel_posts_as_text` — DONE (`afd_outbound/tests/integration_slack_poster.rs`, live)
- **Dimension 5.2** — An interim line is escaped exactly once → Test `an_interim_line_naming_the_channel_posts_as_text` — DONE (`afd_outbound/tests/integration_slack_poster/interim.rs`, live, existing)

### §6 — An installation token carries nothing unrequested but metadata — DONE

`verify_permissions` passes any unrequested permission at read level. It passes only `metadata: read`; any other unrequested name, at any level, is `Overreach`. **Implementation default:** the grant row records no scope, because a workspace member already holds `FleetWrite` (`afd_http/src/auth/ownership/role.rs` withholds only secret and connector writes) and can install a write fleet directly, so a recorded scope would close no hole.

- **Dimension 6.1** — An unrequested read other than `metadata` is refused → Test `verify_refuses_an_unrequested_read_other_than_metadata` — DONE (`afd_credential/src/credential/github/tests/unrequested.rs`)
- **Dimension 6.2** — `metadata: read` beside exactly the requested set passes → Test `verify_admits_metadata_beside_the_request` — DONE (`afd_credential/src/credential/github/tests/unrequested.rs`)
- **Dimension 6.3** — A live dev mint for the pull-request reviewer fleet passes the tightened check → Test `dev_mint_passes_the_tightened_verify` (manual) — DONE (Oct 09, 2026, dev App installation `155905462`: requested `{contents: read}`, granted `{contents: read, metadata: read}`; token discarded unprinted)
- **Dimension 6.4** — `metadata` at any level other than read is refused → Test `verify_refuses_metadata_above_read` — DONE (`afd_credential/src/credential/github/tests/unrequested.rs`)

### §7 — A stopped fleet's run keeps its ceiling — DONE

`installed()` returns nothing for a fleet that is not active, and `budget_covers` then admits the renewal with no ceiling, so a killed fleet's run renews up to `MAX_RUNTIME_MS` bounded only by the tenant wallet. A stored-config read with no status filter feeds `budget_covers`; an absent row means only a purge race. A kill still never cancels a run with room left.

- **Dimension 7.1** — A killed fleet past its ceiling is not renewed → Test `a_fleet_killed_mid_run_keeps_its_breached_ceiling` — DONE (`afd_fleet/tests/integration_renew_coverage.rs`, live)
- **Dimension 7.2** — A killed fleet with room still renews → Test `a_fleet_killed_mid_run_with_room_still_renews` — DONE (`afd_fleet/tests/integration_renew_coverage.rs`, live)

### §8 — A catalogue fault charges tokens late instead of never — DONE

A renewal whose catalogue read fails meters at run-fee rates with the real counts, so the token cursor moves past tokens charged at zero. It meters zero counts instead: the run fee is charged, the cursor stays, and the next priced renewal or the report charges the tokens. A warning names the fault. **Preparation:** find or add a catalogue fault seam the integration lane can trigger; without one, 8.1 runs at the plane with a failing catalogue.

- **Dimension 8.1** — Tokens reported during a catalogue fault are charged at the next priced renewal → Test `a_renewal_during_a_catalogue_fault_charges_its_tokens_later` — DONE (`afd_fleet/tests/integration_renew_coverage.rs`, live, private database)
- **Dimension 8.2** — The fault is logged with its event → Test `a_catalogue_fault_logs_the_held_tokens` — DONE (`afd_fleet/src/lease/renew/tests.rs`)

### §9 — The model library lists today's models at today's prices — DONE

`scripts/model-library-allowlist.json` was past its 45-day staleness limit. Each priced provider is re-verified against its first-party page: new flagships added, superseded rows moved to `retired`, wrong rates fixed (`gpt-6-astra` long-context output, DeepSeek's repricing). Tiered models stay off live-priced gateways. The seeder never deletes, so removed rows stay live as unmanaged. The dashboard's fallback list names the same models. Indy read the dev diff before the apply; production is a separate approval.

- **Dimension 9.1** — Every allowlisted provider is priced or carries a reason → Test `_model_allowlist_check` — DONE (`scripts/check_model_allowlist.py`: 100 providers, 31 priced, 69 reasoned)
- **Dimension 9.2** — Dev's catalogue equals the allowlist → Test `dev_catalogue_matches_the_allowlist` (manual) — DONE (diff after apply: `0 new · 0 changed`)
- **Dimension 9.3** — An empty catalogue still offers current models → Test `falls back to the static known-models list before free text when the catalogue has no rows for the provider` — DONE (`ui/packages/app/tests/provider-model-select.test.tsx`)

## Interfaces

```
afd_wire::policy::HttpRequestRule
  + permitted_fields: Option<Vec<String>>   // #[serde(default)]; None = open, Some([]) = locked fields only
afd_vault::Directory
  + delete_in(&self, tx: &mut Transaction, workspace, name) -> Result<Deleted>
afd_fleet (§7)  Leases::stored_config(&Uuid7) -> Result<Option<FleetConfig>>
Slack chat.postMessage body: `text` is entity-escaped for & < >; `metadata` unchanged
```

No route or error code changes. A referenced Disconnect's 409 gains `current_state: "referenced"` and the mint's `UZ-GH-001` 409 `current_state: "reconnect_required"`; Disconnect, mint and vault delete document their 409s. Refusals reuse `RUN_STALE_FENCING_TOKEN`, `lease_not_found`, `budget_exhausted` and the egress refusal codes.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale holder | A runner presents a token that is not its lease's, or holds a lease the fleet moved past | Memory verbs refuse with the stale-fence code; mint answers lease not found; nothing is read, stored or minted |
| Vault delete refused | A model entry still references the grant key | `forget` rolls back; routing rows stay; Disconnect answers 409 `UZ-VAULT-004` with `current_state: "referenced"` |
| Concurrent Connect and Disconnect | Callback and Disconnect run together | The workspace row lock serializes them; the end state has routing rows only with a handle |
| Unlisted request shape | A tool sends a field or query a locked rule does not name | Egress refuses before the request leaves the sandbox; the tool reads the refusal |
| Markup in an answer | Model output carries `<!channel>` or `<url\|label>` | Posted as entities; Slack renders the characters |
| GitHub returns another ambient read | A token response lists an unrequested read besides `metadata` | Mint refuses as overreach and logs it; 6.3 proves the live response first |
| Catalogue unavailable (§8) | The pricing read fails during renewal | Run fee charged, tokens held at the cursor, warning logged; the run continues |

## Invariants

1. Only the holder's exact, current token passes a fenced verb — every fenced read decides through `fence_holds`; R2 counts no other comparison.
2. Routing rows exist only beside a vault handle — `land` and `forget` each run in one transaction that locks the workspace row first (`sql::LOCK_WORKSPACE`).
3. A closed rule admits nothing it does not name — with `permitted_fields` present, the matcher rejects any key outside the locked and permitted sets and any query string; an absent list leaves a read, or an older daemon's rule, open.
4. Model text reaches Slack only through `literal` — the poster is the one path to `chat.postMessage`.
5. bubblewrap's environment is exactly what `--setenv` names — `Parts::spawn` clears it before exec.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `renew_tokens_held_for_pricing` (§8 only) | ops | A renewal's catalogue read fails | `fleet_id`, `lease_id`, the three held token counts, `error_code`, `reason` | Counts only: no prompt text and no prices | `a_catalogue_fault_logs_the_held_tokens` |
| All other Sections: no product or operator signal changes | not applicable | never | none | Existing refusal events keep their names | `test_memory_routes_refuse_a_superseded_active_lease` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_bubblewrap_starts_with_an_empty_environment` | A fake launcher prints an inherited marker and fails → the refusal reason carries no marker; red today |
| 1.2 | unit | `test_the_command_is_the_bound_runner_told_to_serve` | Log level set → argv carries `--setenv AGENTSFLEET_LOG_LEVEL <level>` |
| 2.1 | integration | `test_memory_capture_refuses_a_token_that_is_not_the_holders` | Live token + 1 and `u64::MAX` → stale-fence refusal, store empty; the live token stores |
| 2.2 | integration | `test_memory_routes_refuse_a_superseded_active_lease` | `fencing_seq` bumped, lease still active → capture, recall and hydrate refused |
| 2.3 | integration | `test_mint_refuses_a_superseded_active_lease` | Same setup → mint answers lease not found; the live holder still mints |
| 2.4 | unit | `a_negative_column_is_a_corrupt_sequence` | Token −1 or live sequence −1 → `INTERNAL_DB_QUERY`, no fence |
| 3.1 | integration | `a_disconnect_whose_vault_delete_is_refused_keeps_its_routing_rows` | Model entry references the grant key → `forget` errs, routing row present |
| 3.2 | integration | `a_reconnect_racing_a_disconnect_leaves_both_rows_or_neither` | Vault row (reconnect) or workspace row (first connect) held, `land` then `forget` queued behind it, released → `land` lands, `forget` answers Disconnected, routing rows present iff the handle is |
| 4.1 | unit | `should_refuse_an_unlisted_key_or_a_query_under_a_locked_rule` | `{"issue":7}` on `/pulls` and `/pulls?draft=false` → refused |
| 4.2 | unit | `should_refuse_a_commit_that_names_its_own_identity` | `author`, `committer` or `signature` on `/git/commits` → refused |
| 4.3 | unit | `should_admit_the_draft_pull_request_a_binding_authorises` | title, body, head, base, `draft:true` → admitted |
| 4.4 | unit | `a_rule_that_names_a_field_lists_every_field_a_run_may_send` | Write binding → commits permit `message`, `tree`, `parents`; refs `sha`; pulls `title`, `body`, `maintainer_can_modify`; the open POST set is blobs and trees |
| 5.1 | integration | `an_answer_naming_the_channel_posts_as_text` | Answer `<!channel> <https://x.example\|docs>` → wire text `&lt;!channel&gt; &lt;https://x.example\|docs&gt;` |
| 5.2 | integration | `an_interim_line_naming_the_channel_posts_as_text` | Interim `<!channel>` → `&lt;!channel&gt;`, not `&amp;lt;` |
| 6.1 | unit | `verify_refuses_an_unrequested_read_other_than_metadata` | Response adds `administration: read` → overreach |
| 6.2 | unit | `verify_admits_metadata_beside_the_request` | Requested set plus `metadata: read` → passes |
| 6.3 | manual | `dev_mint_passes_the_tightened_verify` | A dev pull-request reviewer lease mints; the PR records the response's permission names |
| 6.4 | unit | `verify_refuses_metadata_above_read` | Response carries `metadata: write` → overreach |
| 7.1 | integration | `a_fleet_killed_mid_run_keeps_its_breached_ceiling` | Budget exhausted, status killed → renewal refused with `budget_exhausted`; red today |
| 7.2 | integration | `a_fleet_killed_mid_run_with_room_still_renews` | Budget with room, status killed → renews |
| 8.1 | integration | `a_renewal_during_a_catalogue_fault_charges_its_tokens_later` | Counts 10k→25k under a fault, 40k priced → the second renewal charges 30k tokens |
| 8.2 | unit | `a_catalogue_fault_logs_the_held_tokens` | Fault injected → one `renew_tokens_held_for_pricing` warning |
| 9.1 | unit | `_model_allowlist_check` | The edited allowlist → `✓ [models] 100 providers — 31 priced, 69 reasoned` |
| 9.2 | manual | `dev_catalogue_matches_the_allowlist` | Indy runs the dev apply; `ACTION=diff ENV=dev` afterwards → `0 new · 0 changed` |
| 9.3 | unit | `falls back to the static known-models list before free text when the catalogue has no rows for the provider` | Empty catalogue, provider `anthropic` → `claude-sonnet-5-5` offered |

Regression: the existing memory, mint, connect-roundtrip, egress and Slack poster suites pass unchanged; `a_fleet_stopped_mid_run_still_renews_the_lease_in_flight` is replaced by 7.1 and 7.2.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The launcher clears its environment (§1) | `git grep -c 'env_clear()' -- rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` | `1` | P0 | |
| R2 | No fenced verb compares tokens by hand (§2) | `git grep -nE 'token < live\|presented < live' -- rustd/crates/afd_fleet/src` | no output | P0 | |
| R3 | No "different stores" reasoning survives (§3) | `git grep -n 'different stores' -- rustd/crates/afd_connector docs/architecture/connectors.md` | no output | P0 | |
| R4 | One escaping function, owned by the poster (§5) | `git grep -n 'fn literal' -- rustd/crates/afd_outbound/src` | one line, in `slack/escape.rs` | P0 | |
| R5 | This spec's diff stays inside Files Changed | `git diff --name-only $(git log --diff-filter=A --format=%H -- 'docs/v2/*/M219_001_*.md' \| tail -1)..HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in PR Session Notes.

## Dead Code Sweep

`literal` and its unit tests move from `interim.rs` and `interim/tests.rs` to `slack/escape.rs`; R4 proves one copy remains.

## Out of Scope

- A host user per lease, which would end the shared sandbox uid; §1 removes what that uid could read.
- Recording an approved scope on the grant row (§6 explains why it closes no hole).
- Pricing a run whose final report meets a catalogue fault; §8 covers renewals only.
- The deploy drain and its cancellation behaviour, which Indy chose to leave as they are on Oct 09, 2026.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator reads a boundary in the architecture docs, and the test named beside it fails on the old code and passes on the new.
2. **Preserved user behaviour** — Draft pull requests, memory, Connect and Disconnect, Slack answers and minting work as today for every caller on the happy path; kill still never cancels a run with room.
3. **Optimal-way check** — Each fix sits at the one place the boundary is decided, not at its callers.
4. **Rebuild-vs-iterate** — Iterate; each boundary already has an owner that is close to right.
5. **What we build** — Six fixes, their tests, and the doc lines that describe them; two more on Indy's yes.
6. **What we do NOT build** — Per-lease host users, grant scope columns, report-time repricing.
7. **Fit with existing features** — Compounds with the report and renew fence; must not destabilize the pull-request reviewer's draft flow (4.3 and 6.3 guard it).
8. **Surface order** — N/A — no new surface; Slack answers render masked links as text.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — An egress refusal names the host, method and path the run sent; a Slack answer shows the characters the model wrote.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One Section per boundary, so each fix and its test review alone; §7 and §8 last because they wait on a decision.
- **Alternatives considered:** An advisory lock for §3 (rejected: the workspace row lock orders both paths, a first connect included); a schema column for §6 (rejected: closes no hole); a separate Slack spec (rejected by Indy's fold decision).
- **Patch-vs-refactor verdict:** this is a **patch** because every boundary has a correct owner that admits too much; none needs a new layer.

## Discovery (consult log)

- **Consults** — Review of `docs/event-runtime-positioning` at `bbf220b27` (Oct 09, 2026): adversarial and red-team passes raised the findings; five verification agents read each code path and confirmed, refuted or narrowed it. Refuted: a killed fleet posting messages (`message.rs` refuses it). Narrowed: §1 is unreachable from inside the sandbox; §6 is not a cross-role escalation. Indy chose to fold the security findings into the positioning work, then on Oct 09, 2026 chose "Fold now (Recommended)" for Slack answer escaping, which M212 had left for later with the quote "slack escaping skip follow up spec, let me test and fix it later". M187_001 §3.3 (Indy, Sep 07, 2026: "Why do you need the advisory lock") still holds: §3 adds none. The premise recorded beside it, that a Postgres lock could not cover the vault write, was false, because both stores are one Postgres (`afd_vault/src/write.rs`). Agent choice with evidence: §6 verify only. Indy then chose the branch, §7 and §8 together:
  > Indy (2026-10-09 13:03): "I think fix all the M219 + 1, 2, 3 in this PR and push, so we can test" — context: 1 = this spec ships on `docs/event-runtime-positioning`; 2 = §7, a killed fleet keeps its ceiling; 3 = §8, a catalogue fault charges tokens late.
- **Metrics review** — One operator warning in §8; no analytics or funnel playbook update required, because no product event changes.
- **Skill-chain outcomes** — pending.
- **Oct 09, 2026 decisions (Indy)** — §9: "Fold into this PR"; DeepSeek at its peak rate; no FireRouter row until the platform Fireworks account carries an Anthropic key; MiniMax's permanent discount kept; Sonar rows kept with a note; "donot add Mythos as its not for public use"; "APPLY DEV". Close: "skip changelog in docs"; "yes override in orly" for `spec.ordering`.
- **Deferrals** — none.
