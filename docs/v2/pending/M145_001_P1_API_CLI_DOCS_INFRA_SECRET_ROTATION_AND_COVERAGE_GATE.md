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

# M145_001: Rotate a secret from the client without releasing its name

**Prototype:** v2.0.0
**Milestone:** M145
**Workstream:** 001
**Date:** Jul 29, 2026
**Status:** PENDING
**Priority:** P1 — `0.24.0` shipped an operator-facing hole: no client path replaces a secret value without a window where the name is absent.
**Categories:** API, CLI, DOCS, INFRA
**Batch:** B1 — standalone; no other workstream touches the vault surface.
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none — `PATCH /v1/workspaces/{workspace_id}/secrets/{secret_name}` shipped in `0.24.0`.
**Provenance:** agent-generated (pre-spec, `docs/v2/done/M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md` Discovery A15 and the standing coverage finding)
**Canonical architecture:** `docs/architecture/web_app.md` (secret surfaces) and `docs/AUTH.md` (workspace operator role on every vault route)

---

## Overview

**Goal (testable):** `agentsfleet secret rotate <name> --api-key <key>` replaces a stored key in one call, the name stays claimed for the whole call, and a secret with no `api_key` to replace is refused rather than silently given one.
**Problem:** An operator who must replace a leaked or expiring key has no safe client move. `create` refuses a name the workspace already holds (`UZ-VAULT-005`), and `delete` then `create` leaves the name absent between two calls — every fleet that requires it fails in the gap. The endpoint that does this correctly has been published since `0.24.0` and only the browser calls it.
**Solution summary:** Add a `secret rotate` command that mirrors the published PATCH route and the browser's `rotateSecret` exactly, so the client gains rotation with no new server capability. Close the two hazards that making rotation scriptable would otherwise expose. First, the route is an unlocked read-modify-write: it loads the stored object and re-stores it with an upserting statement, in two autocommit statements with nothing held between them — so a rotation racing a delete resurrects the deleted credential, and two rotations lose one another's write while both report success. Rotation moves inside the vault row lock the delete path already takes. Second, the re-store inserts `api_key` when the stored object has none, which would silently add a junk field to a custom secret, leave the real credential stale, and answer `200`; that case becomes a typed `409` decided inside the same lock. Separately, wire the Command-Line Interface (CLI) coverage floor into Continuous Integration (CI), where it has never run, and clear the gaps it finds.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(cli): rotate a workspace secret without releasing its name
- **Intent (one sentence):** An operator replaces a credential in place, and is told plainly when the secret's shape means rotation would not do what they asked.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/fleets/secrets.zig` — `innerRotateSecret` and `rotateSecretKeyOnConn` are the behaviour being wrapped; the `put` on the parsed object is the exact line §2 guards.
2. `ui/packages/app/lib/api/secrets.ts` — `rotateSecret` is the shipped caller; the CLI mirrors its body and its preservation guarantee rather than inventing a second shape.
3. `cli/src/commands/fleet_secret.ts` and `cli/src/commands/fleet_secret_body.ts` — the command style to match, and the stdin sentinel (`@-`) whose stated rationale is keeping secrets out of shell history.
4. `docs/v2/done/M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md` Discovery A15/A16 — why `--force` was retired and why creation claims a free name; this workstream completes that decision.
5. `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — typed-error and status-selection rules for the new `409`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/commands/fleet_secret.ts` | EDIT | Add the rotate effect beside create/show/list/delete. |
| `cli/src/commands/fleet_secret_rotate.ts` | CREATE | Key resolution (literal and stdin) split out so `fleet_secret.ts` stays under the length cap, mirroring the existing `fleet_secret_body.ts` split. |
| `cli/src/program/cli-tree-fleet.ts` | EDIT | Register `secret rotate <name>` with its flag. |
| `cli/src/program/handlers-bind-fleet.ts` | EDIT | Bind the handler into the `secret` group. |
| `src/agentsfleetd/http/handlers/fleets/secrets.zig` | EDIT | Rotate inside the vault row lock; refuse when the stored object carries no `api_key`. |
| `src/agentsfleetd/state/secret_reference_txn.zig` | EDIT | Expose the step-1 vault row lock for a participant that changes no reference set. |
| `src/agentsfleetd/errors/error_registry.zig`; `src/agentsfleetd/errors/error_entries.zig` | EDIT | The new code, its message, and its registry row. |
| `public/openapi/paths/secrets.yaml`; `public/openapi.json` | EDIT | Publish the new response on the PATCH operation. |
| `src/agentsfleetd/http/secrets_json_metadata_integration_test.zig` | EDIT | Negative coverage for the refusal and the preservation guarantee. |
| `cli/test/fleet-secret-rotate.unit.test.ts` | CREATE | Command-level behaviour and flag validation. |
| `cli/test/secrets.integration.test.ts`; `cli/test/acceptance/secret-vault.spec.ts` | EDIT | In-process rotation against the stub, and the live lane. |
| `.github/workflows/test.yml`; `make/test-unit.mk` | EDIT | Run the enforcing coverage script instead of the non-enforcing one. |
| `cli/test/api-key-linecov.unit.test.ts`; `cli/test/cli-linecov.unit.test.ts`; `cli/test/connectors.service.unit.test.ts`; `cli/test/coverage-fill.unit.test.ts` | EDIT | Close the floor gaps §4 turns red-blocking. |
| `~/Projects/docs/fleets/secrets.mdx`; `~/Projects/docs/changelog.mdx`; `~/Projects/docs/api-reference/error-codes.mdx` | EDIT | Document the command, correct the wrong PATCH body, announce, and regenerate the error reference. |

