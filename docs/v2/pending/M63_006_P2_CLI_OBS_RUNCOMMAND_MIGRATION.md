<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M63_006: zombiectl runCommand migration — every handler routes through the per-command boundary

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 006
**Date:** May 08, 2026
**Status:** PENDING
**Priority:** P2 — `runCommand` shipped in M63_004 but no handler imports it; the catch logic still lives inline in `cli.js`. The wrapper is dead code until commands migrate.
**Categories:** CLI, OBS
**Batch:** B1 — independent of any other in-flight workstream. No external dependencies.
**Branch:** feat/m63-006-runcommand-migration (to be created at CHORE(open)).
**Depends on:** M63_004_P1_CLI_OBS_RESILIENCE — `runCommand`, `apiRequestWithRetry`, `trackHttpRequest/Retry` already exist; this spec consumes them.

**Canonical architecture:** `zombiectl` is the customer/operator entry point. M63_004 introduced the resilience layer; M63_006 finishes the rollout so every command shares one error-handling, telemetry, and exit-code boundary.

---

## Implementing agent — read these first

1. `zombiectl/src/lib/run-command.js` — the wrapper to migrate onto. Note the signature: `runCommand({ name, handler, retry, instrument, errorMap, ctx, deps })` and the returned exit code (0/1). Every command in this spec ends up wrapping its body through this entry point.
2. `zombiectl/src/cli.js:280-318` — the inline top-level catch that distinguishes `ApiError` / `TypeError("fetch failed")` / unknown. Once every command runs through `runCommand`, this catch becomes a thin safety net (or goes away). The spec keeps the safety net; what changes is that handlers no longer rely on it for normal error formatting.
3. `zombiectl/src/program/command-registry.js` — where commands are wired into the dispatcher. The registry is the natural choke point for the migration: each registered command is reshaped to delegate through `runCommand`.
4. `zombiectl/src/commands/*.js` — 14 handler files, every one currently shaped as `export async function commandX(ctx, args, workspaces, deps)`. The migration preserves the external signature and inserts `runCommand` as the body's outermost call.
5. `zombiectl/src/program/http-client.js:7-50` — `request()` already routes through `apiRequestWithRetry`. Confirms handlers do not need code changes for retry; they only inherit `errorMap`-driven user messages.
6. `zombiectl/src/lib/analytics.js:65-90` — `trackCliEvent`, `trackHttpRequest`, `trackHttpRetry`. The `cli_command_started/finished/error` triplet currently fires from `cli.js`; once every command is migrated, it fires from inside `runCommand` instead, carrying per-command props.
7. `docs/REST_API_DESIGN_GUIDELINES.md` §error-shape — canonical UZ-* error envelope. The `errorMap` per command keys off these codes.
8. `docs/v2/done/M63_004_P1_CLI_OBS_RESILIENCE.md` §3 — the "Out of Scope" carve-out that this spec discharges. Review the boundaries M63_004 set so we don't relitigate them.

If any handler currently does its own `try { … } catch (err) { … }` to format an `ApiError`, the migration removes that catch and pushes the per-code message into the `errorMap`. Handlers retain `try` blocks ONLY when they have legitimate per-step recovery logic (e.g., poll loops with their own backoff).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal. Rules in scope:
  - **RULE NSQ** (no silent quiet) — every command emits the `cli_command_started/finished/error` triplet via `runCommand`; no handler may swallow an error without surfacing through the wrapper.
  - **RULE EMS** (exception messages stable) — `errorMap` entries become the user-facing surface for known UZ-* codes; once shipped they are stable strings.
  - **RULE TST-NAM** — no milestone IDs in test names or fixture names.
  - **RULE NDC** / **RULE NLR** — once migration completes, the inline catch in `cli.js` becomes dead code (only the safety net remains); delete the duplicated formatting; do not retain a "legacy" branch.
  - **RULE NLG** (no legacy framing pre-v2.0.0) — `cat VERSION` is < 2.0.0; the spec must not introduce `legacy_*` names, `V2` twins, or compat shims for the unmigrated handlers. The single PR migrates them all.
- **`docs/BUN_RULES.md`** — TS/JS file shape, const/import discipline, file-as-module pattern. Every modified handler stays under the 350-line file gate.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** §error-shape — only consulted to enumerate the UZ-* codes that the `errorMap` audit knows about. This spec does not add or modify any HTTP route.

Standard set is the floor. No Zig, schema, or auth-flow concerns.

---

## Anti-Patterns to Avoid (read this BEFORE drafting the spec)

