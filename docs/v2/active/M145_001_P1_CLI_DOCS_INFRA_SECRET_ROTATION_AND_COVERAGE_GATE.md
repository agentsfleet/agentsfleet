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
**Status:** IN_PROGRESS
**Priority:** P1 — `0.24.0` shipped an operator-facing hole: no client path replaces a secret value without a window where the name is absent.
**Categories:** CLI, DOCS, INFRA
**Batch:** B1 — standalone; no other workstream touches the vault surface.
**Branch:** feat/m145-secret-rotate
**Test Baseline:** unit=3223 integration=455
**Depends on:** none — `PATCH /v1/workspaces/{workspace_id}/secrets/{secret_name}` shipped in `0.24.0`.
**Provenance:** agent-generated (pre-spec, `docs/v2/done/M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md` Discovery A15 and the standing coverage finding)
**Canonical architecture:** `docs/architecture/web_app.md` (secret surfaces) and `docs/AUTH.md` (workspace operator role on every vault route)

---

## Overview

**Goal (testable):** `agentsfleet secret rotate <name> --api-key <key>` replaces a stored key in one call, and the name stays claimed for the whole call so a fleet requiring it keeps running.
**Problem:** An operator who must replace a leaked or expiring key has no safe client move. `create` refuses a name the workspace already holds (`UZ-VAULT-005`), and `delete` then `create` leaves the name absent between two calls — every fleet that requires it fails in the gap. The endpoint that does this correctly has been published since `0.24.0` and only the browser calls it.
**Solution summary:** Add a `secret rotate` command that mirrors the published PATCH route and the browser's `rotateSecret` exactly, so the client gains rotation with no server change at all. Document the route's real body field, which the published page currently gets wrong, and document plainly that rotation replaces `api_key` and only `api_key`. Separately, wire the Command-Line Interface (CLI) coverage floor into Continuous Integration (CI), where it has never run, and clear the gaps it finds.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(cli): rotate a workspace secret without releasing its name
- **Intent (one sentence):** An operator replaces a credential in place instead of deleting and recreating it.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `ui/packages/app/lib/api/secrets.ts` — `rotateSecret` is the shipped caller; the CLI mirrors its body and its preservation guarantee rather than inventing a second shape.
2. `cli/src/commands/fleet_secret.ts` and `cli/src/commands/fleet_secret_body.ts` — the command style to match, and the stdin sentinel (`@-`) whose stated rationale is keeping secrets out of shell history.
3. `src/agentsfleetd/http/handlers/fleets/secrets.zig` — `innerRotateSecret` is the behaviour being wrapped. Read-only for this workstream; no Zig changes.
4. `docs/v2/done/M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md` Discovery A15 — why `--force` was retired and why creation claims a free name; this workstream completes that decision.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/commands/fleet_secret.ts` | EDIT | Add the rotate effect beside create/show/list/delete. |
| `cli/src/commands/fleet_secret_rotate.ts` | CREATE | Key resolution (literal and stdin) split out so `fleet_secret.ts` stays under the length cap, mirroring the existing `fleet_secret_body.ts` split. |
| `cli/src/program/cli-tree-fleet.ts` | EDIT | Register `secret rotate <name>` with its flag. |
| `cli/src/program/handlers-bind-fleet.ts` | EDIT | Bind the handler into the `secret` group. |
| `cli/test/fleet-secret-rotate.unit.test.ts` | CREATE | Command-level behaviour and flag validation. |
| `cli/test/secrets.integration.test.ts`; `cli/test/acceptance/secret-vault.spec.ts` | EDIT | In-process rotation against the stub, and the live lane. |
| `.github/workflows/test.yml`; `make/test-unit.mk` | EDIT | Run the enforcing coverage script instead of the non-enforcing one. |
| `cli/test/api-key-linecov.unit.test.ts`; `cli/test/cli-linecov.unit.test.ts`; `cli/test/connectors.service.unit.test.ts`; `cli/test/coverage-fill.unit.test.ts` | EDIT | Close the floor gaps §3 turns red-blocking. |
| `~/Projects/docs/fleets/secrets.mdx`; `~/Projects/docs/changelog.mdx` | EDIT | Document the command, correct the wrong PATCH body, and announce. |

**Scope grading.** Rubric R2 compares `git diff --name-only origin/main` against this table. A path that proves genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition. The `cli/test/*linecov*` row names the likely homes for the coverage closures; the gaps are enumerated in §3 and the agent may place a test in the file that already owns its subject instead.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (flag strings and field names as named constants), JCL (the rotate command's JSON-mode output shape), TNM and TST-NAM (test naming), NDC and ORP (no helper without a caller; sweep on any rename), TFX (tests reuse production constants), TVR (do not test values that cannot occur).
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** — the CLI diff is the whole of this workstream's code: file-shape decision at PLAN, `const` and import discipline, Bun primitives.
- **`~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md`** and **`~/Projects/dotfiles/docs/CHANGELOG_VOICE.md`** — the published page and the changelog entry.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / LIFECYCLE | no | No `*.zig` file changes; the server is consumed as published. |
| File & Function Length | yes | `fleet_secret.ts` sits near the cap already, which is why key resolution lands in its own file. |
| UFS | yes | Flag literal, the `api_key` field name, and the stdin sentinel become named constants, shared verbatim with the existing custom-endpoint constants. |
| UI Substitution / DESIGN TOKEN | no | No User Interface (UI) files change — the browser already rotates. |
| ERROR REGISTRY | no | No new code; the command surfaces the codes the route already returns. |
| SCHEMA / LOGGING | no | No schema change and no new logging surface. |

## Prior-Art / Reference Implementations

- **Reference (CLI):** `cli/src/commands/fleet_secret.ts` — `secret rotate` is a sibling of `create`, not a new pattern. Aligns with the "7 Pillars" of CLI developer experience on command → handler → errors split, handler purity, output as a service (human and JSON renderers chosen by the renderer, never `console.log` in the handler), structured errors carrying a `suggestion`, and the three-tier test pyramid. Divergence: none intended.
- **Reference (client call):** `ui/packages/app/lib/api/secrets.ts` `rotateSecret` — the body shape and the preservation guarantee are copied, not re-derived.

## Sections (implementation slices)

### §1 — The client rotates a secret in one call

An operator replaces a stored key with `agentsfleet secret rotate <name> --api-key <key>`. The command sends one PATCH to the published route and reports the outcome; the name is never released, so a fleet requiring it keeps running across the rotation. The key may be supplied on stdin, because a credential passed as an argument lands in shell history — the same reason `create` already offers `--data=@-`.

**Implementation default:** the flag is `--api-key`, matching the endpoint field, the browser caller, and `create`'s existing typed flag.

- **Dimension 1.1** — a rotate call issues exactly one PATCH to the secret's item route with the key as `api_key`, and reports success naming the secret → Test `test_secret_rotate_sends_single_patch`
- **Dimension 1.2** — `--api-key=@-` reads the key from stdin, rejects empty stdin with a usage suggestion, and a missing or empty flag fails before any request is sent → Test `test_secret_rotate_key_sources_and_validation`
- **Dimension 1.3** — JSON mode emits the rotate outcome keyed on the secret name and nothing resembling the key; human mode prints no key bytes → Test `test_secret_rotate_output_modes_omit_key`
- **Dimension 1.4** — a `404` from the route renders as "secret not found" with a `secret list` suggestion and a non-zero exit → Test `test_secret_rotate_renders_missing_secret`

### §2 — The published page describes what the route actually does

`fleets/secrets.mdx` instructs the reader to send PATCH "with the new `data`". The route requires `api_key` and rejects a `data` body, so the documented call fails for anyone who follows it. The same page states the client has no rotate command, which §1 makes false.

The page must also say plainly what rotation does **not** cover: it replaces the `api_key` field and only that field. A secret storing another field name — the page's own `github` example stores `api_token`, and fleets read `${secrets.github.token}` — is not rotated by this call. Stating that is cheaper than a reader discovering it.

- **Dimension 2.1** — the secrets page documents `secret rotate`, states the correct PATCH body field, and retains no claim that the client cannot rotate → Test `test_secrets_page_matches_route`
- **Dimension 2.2** — the page states the `api_key`-only scope and points a non-`api_key` secret at delete-then-create → Test `test_secrets_page_states_rotation_scope`

### §3 — The CLI coverage floor runs where it can fail the build

`cli/bunfig.toml` declares a 100% line and function floor, and `scripts/enforce-coverage.mjs` exists to enforce it because Bun parses the threshold without acting on it. Nothing calls the script: CI's `test-unit-cli` and `make test-coverage-all` both run `test:coverage`, which only prints a table. The floor has been decorative, and `main` is below it.

**Implementation default:** point the existing CI job and the make recipe at the enforcing script rather than adding a new job or a new make target.

- **Dimension 3.1** — the CI job and the make recipe fail when coverage is below the declared floor, proven by the gaps on `main` being red before they are closed → Test `test_coverage_gate_fails_below_floor`
- **Dimension 3.2** — the floor is met: the uncovered lines in `cli.ts`, `commands/api_key.ts`, `commands/connector.ts`, `commands/fleet_install.ts`, `commands/fleet_schedule.ts` and the uncovered function in `commands/login-helpers.ts` are covered, or deleted if unreachable → Test `test_cli_coverage_floor_met`

Gaps are read from `cli/coverage/lcov.info` (`DA:` for lines, `FNDA:` for functions). The text reporter's "Uncovered Line #s" column also lists lines carrying uncovered *branches*, and reading it as a line list cost a prior session a full cycle. Where a gap proves genuinely unreachable, RULE TVR applies: delete the dead arm rather than manufacture a test for a value that cannot occur, and record it in Discovery.

## Interfaces

```
PATCH /v1/workspaces/{workspace_id}/secrets/{secret_name}     (published, consumed unchanged)
  request   { "api_key": "<non-empty string>" }
  200       { "name": "<secret_name>" }
  400       UZ-* invalid request — absent/malformed body, empty api_key, bad workspace id or name
  404       UZ-VAULT-003 secret not found in this workspace
  413-class UZ-VAULT-002 resulting object exceeds the 4 KiB stored limit

agentsfleet secret rotate <name> --api-key <key|@->
  exit 0    human: one line naming the secret, no key bytes
            json: { "status": "rotated", "name": "<name>" }
  exit != 0 typed CliError carrying code + suggestion; validation failures send no request
```

The route replaces `api_key` and preserves every other stored field. This workstream consumes that behaviour and does not change it; §2 documents it.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown secret | Name absent from this workspace | `404` `UZ-VAULT-003`; CLI suggests `secret list` and exits non-zero |
| Empty key | Flag present but empty, or `@-` with empty stdin | Rejected client-side before any request; usage suggestion names both key sources |
| Missing flag | `rotate <name>` with no `--api-key` | Validation failure naming the flag; no request sent |
| Missing name | `rotate` with no positional | Validation failure naming the usage line; no request sent |
| Oversize result | New key pushes the stored object past the 4 KiB limit | `UZ-VAULT-002`; nothing written; CLI surfaces the typed code |
| Insufficient role | Caller below workspace operator | `403` from the existing workspace guard; CLI renders it unchanged |
| Coverage regression | A later diff drops CLI coverage below the floor | The CI job exits non-zero and names the floor and the actual value |

## Invariants

1. A rotation never releases the secret name — the command issues PATCH only, and never a DELETE; enforced by Dimension 1.1 asserting the exact request set.
2. The key never reaches a log line, an analytics event, or stdout — enforced by the CLI's argument redaction (`cli/test/argv-redact.unit.test.ts`) and by Dimension 1.3.
3. A validation failure sends no request — enforced by Dimension 1.2 asserting zero requests on each rejected input, so a malformed invocation cannot reach the vault.
4. CLI coverage cannot fall below the declared floor — enforced by the CI job exiting non-zero, which is what §3 exists to make true.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet.secret.rotate` command span | ops | The rotate command runs | command name, outcome, duration | no key bytes, no secret values; argument redaction covers `--api-key` | `test_secret_rotate_output_modes_omit_key` |

No product analytics event is added and no funnel changes: rotation is an operator action on an existing resource, and the command's telemetry span comes from the existing `wrapEFn` wrapper rather than new instrumentation.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_secret_rotate_sends_single_patch` | `rotate acme --api-key k2` against the stub issues exactly one PATCH to the item route with body `{api_key:"k2"}`; no GET precedes it and no DELETE follows |
| 1.2 | unit | `test_secret_rotate_key_sources_and_validation` | `--api-key=@-` with piped text uses it; empty stdin, empty flag, absent flag, and absent name each fail with a suggestion and zero requests sent |
| 1.3 | unit | `test_secret_rotate_output_modes_omit_key` | JSON mode emits status and name only; neither renderer's output contains the key substring |
| 1.4 | unit | `test_secret_rotate_renders_missing_secret` | A `UZ-VAULT-003` server error renders with a `secret list` suggestion and exits non-zero |
| 2.1 | unit | `test_secrets_page_matches_route` | The page's PATCH body field equals the route's required field; zero occurrences of the "no rotate command" claim |
| 2.2 | unit | `test_secrets_page_states_rotation_scope` | The page states rotation replaces `api_key` only and names delete-then-create for other shapes |
| 3.1 | integration | `test_coverage_gate_fails_below_floor` | The enforcing script exits non-zero on a below-floor run and its output names both floor and actual |
| 3.2 | unit | `test_cli_coverage_floor_met` | The enforcing script exits 0 against the branch |
| e2e | e2e | `test_secret_rotate_live_lane` | Against a live daemon: create, rotate, confirm the name resolves throughout, delete |
| regression | integration | `test_secret_create_still_claims_free_name` | `create` on a taken name still reports a skip and exits 0; the `UZ-VAULT-005` behaviour M143_002 shipped is unchanged |
| idempotency | integration | `test_repeat_rotation_is_stable` | Rotating twice to the same value succeeds twice and leaves one stored secret under that name |

The live lane (`cli/test/acceptance/secret-vault.spec.ts`) self-skips without `AGENTSFLEET_ACCEPTANCE_TARGET`. It was edited but never executed in M143_002, so this workstream runs it against a live daemon for real and records the outcome in Discovery; a skipped run is not evidence.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The client rotates in one call (§1) | `cd cli && bun test test/fleet-secret-rotate.unit.test.ts test/secrets.integration.test.ts` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R3 | No Zig changed — the server is consumed as published | `git diff --name-only origin/main...HEAD \| grep -c '\.zig$'` | 0 | P0 | |
| R4 | The page no longer documents a body the route rejects (§2) | `grep -c 'with the new `data`' ~/Projects/docs/fleets/secrets.mdx` | 0 | P0 | |
| R5 | The coverage floor is enforced and met (§3) | `cd cli && node ./scripts/enforce-coverage.mjs` | exit 0, output contains `enforce-coverage: PASS` | P0 | |
| R6 | CI runs the enforcing script, not the printing one | `grep -A2 'name: Test agentsfleet' .github/workflows/test.yml` | names the enforcing script | P0 | |
| R7 | The live rotation lane executed against a daemon | `cd cli && AGENTSFLEET_ACCEPTANCE_TARGET=<url> bun run test:acceptance:live:run` | exit 0, rotation spec not skipped | P1 | |
| S1 | Unit lanes pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. If §3 resolves a coverage gap by deleting an unreachable arm (RULE TVR), the removed symbol gets a grep row here and a Discovery note before the deletion is committed.

## Out of Scope

- **Any server change.** Ruled out by Indy during EXECUTE — see Discovery. The route is consumed exactly as published; rotation's `api_key`-only scope and its unlocked read-modify-write both stay as they are on `main`, and §2 documents the first of those rather than changing it.
- **Rotating a secret that stores some other field name.** The route only replaces `api_key`, so `github.token` and `stripe.api_token` are rotated by delete-then-create. §2 documents this; generalizing the route is a server change.
- **`GET /v1/models?provider=`, published and uncalled** — the standing finding from M143_002 Discovery. Same position `q` was in, and cheap, but unrelated to the vault surface.
- **`gen_error_codes.zig`'s hardcoded `verified:` date**, which needs a manual bump on each regeneration. No error code changes here, so it is not reached.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a key leaks; the operator runs one command; the fleets that require that secret never notice, because the name was claimed the whole time.
2. **Preserved user behaviour** — `create` still claims a free name and reports a taken one as a skip with exit 0; `show`, `list`, and `delete` are untouched; the browser's rotate dialog keeps working through the same route.
3. **Optimal-way check** — the direct shape is exactly this: the route, the role guard, and the browser caller already exist, so the gap is one command and the workstream carries no server risk at all. The gap from the unconstrained optimum is that a secret storing another field name still cannot be rotated in place; §2 documents that rather than hiding it.
4. **Rebuild-vs-iterate** — iterate. Nothing about the vault surface wants rebuilding for this command to exist.
5. **What we build** — one CLI command, one corrected published page, and the CI wiring that makes the existing coverage script able to fail a build.
6. **What we do NOT build** — any server change, whole-object rotation, a rotate confirmation prompt (rotation is not destructive; the old value is meant to stop working), rotation history, and scheduled or automatic rotation.
7. **Fit with existing features** — compounds with the credential firewall and the model registry, which both read secrets by name. It changes no server behaviour, so it cannot destabilize either.
8. **Surface order** — CLI-only. The browser shipped rotation first; this closes the gap in the other direction, which is the repository's CLI-first default reasserting itself.
9. **Dashboard restraint** — no UI change. Rotation history would be a control with no evidence behind it until the server records rotations, which it does not.
10. **Confused-user next step** — `agentsfleet secret rotate --help` names both key sources, the `404` render points at `secret list`, and the secrets page states the `api_key`-only scope. Neither path ends in "file a ticket".

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections split by surface — client, published page, and CI wiring. The coverage work rides along rather than becoming its own workstream: it is the gate that will grade this workstream's own CLI tests, and it is red on `main` today, so a separate spec would mean this PR's coverage claims rest on a floor nobody enforces.
- **Alternatives considered:** (a) generalize the route to patch a named field so any secret shape rotates, rejected as a server change Indy ruled out for this workstream; (b) hold the command until the route is generalized, rejected because the operator has no safe move today and the `api_key` case is the common one; (c) fix the page only and defer the command, rejected for the same reason.
- **Patch-vs-refactor verdict:** this is a **patch**. Solution-size matches problem-size: the route, the guard, and the browser caller all exist, and the diff adds one command plus test and documentation changes. The quality-ceiling question was asked and answered against the server during EXECUTE; Indy's call was that it is not an issue for this workstream, so the refactor axis is closed here rather than traded off silently.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an Indy-acked verbatim quote here.

- **Amendment A1 (EXECUTE) — the server sections are removed; this workstream ships no Zig.** The spec as authored carried a §2 that took the vault row lock across rotation's read-modify-write and refused a rotation on a secret with no `api_key` field (`UZ-VAULT-006`). Both were built, compiled, cross-compiled for both linux targets, and covered by two-connection lock tests. Indy reviewed the problem and closed it.

  > Indy (2026-07-29 07:51): "this is not an issue, lets move on" — context: the rotate route's unlocked read-modify-write and the `api_key`-only shape guard, after the failure was walked through end to end.

  The work is preserved on `stash@{0}` of this worktree rather than deleted, so reactivating it costs a `git stash pop`. Categories dropped `API` accordingly and the ZIG, PUB, LIFECYCLE and ERROR REGISTRY gate rows flipped to "no". What remains of the finding is documentation: §2 Dimension 2.2 states the `api_key`-only scope on the published page, which is the part a user can act on.

- **Authoring verification (Jul 29, 2026)** — every load-bearing claim was read from source on `main` at `701a7052a`, not from the handoff: `innerRotateSecret`'s `RotateBody` requires `api_key` (`secrets.zig:240`); the published page's wrong `data` body is `fleets/secrets.mdx:72`; the CLI has no rotate binding (`handlers-bind-fleet.ts`); the runner resolves `${secrets.<name>.<field>}` for arbitrary field names, with `${secrets.github.token}` appearing in `runner/engine/credential_request.zig` and its redaction tests; and the coverage floor was reproduced red by running `cli/scripts/enforce-coverage.mjs` — `actual function=99.97% line=99.74%` against a 100% floor.

- **Correction to the inherited finding** — M143_002 recorded four red coverage files. Reading `coverage/lcov.info` directly gives six: `cli.ts` (2 lines), `commands/api_key.ts` (27), `commands/connector.ts` (10), `commands/fleet_install.ts` (3), `commands/fleet_schedule.ts` (16), and one uncovered function in `commands/login-helpers.ts`. The same spec's Amendment A14 also states `cli/` is enforced by `bunfig.toml` and `enforce-coverage.mjs`; the script is real but nothing invokes it, which is what §3 fixes.