**Scope grading.** Rubric R2 compares `git diff --name-only origin/main` against this table. A path that proves genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition. The `cli/test/*linecov*` row names the likely homes for the coverage closures; the gaps are enumerated in §4 and the agent may place a test in the file that already owns its subject instead.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (flag strings, field names, and the new code as named constants), JCL (the rotate command's JSON-mode output shape), EMS (the new error message structure), TNM and TST-NAM (test naming), ECL (refusal is a distinct class from not-found), NDC and ORP (no helper without a caller; sweep on any rename), TFX (tests reuse production constants).
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** — the CLI diff is the bulk of this workstream: file-shape decision at PLAN, `const` and import discipline, Bun primitives.
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — the handler edit: lifecycle and `errdefer` placement, function length, cross-compile.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — status selection and typed-error shape for the new refusal.
- **`~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md`** and **`~/Projects/dotfiles/docs/CHANGELOG_VOICE.md`** — the two published pages and the changelog entry.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | `secrets.zig` gains the guard; cross-compile both linux targets; the added branch keeps `rotateSecretKeyOnConn` under the function cap. |
| PUB / Struct-Shape | yes | No new `pub` surface is intended; if the guard is extracted, the shape verdict is recorded before it is exported. |
| File & Function Length | yes | `fleet_secret.ts` sits near the cap already, which is why key resolution lands in its own file rather than inline. |
| UFS | yes | Flag literal, the `api_key` field name, the stdin sentinel, and `UZ-VAULT-006` all become named constants; the field name is shared verbatim with the existing custom-endpoint constants. |
| UI Substitution / DESIGN TOKEN | no | No User Interface (UI) files change — the browser already rotates. |
| ERROR REGISTRY | yes | `UZ-VAULT-006` lands in `error_registry.zig` and `error_entries.zig` together, with a negative test and a regenerated reference page. |
| LOGGING | yes | The refusal logs at the same level and shape as the existing rotate failures, and never logs the key. |
| LIFECYCLE | yes | §2 introduces a transaction on the rotate path: `errdefer` abort placement and a single owner for commit-or-abort, matching the delete path's existing shape. |
| SCHEMA | no | No schema change and no migration — §2 adds a lock over existing tables, not a new column. |

## Prior-Art / Reference Implementations

- **Reference (CLI):** `cli/src/commands/fleet_secret.ts` — `secret rotate` is a sibling of `create`, not a new pattern. Aligns with the "7 Pillars" of CLI developer experience on command → handler → errors split, handler purity, output as a service (human and JSON renderers chosen by the renderer, never `console.log` in the handler), structured errors carrying a `suggestion`, and the three-tier test pyramid. Divergence: none intended.
- **Reference (client call):** `ui/packages/app/lib/api/secrets.ts` `rotateSecret` — the body shape and the preservation guarantee are copied, not re-derived.
- **Reference (server refusal):** the `UZ-VAULT-005` arm added in M143_002 — same file, same typed-conflict shape, same "answer precisely rather than report a success we cannot confirm" reasoning.

## Sections (implementation slices)

### §1 — The client rotates a secret in one call

An operator replaces a stored key with `agentsfleet secret rotate <name> --api-key <key>`. The command sends one PATCH to the published route and reports the outcome; the name is never released, so a fleet requiring it keeps running across the rotation. The key may be supplied on stdin, because a credential passed as an argument lands in shell history — the same reason `create` already offers `--data=@-`.

**Implementation default:** the flag is `--api-key`, matching the endpoint field, the browser caller, and `create`'s existing typed flag. Whole-object replacement is not offered here — see Out of Scope.

- **Dimension 1.1** — a rotate call issues exactly one PATCH to the secret's item route with the key as `api_key`, and reports success naming the secret → Test `test_secret_rotate_sends_single_patch`
- **Dimension 1.2** — `--api-key=@-` reads the key from stdin, rejects empty stdin with a usage suggestion, and a missing or empty flag fails before any request is sent → Test `test_secret_rotate_key_sources_and_validation`
- **Dimension 1.3** — JSON mode emits the rotate outcome keyed on the secret name and nothing resembling the key; human mode prints no key bytes → Test `test_secret_rotate_output_modes_omit_key`

### §2 — Rotation becomes a locked read-modify-write that refuses what it cannot rotate

`rotateSecretKeyOnConn` runs `loadJson`, mutates the parsed object in memory, and re-stores it — two autocommit statements with no transaction and no row lock, where the re-store is `INSERT … ON CONFLICT DO UPDATE`. Two defects follow, and both become materially easier to hit once rotation is scriptable rather than a browser dialog:

**A rotation racing a delete resurrects the credential.** The rotation reads the row; the delete takes the vault row lock, confirms no model-registry entry references it, removes it, and answers `204`; the rotation's upsert then finds no conflicting row and **inserts**. The operator deleted a credential and it is live again under the same name, with a fresh identifier that never passed the reference check. `state/secret_reference_txn.zig` exists because this exact shape of race orphaned model entries, and rotation is the path that fix did not cover.

**Two concurrent rotations lose one write and both report success.** Last writer wins on an unlocked read-modify-write, so one operator is told their new key is live when the other's is. A rotation is usually performed *because* a key leaked, which makes a false confirmation the expensive kind of wrong.

The fix is to hold the same lock the delete path takes — `vault.secrets (workspace_id, key_name)` `FOR UPDATE`, step 1 of the protocol in `secret_reference_txn.zig` — across the load, the decision, and the write. Rotation changes no reference set, so it takes step 1 only; a prefix of the established order cannot deadlock against a full participant.

Deciding inside the lock is also what makes the shape refusal real rather than narrower. When the stored object has no `api_key` — every secret created through `secret create --data` with another shape, including the `api_token` example on our own published page — the put **inserts** instead of replacing, leaving the credential the fleet reads stale beside a junk field, and answering `200`. That case returns a typed `409` (`UZ-VAULT-006`) and writes nothing. It is a conflict with the stored resource's shape, not a bad request and not a missing secret, so it earns its own class and code.

**Implementation default:** reuse the locking statement from `secret_reference_txn.zig` rather than spelling `FOR UPDATE` at a second call site — that module's own header records that a protocol re-implemented per call site is one that eventually gets re-implemented backwards.

- **Dimension 2.1** — rotation holds the vault row lock across load, decision, and write; a rotation racing a delete resolves deterministically, never resurrecting the row → Test `test_rotate_serializes_against_delete`
- **Dimension 2.2** — rotating a secret whose stored object has no `api_key` returns `409`/`UZ-VAULT-006` and leaves the stored object byte-identical → Test `test_rotate_refuses_object_without_api_key`
- **Dimension 2.3** — rotating a secret that does have `api_key` succeeds and preserves every sibling field (`provider`, `model`, `base_url`) → Test `test_rotate_preserves_sibling_fields`
- **Dimension 2.4** — the CLI renders the refusal with a suggestion naming the concrete alternative, and exits non-zero → Test `test_secret_rotate_renders_unrotatable_refusal`

### §3 — The published pages describe what the server actually does

`fleets/secrets.mdx` currently instructs the reader to send PATCH "with the new `data`". The endpoint requires `api_key` and rejects a `data` body, so the documented call fails for anyone who follows it. The same page states the client has no rotate command, which §1 makes false.

- **Dimension 3.1** — the secrets page documents `secret rotate`, states the correct PATCH body field, and retains no claim that the client cannot rotate → Test `test_secrets_page_matches_endpoint`
- **Dimension 3.2** — the error reference carries `UZ-VAULT-006`, regenerated rather than hand-edited → Test `test_error_reference_regenerated`

### §4 — The CLI coverage floor runs where it can fail the build

`cli/bunfig.toml` declares a 100% line and function floor, and `scripts/enforce-coverage.mjs` exists to enforce it because Bun parses the threshold without acting on it. Nothing calls the script: CI's `test-unit-cli` and `make test-coverage-all` both run `test:coverage`, which only prints a table. The floor has been decorative, and `main` is below it.

**Implementation default:** point the existing CI job and the make recipe at the enforcing script rather than adding a new job or a new make target.

- **Dimension 4.1** — the CI job and the make recipe fail when coverage is below the declared floor, proven by the gaps on `main` being red before they are closed → Test `test_coverage_gate_fails_below_floor`
- **Dimension 4.2** — the floor is met: the uncovered lines in `cli.ts`, `commands/api_key.ts`, `commands/connector.ts`, `commands/fleet_install.ts`, `commands/fleet_schedule.ts` and the uncovered function in `commands/login-helpers.ts` are covered, or deleted if unreachable → Test `test_cli_coverage_floor_met`

Gaps are read from `cli/coverage/lcov.info` (`DA:` for lines, `FNDA:` for functions). The text reporter's "Uncovered Line #s" column also lists lines carrying uncovered *branches*, and reading it as a line list cost a prior session a full cycle. Where a gap proves genuinely unreachable, RULE TVR applies: delete the dead arm rather than manufacture a test for a value that cannot occur, and record it in Discovery.

## Interfaces

```
PATCH /v1/workspaces/{workspace_id}/secrets/{secret_name}     (published, unchanged shape)
  request   { "api_key": "<non-empty string>" }
  200       { "name": "<secret_name>" }
  400       UZ-* invalid request — absent/malformed body, empty api_key, bad workspace id or name
  404       UZ-VAULT-003 secret not found in this workspace
  409       UZ-VAULT-006 stored secret has no api_key field to replace   (NEW — nothing is written)
  413-class UZ-VAULT-002 resulting object exceeds the 4 KiB stored limit

agentsfleet secret rotate <name> --api-key <key|@->
  exit 0    human: one line naming the secret, no key bytes
            json: { "status": "rotated", "name": "<name>" }
  exit != 0 typed CliError carrying code + suggestion; validation failures send no request
```

Sibling-field preservation is part of this surface: rotation replaces `api_key` and nothing else. Any change to that guarantee is a spec amendment, because the browser's custom-endpoint dialog depends on `base_url` surviving a rotation.

The `404` above becomes reachable under a concurrent delete once §2 lands — a rotation that loses the race writes nothing and reports the secret gone, which is the correct answer and the one the client already renders.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unrotatable shape | Stored object carries no `api_key` | `409` `UZ-VAULT-006`; nothing written; CLI prints the refusal plus the concrete alternative and exits non-zero |
| Unknown secret | Name absent from this workspace | `404` `UZ-VAULT-003`; CLI suggests `secret list` |
| Empty key | Flag present but empty, or `@-` with empty stdin | Rejected client-side before any request; usage suggestion names both key sources |
| Missing flag | `rotate <name>` with no `--api-key` | Validation failure naming the flag; no request sent |
| Oversize result | New key pushes the stored object past the 4 KiB limit | `UZ-VAULT-002`; nothing written; CLI surfaces the typed code |
| Insufficient role | Caller below workspace operator | `403` from the existing workspace guard, unchanged by this workstream |
| Rotation races a delete | Delete commits between the rotation's read and its write | The lock serializes them: delete first → rotation finds no row and returns `404`, writing nothing and never resurrecting it; rotation first → the delete blocks, then proceeds normally |
| Concurrent rotations | Two rotations of one secret overlap | The lock serializes them; each caller's `200` is true at the moment it commits, and the later rotation's key is the stored one. No name is ever released |
| Lock wait under load | Many rotations of one secret queue on the row lock | Requests block on the row rather than interleaving; the existing statement timeout bounds the wait, and rotation touches exactly one row so the contention is per-secret, not global |
| Coverage regression | A later diff drops CLI coverage below the floor | The CI job exits non-zero and names the floor and the actual value |

## Invariants

1. A rotation never releases the secret name, and never recreates one that was released — enforced by the row lock held across load and write, and by Dimension 2.1 driving a real concurrent delete rather than asserting the property in prose.
2. Rotation only ever replaces an existing `api_key` — enforced by the §2 guard, which runs inside the lock and returns the typed error before the write; a runtime check whose decision cannot go stale, not a review convention.
3. The key never reaches a log line, an analytics event, or stdout — enforced by the existing secure-memory handling on the server, by the CLI's argument redaction (`cli/test/argv-redact.unit.test.ts`), and by Dimension 1.3.
4. Sibling fields survive a rotation — enforced by Dimension 2.2 asserting each preserved field after the call.
5. CLI coverage cannot fall below the declared floor — enforced by the CI job exiting non-zero, which is what §4 exists to make true.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet.secret.rotate` command span | ops | The rotate command runs | command name, outcome, duration | no key bytes, no secret values; argument redaction covers `--api-key` | `test_secret_rotate_output_modes_omit_key` |
| server `rotated` / `rotate_failed` log | ops | PATCH succeeds or fails | secret name, workspace, error code | never the key or the stored object | `test_rotate_refuses_object_without_api_key` |

No product analytics event is added and no funnel changes: rotation is an operator action on an existing resource, and the CLI's telemetry span comes from the existing `wrapEFn` wrapper rather than new instrumentation. The new refusal must appear as a distinct error code in the existing error-code signal rather than folding into `UZ-VAULT-003`.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_secret_rotate_sends_single_patch` | `rotate acme --api-key k2` against the stub issues exactly one PATCH to the item route with body `{api_key:"k2"}`; no GET precedes it |
| 1.2 | unit | `test_secret_rotate_key_sources_and_validation` | `--api-key=@-` with piped text uses it; empty stdin, empty flag, and absent flag each fail with a suggestion and zero requests sent |
| 1.3 | unit | `test_secret_rotate_output_modes_omit_key` | JSON mode emits status and name only; neither renderer's output contains the key substring |
| 2.1 | integration | `test_rotate_serializes_against_delete` | Two connections: a rotation paused after its read while a delete commits → the rotation returns `404` and `SELECT` finds no row. Asserted against the database, not by inspecting the code path |
| 2.2 | integration | `test_rotate_refuses_object_without_api_key` | Stored `{"api_token":"old"}` + PATCH `{api_key:"new"}` → `409` `UZ-VAULT-006`; re-read is byte-identical and has no `api_key` |
| 2.3 | integration | `test_rotate_preserves_sibling_fields` | Stored `{provider,model,base_url,api_key}` → `200`; re-read shows the new key and all three siblings unchanged |
| 2.4 | unit | `test_secret_rotate_renders_unrotatable_refusal` | A `UZ-VAULT-006` server error renders with a suggestion naming the alternative and exits non-zero |
| 2.1 | integration | `test_concurrent_rotations_do_not_lose_a_write` | Two overlapping rotations of one secret both return `200`; the stored key equals the one that committed second, and exactly one row exists |
| 3.1 | unit | `test_secrets_page_matches_endpoint` | The page's PATCH body field equals the endpoint's required field; zero occurrences of the "no rotate command" claim |
| 3.2 | unit | `test_error_reference_regenerated` | The reference contains `UZ-VAULT-006` and matches generator output byte-for-byte |
| 4.1 | integration | `test_coverage_gate_fails_below_floor` | The enforcing script exits non-zero on a below-floor run and its output names both floor and actual |
| 4.2 | unit | `test_cli_coverage_floor_met` | The enforcing script exits 0 against the branch |
| e2e | e2e | `test_secret_rotate_live_lane` | Against a live daemon: create, rotate, confirm the name never 404s during the rotation, delete |
| regression | integration | `test_secret_create_still_claims_free_name` | `create` on a taken name still reports a skip and exits 0; the `UZ-VAULT-005` behaviour M143_002 shipped is unchanged |
| idempotency | integration | `test_repeat_rotation_is_stable` | Rotating twice to the same value succeeds twice and leaves one stored object with that key |

The live lane (`cli/test/acceptance/secret-vault.spec.ts`) self-skips without `AGENTSFLEET_ACCEPTANCE_TARGET`. It was edited but never executed in M143_002, so this workstream runs it against a live daemon for real and records the outcome in Discovery; a skipped run is not evidence.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The client rotates in one call (§1) | `cd cli && bun test test/fleet-secret-rotate.unit.test.ts test/secrets.integration.test.ts` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Rotation is locked, and an unrotatable secret is refused rather than corrupted (§2) | `zig build test --summary all 2>&1 \| grep -cE "rotate_(serializes_against_delete\|refuses_object_without_api_key)"` | 2 | P0 | |
| R4 | The secrets page no longer documents a body the endpoint rejects (§3) | `grep -c "with the new \`data\`" ~/Projects/docs/fleets/secrets.mdx` | 0 | P0 | |
| R5 | The coverage floor is enforced and met (§4) | `cd cli && node ./scripts/enforce-coverage.mjs` | exit 0, output contains `enforce-coverage: PASS` | P0 | |
| R6 | CI runs the enforcing script, not the printing one | `grep -A2 "name: Test agentsfleet" .github/workflows/test.yml` | names the enforcing script | P0 | |
| R7 | The live rotation lane executed against a daemon | `cd cli && AGENTSFLEET_ACCEPTANCE_TARGET=<url> bun run test:acceptance:live:run` | exit 0, rotation spec not skipped | P1 | |
| S1 | Unit lanes pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. If §4 resolves a coverage gap by deleting an unreachable arm (RULE TVR), the removed symbol gets a grep row here and a Discovery note before the deletion is committed.

## Out of Scope

- **Whole-object rotation (`secret rotate --data`).** Replacing the entire stored object would change a published endpoint's required body and would drop the sibling-field guarantee the browser's custom-endpoint dialog depends on. §2's refusal means no one is silently corrupted while this waits. A follow-up workstream can add it as a second, additive body form once there is a real user asking for it.
- **`GET /v1/models?provider=`, published and uncalled** — the standing finding from M143_002 Discovery. Same position `q` was in, and cheap, but unrelated to the vault surface.
- **`gen_error_codes.zig`'s hardcoded `verified:` date**, which needs a manual bump on each regeneration. §3 regenerates the page and will hit it; fixing the generator is a separate workstream.
- **The `dispatch/write_sql.md` versus `SCHEMA_CONVENTIONS.md` disagreement on pre-`2.0.0` `ALTER TABLE`** — a dotfiles fix, and this workstream touches no schema.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a key leaks; the operator runs one command; the fleets that require that secret never notice, because the name was claimed the whole time.
2. **Preserved user behaviour** — `create` still claims a free name and reports a taken one as a skip with exit 0; `show`, `list`, and `delete` are untouched; the browser's rotate dialog keeps working through the same endpoint and keeps its sibling-field guarantee.
3. **Optimal-way check** — the direct shape is exactly this: the endpoint, the role guard, and the browser caller already exist, so the client gap is one command. Authoring found the endpoint itself is not yet correct under concurrency, so §2 is the difference between shipping a command and shipping a command that tells the truth. The remaining gap from the unconstrained optimum is that a custom-shaped secret still cannot be rotated in place; §2 makes that an honest error instead of a silent one, and Out of Scope names the follow-up.
4. **Rebuild-vs-iterate** — iterate. Nothing about the vault surface wants rebuilding; the routes were already correctly separated, and M143_002 fixed the storage layer that conflated them.
5. **What we build** — one CLI command, one server guard with its error code, two corrected published pages, and the CI wiring that makes the existing coverage script able to fail a build.
6. **What we do NOT build** — whole-object rotation, a rotate confirmation prompt (rotation is not destructive; the old value is meant to stop working), rotation history or audit surfacing, and any scheduled or automatic rotation.
7. **Fit with existing features** — compounds with the credential firewall and the model registry, which both read secrets by name; it must not destabilize the model-entry reference protocol, which is why rotation writes in place and never deletes.
8. **Surface order** — CLI-only. The browser shipped rotation first; this closes the gap in the other direction, which is the repository's CLI-first default reasserting itself.
9. **Dashboard restraint** — no UI change. Rotation history would be a control with no evidence behind it until the server records rotations, which it does not.
10. **Confused-user next step** — the `UZ-VAULT-006` refusal names the alternative in its message, and `agentsfleet secret rotate --help` names both key sources. Neither path ends in "file a ticket".

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections split by surface — client, server correctness, published pages, and the CI wiring — because each is independently verifiable and only the second carries design risk. The coverage work rides along rather than becoming its own workstream: it is the gate that will grade this workstream's own CLI tests, and it is red on `main` today, so a separate spec would mean this PR's coverage claims rest on a floor nobody enforces.
- **Alternatives considered:** (a) ship the command alone and accept both the silent-corruption path and the unlocked write, rejected because the corrupting shape is the one our own documented example uses and because a scriptable client turns a two-browser-tab race into a two-cron-job race; (b) generalize PATCH to full-object replacement, rejected as a breaking change to a published endpoint and a loss of the sibling-field guarantee the browser depends on; (c) fix the docs only and defer the command, rejected because the operator still has no safe move; (d) move every vault write behind one store-level transaction helper, rejected as the larger refactor — see the verdict.
- **Patch-vs-refactor verdict:** this is a **patch that also finishes an unfinished fix**. The quality-ceiling question — would a larger refactor be more concurrent, faster, or more testable — has one honest yes and several noes. The yes is concurrency: rotation is a read-modify-write with no lock, so §2 adopts the row-lock protocol the delete path already uses. That is reuse of an existing module, not a rebuild. The noes: performance is dominated by envelope decrypt/encrypt, which no restructuring removes; the lock is per-secret, so it serializes only writers to the same credential and adds no global contention; the user-facing path is already one round-trip; and testability improves through the lock making the property assertable against a real database rather than through any new abstraction. A store-level transaction helper wrapping all five vault write paths is the tempting bigger move, and it is rejected for now — the other four paths are single-statement writes that are already atomic, so the helper would be scaffolding for one real caller (RULE HLP). If a second multi-statement vault write appears, that helper becomes correct and should be its own workstream.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

- **Authoring verification (Jul 29, 2026)** — every load-bearing claim above was read from source on `main` at `701a7052a`, not from the handoff: `innerRotateSecret`'s `RotateBody` requires `api_key` (`secrets.zig:240`); the insert-not-replace behaviour is `parsed.value.object.put` at `secrets.zig:323`; the published page's wrong `data` body is `fleets/secrets.mdx:72`; the docs' own create example stores `{"api_token":...}`, which is precisely the shape §2 refuses; the CLI has no rotate binding (`handlers-bind-fleet.ts`); and the coverage floor was reproduced red by running `cli/scripts/enforce-coverage.mjs` — `actual function=99.97% line=99.74%` against a 100% floor.
- **Authoring finding, folded into §2 — the rotate route is an unlocked read-modify-write.** Raised by Indy's quality-ceiling question ("would a large refactor be better optimized, concurrent, performant… easily testable?") and confirmed from source, not inferred: `innerRotateSecret` acquires a pooled connection and calls `rotateSecretKeyOnConn`, which runs `vault.loadJson` and then `vault.storeJsonPlaintext` with no `BEGIN` and no row lock (`secrets.zig:302-330`); the write resolves to `INSERT_SECRET`, an `ON CONFLICT … DO UPDATE` upsert (`secrets/sql.zig:39`). So a rotation that reads before a concurrent delete commits will re-INSERT the row it read — resurrecting a deleted credential with a new identifier that never passed the model-entry reference check that `state/secret_reference_txn.zig` exists to enforce. Two concurrent rotations lose one write while both answer `200`.

  The defect is on `main` today and the browser can already reach it; the CLI does not introduce it. It is folded in rather than filed because a scriptable rotate command is what makes it likely — a provisioning script racing a teardown script, instead of two browser tabs — and because §2's shape guard would otherwise be a check-then-write inside the same unlocked window, narrowing the race rather than closing it. The fix reuses step 1 of the existing lock protocol; rotation changes no reference set, so it takes that prefix only and cannot deadlock against a full participant.

- **Correction to the inherited finding** — M143_002 recorded four red coverage files. Reading `coverage/lcov.info` directly gives six: `cli.ts` (2 lines), `commands/api_key.ts` (27), `commands/connector.ts` (10), `commands/fleet_install.ts` (3), `commands/fleet_schedule.ts` (16), and one uncovered function in `commands/login-helpers.ts`. The same spec's Amendment A14 also states `cli/` is enforced by `bunfig.toml` and `enforce-coverage.mjs`; the script is real but nothing invokes it, which is what §4 fixes.