(Template guidance — already absorbed during drafting.)

---

## Overview

**Goal (testable):** Every command registered in `zombiectl/src/program/command-registry.js` dispatches through `runCommand({ name, errorMap, handler })`; the inline catch in `cli.js` collapses to a thin safety net (or is removed entirely); a new audit script `scripts/audit-cli-runcommand.sh` fails CI when any registered command bypasses the wrapper or omits an `errorMap` entry for the UZ-* codes its endpoints can return.

**Problem (operator-visible):** Today, error messages drift across commands — `commandSteer` formats an `ApiError` one way, `commandBillingShow` another, and a third command silently exits with the bare server message because no one wrote a per-code translation. Per-command analytics props (`exit_code`, `error_code`, command-specific `target` IDs) are missing because the catch lives in `cli.js` where the command name is the only handle. New commands added since M63_004 reproduce the old shape because there is no compiler/audit signal that `runCommand` exists.

**Solution summary:** Migrate every file in `zombiectl/src/commands/*.js` (14 files) so each registered handler is exposed as `{ name, errorMap, handler }`; the registry wraps it with `runCommand` at dispatch. The inline `cli.js` catch is reduced to a last-resort `process.on("uncaughtException")` net. A new `scripts/audit-cli-runcommand.sh` enumerates registered commands, asserts each one declares an `errorMap`, and asserts the map covers every UZ-* code the command's reachable endpoints can return (cross-checked against `zombiectl/src/lib/api-paths.js` and the OpenAPI document). The audit runs in `make lint`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/program/command-registry.js` | EDIT | Dispatch every registered command through `runCommand`. Inject `errorMap` and `name` per registration. Single choke point for the migration. |
| `zombiectl/src/commands/agent.js` | EDIT | Convert exported `commandAgent` to `{ name, errorMap, handler }` shape; remove inline `try/catch` for `ApiError` formatting. |
| `zombiectl/src/commands/agent_external.js` | EDIT | Same pattern; three sub-commands (add/list/delete) each get an `errorMap`. |
| `zombiectl/src/commands/billing.js` | EDIT | Same pattern; `commandBilling` + `commandBillingShow`. |
| `zombiectl/src/commands/core-ops.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/core.js` | EDIT | Same pattern; auth-critical (login/logout/hydration) — keep current message strings verbatim in the `errorMap` so user-visible behavior is byte-identical. |
| `zombiectl/src/commands/grant.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/tenant_provider.js` | EDIT | Same pattern; show/add/delete. |
| `zombiectl/src/commands/tenant.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/workspace.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/zombie_credential.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/zombie_events.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/zombie_list.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/zombie_steer.js` | EDIT | Same pattern. |
| `zombiectl/src/commands/zombie.js` | EDIT | Same pattern. |
| `zombiectl/src/cli.js` | EDIT | Remove the duplicated `ApiError`/fetch-failed/unknown formatting from the top-level catch. Keep an `uncaughtException` safety net only. |
| `zombiectl/src/lib/run-command.js` | EDIT (light) | If the audit reveals shape mismatches (e.g., handlers needing `args` plus `ctx`), tighten the wrapper signature and update the docstring. No new public surface. |
| `scripts/audit-cli-runcommand.sh` | CREATE | New audit. Enumerates registered commands; fails if any bypasses `runCommand` or omits an `errorMap` entry for reachable UZ-* codes. Wired into `make lint`. |
| `Makefile` | EDIT | Wire `audit-cli-runcommand` into the `lint` target alongside the existing CLI audits. |
| `zombiectl/test/run-command.unit.test.js` | EDIT | Extend with cases asserting (a) the wrapper invokes the handler with the existing `(ctx, args, workspaces, deps)` shape preserved, (b) `errorMap` rewrites `ApiError.code → user-facing code+message`, (c) `cli_command_started/finished/error` events carry the per-command name verbatim. |
| `zombiectl/test/commands/registry.unit.test.js` | CREATE | New unit suite asserting every registered command exports a `name` + `errorMap` + `handler` triple and the dispatch path returns the wrapper's exit code. |
| `zombiectl/test/commands/error-map.fixture.js` | CREATE | Shared fixture: list of `{ command, expectedCodes }` derived from `api-paths.js` + OpenAPI. The audit script and the unit suite both read it. |
| `~/Projects/docs/changelog.mdx` | EDIT (at CHORE(close)) | New `<Update>` describing the migration in user-visible terms — "every CLI command now reports a stable error code and surfaces server messages consistently". |

---

## Sections (implementation slices)

### §1 — Registry-driven dispatch

The migration's single load-bearing change. `zombiectl/src/program/command-registry.js` is the only file that calls handlers; once it wraps every dispatch with `runCommand`, every command is migrated regardless of its file.

What §1 delivers: the registry imports `runCommand`, exposes a single `dispatch(name, ctx, args, …)` entry point, and every entry in the registry's table carries `{ name, errorMap, handler }`. Handlers themselves don't change shape yet.

**Implementation default:** the dispatcher reads `errorMap` from the registry table — handlers do NOT receive it. This keeps handlers unaware of the wrapper and lets the audit script enforce one canonical declaration site.

### §2 — Per-command `errorMap` declarations

Each command declares the UZ-* codes its reachable endpoints can return and the user-facing message+code for each. The map lives next to the handler in the same file.

What §2 delivers: every file under `zombiectl/src/commands/` exports an `errorMap` object alongside the handler. Codes the command does not call are absent (zero-cost). Codes the command MUST handle (per the audit, derived from `api-paths.js` + OpenAPI) are present.

**Implementation default:** when a code maps to the same human message in multiple commands, lift it into a shared `zombiectl/src/lib/error-map-presets.js` and re-export. The audit treats presets identically to inline maps.

### §3 — Inline-catch teardown in `cli.js`

What §3 delivers: `cli.js`'s top-level catch is reduced to a `process.on("uncaughtException")`-style safety net that emits `cli_error{ code: "UNEXPECTED" }` and exits 1. The `ApiError` and `fetch failed` branches are deleted because every code path that produced them now flows through `runCommand`.

**Implementation default:** keep the safety net even though §1+§2 should cover every dispatch path — defense in depth for any code that runs before the registry is constructed (e.g., flag parser fatal). The safety net does NOT format `ApiError` (impossible to reach there); it only handles raw thrown errors.

### §4 — `audit-cli-runcommand.sh` + Makefile wiring

What §4 delivers: a shell script that

- parses `command-registry.js` to enumerate registered names,
- asserts every entry uses `runCommand` (no direct handler call),
- asserts every entry has an `errorMap`,
- cross-references each command against the endpoint paths it touches (via `api-paths.js`) and the OpenAPI document, and asserts the `errorMap` covers every UZ-* code those endpoints can return.

The script fails fast with a non-zero exit and a per-violation report. It is added to `make lint`.

**Implementation default:** the cross-reference is built from a static map, not by tracing imports — handlers may construct paths dynamically. The static map lives in `error-map.fixture.js` and is generated from `api-paths.js`. Manual additions are allowed (e.g., a command that hits more than one endpoint) but every entry must list its endpoints explicitly.

### §5 — Test coverage and changelog

What §5 delivers: extended `run-command.unit.test.js`, a new `registry.unit.test.js`, and a `<Update>` block in `~/Projects/docs/changelog.mdx` describing the user-visible behavior (consistent error codes across every CLI command, stable server-message surfacing). Tests live next to the migration so they ship in the same commit.

---

## Interfaces

The registry table entries become:

```
{
  name: "zombie steer",
  errorMap: {
    "UZ-WS-404":      { code: "WORKSPACE_NOT_FOUND", message: "workspace not found — check `zombie workspace list`" },
    "UZ-ZB-404":      { code: "ZOMBIE_NOT_FOUND",     message: "zombie not found in this workspace" },
    "UZ-AUTH-401":    { code: "EXPIRED_SESSION",       message: "session expired — run `zombie login`" }
  },
  endpoints: ["POST /v2/workspaces/:wsid/zombies/:zid/steer"],
  handler: commandSteer
}
```

`runCommand` receives `{ name, errorMap, handler, ctx, deps, retry, instrument }` (already implemented). Handler signature is unchanged (`(ctx, args, workspaces, deps)`); the registry adapts the call.

`scripts/audit-cli-runcommand.sh` exit codes:

- `0` — every registered command uses `runCommand` and declares `errorMap` covering reachable UZ-* codes.
- `1` — at least one command bypasses `runCommand`, omits `errorMap`, or has gaps relative to its declared endpoints. Output lists the offending command + missing codes.

No HTTP routes, schema, or auth flow changes.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Handler throws `ApiError` for an unmapped UZ-* code | Endpoint added a new code; `errorMap` not updated | `runCommand` falls through to the raw `code`/`message` from the server (today's behavior). Audit flags it on next `make lint`. |
| Audit false positive: command's reachable codes change without `api-paths.js` update | Static map drift | Audit fails fast; PR is blocked until either `errorMap` or the `endpoints` declaration is corrected. |
| Handler still does its own `try/catch` for legitimate per-step recovery | Long poll loops, retry-with-jitter inside login | Allowed. Audit detects only top-level direct dispatch outside `runCommand`. Per-step `try/catch` inside the handler body is fine. |
| Two commands map the same UZ-* code to different user messages | Inconsistent operator surface | Allowed at the JS level, surfaced by a `make lint` warning (not error). Encourages migration into `error-map-presets.js`. |
| `runCommand` swallows an error class today's `cli.js` catch surfaced | Migration regression | Tests in `run-command.unit.test.js` cover every error class; `make test` blocks the migration. |
| New command file added without registry entry | Dead code | Registry is the source of truth; dead command files are caught by RULE ORP at next CHORE(close). |

---

## Invariants

1. Every entry in `command-registry.js` exports `{ name, errorMap, handler }`. — Enforced by `scripts/audit-cli-runcommand.sh` in `make lint`.
2. Every reachable UZ-* code (per the `endpoints` declaration) is keyed in the corresponding `errorMap`. — Enforced by the same audit.
3. No handler imports `ApiError` for the purpose of formatting — handlers may still `instanceof`-check for control flow, but message-formatting lives only in `errorMap` entries or `runCommand`. — Enforced by a grep pattern in the audit (`new ApiError\\(` outside `lib/`, `printApiError\\(` outside `lib/`).
4. `cli.js` no longer contains an `ApiError`-formatting branch in its top-level catch. — Enforced by the audit (`grep -n 'ApiError' zombiectl/src/cli.js` returns zero matches outside imports).
5. `cli_command_started` and `cli_command_finished` (or `cli_error`) emit exactly once per dispatch. — Enforced by the existing `run-command.unit.test.js` plus the new registry test.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_registry_every_command_uses_runCommand` | Iterates every registry entry; asserts dispatch uses `runCommand` (mocked) and the handler is reached only through it. |
| `test_registry_every_command_declares_errorMap` | Asserts every entry has a non-null `errorMap` object. |
| `test_errorMap_covers_reachable_codes` | Reads `error-map.fixture.js`; for each `{ command, expectedCodes }`, asserts every expected code is keyed in the command's `errorMap`. |
| `test_runCommand_translates_ApiError_via_errorMap` | Mock handler throws `ApiError` with code `UZ-WS-404`; assert `runCommand` returns 1 AND the `cli_error` event carries `error_code = "WORKSPACE_NOT_FOUND"` AND `writeError` receives the mapped message. |
| `test_runCommand_unmapped_code_falls_through` | Mock handler throws `ApiError` with code `UZ-XX-999` (not in `errorMap`); assert `runCommand` returns 1 AND emits `cli_error` with `error_code = "UZ-XX-999"` AND surfaces the server message verbatim. |
| `test_runCommand_fetch_failed_path` | Mock handler throws `TypeError("fetch failed")`; assert exit 1, `cli_error` with `error_code = "API_UNREACHABLE"`, the message includes `ctx.apiUrl`. |
| `test_runCommand_unexpected_path` | Mock handler throws a plain `Error("boom")`; assert exit 1, `cli_error` with `error_code = "UNEXPECTED"`, the message is the original string. |
| `test_runCommand_emits_lifecycle_triplet` | Successful handler; assert exactly two events fire — `cli_command_started` then `cli_command_finished` — both carrying the registered command name. |
| `test_runCommand_no_double_emit` | Handler that throws `ApiError`; assert `cli_command_finished` does NOT fire and `cli_error` fires exactly once. |
| `test_audit_cli_runcommand_passes_on_clean_tree` | Run `scripts/audit-cli-runcommand.sh`; expect exit 0 on the post-migration tree. |
| `test_audit_cli_runcommand_fails_on_bypass` | Mutate a fixture registry to bypass `runCommand`; assert audit exits 1 with the offending command in stderr. |
| `test_audit_cli_runcommand_fails_on_missing_code` | Mutate a fixture `errorMap` to drop an expected code; assert audit exits 1 listing the missing code. |
| `test_cli_top_level_catch_has_no_ApiError_branch` | Static check on `cli.js` source: asserts no `ApiError` reference outside imports. |
| `test_existing_command_user_visible_strings_unchanged` | Snapshot test: invoke `commandSteer` (and 2-3 other commands) against a stub server returning known UZ-* codes; assert stderr matches the pre-migration snapshot byte-for-byte. Guards against accidental error-string drift. |

Fixtures: `zombiectl/test/commands/error-map.fixture.js`, `zombiectl/test/commands/snapshots/{command}.txt`. Fixture names carry no milestone IDs (RULE TST-NAM).

---

## Acceptance Criteria

- [ ] Every registry entry uses `runCommand` — verify: `scripts/audit-cli-runcommand.sh`
- [ ] Every command declares an `errorMap` covering its endpoints — verify: same audit
- [ ] No `ApiError` reference in `cli.js` outside imports — verify: `grep -n 'ApiError' zombiectl/src/cli.js | grep -v 'import'` returns empty
- [ ] All new and existing CLI tests pass — verify: `cd zombiectl && bun test`
- [ ] Audit wired into `make lint` and clean — verify: `make lint`
- [ ] Snapshot tests for representative commands match the pre-migration stderr — verify: `bun test test/commands/snapshot.unit.test.js`
- [ ] No file over 350 lines added or grown into the cap — verify: `git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350'`
- [ ] `gitleaks detect` clean — verify: `gitleaks detect`
- [ ] `make test` passes — verify: `make test`
- [ ] CHORE(close): `~/Projects/docs/changelog.mdx` carries a new `<Update>` describing the consistent error surface — verify: `git diff ~/Projects/docs/changelog.mdx`

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Audit (the new lint gate)
bash scripts/audit-cli-runcommand.sh && echo "PASS" || echo "FAIL"

# E2: cli.js carries no ApiError formatting branch
( ! grep -nE 'ApiError|printApiError' zombiectl/src/cli.js | grep -v 'import' ) && echo "PASS" || echo "FAIL"

# E3: Tests
cd zombiectl && bun test 2>&1 | tail -10

# E4: Lint (includes the new audit)
make lint 2>&1 | tail -10

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3

# E6: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E7: No milestone IDs in tests/code
bash scripts/audit-milestone-ids.sh 2>&1 | tail -5
```

---

## Dead Code Sweep

After §3 lands, the inline `ApiError` / `fetch failed` formatting in `cli.js` is unreachable. Remove it in the same commit (RULE NDC). No file deletions; only branch removals.

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| Inline `ApiError` branch in `cli.js` top-level catch | `grep -n 'ApiError' zombiectl/src/cli.js \| grep -v import` | 0 matches |
| Inline `fetch failed` branch in `cli.js` top-level catch | `grep -n 'fetch failed' zombiectl/src/cli.js` | 0 matches |
| Per-handler `printApiError` calls | `grep -rn 'printApiError(' zombiectl/src/commands/` | 0 matches (lib uses are fine) |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the migrated registry + `errorMap` paths against this spec's Test Specification. | Returns clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review against this spec, REST guidelines (for UZ-* code coverage), Failure Modes, Invariants. Catches snapshot drift, missing codes, dead branches in `cli.js`. | Returns clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments on the open PR; catches anything `/review` missed post-rebase (registry import path drift, fixture rebase conflicts). | Comments addressed inline. |
| After every push to the PR | `kishore-babysit-prs` | Polls greptile per cadence; classifies findings; suppresses against history. Stops on two consecutive empty polls. | Final report in PR Session Notes. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Audit | `bash scripts/audit-cli-runcommand.sh` | {paste output} | |
| Unit tests | `cd zombiectl && bun test` | {paste output} | |
| Lint | `make lint` | {paste output} | |
| Gitleaks | `gitleaks detect` | {paste output} | |
| 350L gate | `wc -l` | {paste output} | |
| Dead-branch sweep | `grep -n 'ApiError' zombiectl/src/cli.js` | {paste output} | |

---

## Discovery (consult log)

Empty at creation. Populated as Architecture Consult / Legacy-Design Consult fires during implementation.

---

## Out of Scope

- Migrating handlers to a fully `(ctx)`-only signature. Today handlers receive `(ctx, args, workspaces, deps)`; this spec preserves that shape. Reshaping is its own workstream when (and if) the registry-side adapter becomes load-bearing.
- Per-step `try/catch` removal inside long poll loops (login pairing, browser-launch). Those keep their per-step recovery and only their outer dispatch goes through `runCommand`.
- Adding new UZ-* error codes server-side. The `errorMap` mirrors what already exists; new codes are introduced by their own spec and added to the maps as part of that change.
- `streamFetch` / SSE retry policy. SSE has different semantics; today's behavior carries forward unchanged. A future spec may revisit if SSE failures become a hot path.
- Telemetry destination changes. Today the CLI emits to PostHog (`us.i.posthog.com`) with the baked-in default project key; this spec reuses that transport unchanged.
