# M71_001: CLI Command Effects — Session Identity, Trace Files, Consent UX

**Prototype:** v1.0.0
**Milestone:** M71
**Workstream:** 001
**Date:** May 16, 2026
**Status:** DONE
**Priority:** P1 — every operator session is a blind spot in analytics without session continuity; consent UX is frustrating (env-var-only)
**Categories:** CLI
**Batch:** B1 — solo workstream, no sibling dependencies
**Branch:** feat/m71-001-cli-command-effects
**Depends on:** M63_002_P1_CLI_TELEMETRY_CONSENT (analytics consent baseline), M63_006_P2_CLI_OBS_RUNCOMMAND_MIGRATION (runCommand boundary)
**Provenance:** human-written (spec authored from Supabase CLI codebase audit)

**Canonical architecture:** `docs/ARCHITECTURE.md` §N/A — zombiectl layer; no architecture doc section covers it yet. Supabase CLI reference: `~/Projects/oss/cli/apps/cli/src/shared/telemetry/` for identity + tracing patterns.

---

## Implementing agent — read these first

1. `zombiectl/src/cli.js` — entrypoint, lifecycle object construction, post-action analytics, Commander wiring. This is where session identity gets attached.
2. `zombiectl/src/lib/state.js` — persistent filesystem state (`~/.config/zombiectl/`). The new `session.json` file lands here.
3. `zombiectl/src/lib/analytics.js` — PostHog client creation, event emits, context helpers, consent resolution. The consent chain and session properties flow through here.
4. `zombiectl/src/lib/run-command.js` — per-command boundary: `cli_command_started`/`cli_command_finished`/`cli_error` events. Trace file writes attach here.
5. **Supabase reference** (do NOT copy TyepScript patterns; convert to the repo's JS/Commander style): `~/Projects/oss/cli/apps/cli/src/shared/telemetry/identity.ts` (device_id + session_id with TTL), `~/Projects/oss/cli/apps/cli/src/shared/telemetry/consent.ts` (consent resolution chain), `~/Projects/oss/cli/apps/cli/src/shared/telemetry/tracing.layer.ts` (NDJSON trace export).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE UFS (named constants for repeat literals), RULE NDC (no dead code at write time), RULE NLR (touch-it-fix-it cleanup)
- **`scripts/audit-const-names.mjs`** — enforces no raw event-name strings outside `analytics-events.js`
- **`scripts/audit-runtime-imports.mjs`** — enforces no runtime-only imports in core paths

---

## Overview

**Goal (testable):** `session_id` rotates after 30 min of CLI inactivity; `device_id` is permanent and stable across installs; every command run writes a one-line NDJSON trace to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson`; both IDs flow as base properties on every PostHog event.

**Problem:** Today every `zombiectl` invocation gets a fresh `distinctId` derived from the JWT token — there is no device-level or session-level continuity. Analysing operator behaviour across sessions is a guess. There's no offline trace file for debugging what happened when the network was down.

**Solution summary:** Two independent additions to the command effects layer, all implemented as additions to the existing `analytics.ts` + `state.ts` + `run-command.ts` boundaries — no handler code changes required:

1. **Session identity** — `state.ts` grows a `session.json` file holding `device_id` (UUID, generated once, permanent) and `session_id` (UUID, rotated if last activity > 30 min ago). `runCli()` reads it during lifecycle construction; subsequent runs bump `last_activity`. Both IDs are attached as base properties on every PostHog event.

2. **NDJSON trace file** — `runCommand()` appends a one-line JSON record per command boundary to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson`. Records include `ts`, `command`, `session_id`, `device_id`, `exit_code`, `duration_ms`. A `cleanupTraces()` helper deletes files older than 7 days (called at CLI startup, best-effort, never blocks).

None of this changes handler signatures, HTTP client wiring, or the Commander tree structure. Consent stays env-var-only (`DISABLE_TELEMETRY=0` opts in) — the "telemetry on/off/status" subcommand and three-tier consent chain originally in §2/§3 of this spec were dropped per Captain decision on May 17, 2026 (operator can already toggle via env var; a new command surface adds discoverability cost without a behaviour win).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/lib/state.ts` | EDIT | Add `Session` type, `loadSession`/`saveSession`/`cleanupTraces`/`appendTrace`, path resolution for `traces/` dir |
| `zombiectl/src/lib/run-command.ts` | EDIT | Capture duration, attach session_id/device_id as base props on events, append NDJSON trace record per command boundary |
| `zombiectl/src/cli.ts` | EDIT | Load session at startup, pass session_id/device_id into ctx, fire cleanupTraces() best-effort |
| `zombiectl/test/state.unit.test.ts` | EDIT | Session.json read/write, TTL rotation, cleanupTraces, appendTrace |
| `zombiectl/test/run-command.unit.test.ts` | EDIT | Trace writes on success/error paths, session/device base props on events |
| `zombiectl/test/cli-analytics.unit.test.ts` | EDIT | Updated event-shape expectations to include session_id/device_id base props |

---

## Sections (implementation slices)

### §1 — Session Identity (`session.json` + `device_id` / `session_id`) ✅ DONE

What this delivers: `state.ts` gains `loadSession()` and `saveSession()` functions operating on `~/.config/zombiectl/session.json`. The file shape:

```json
{
  "device_id": "<uuid>",
  "session_id": "<uuid>",
  "last_activity": <ms-epoch>
}
```

On first load, `device_id` and `session_id` are generated fresh. On subsequent loads, `device_id` is kept; `session_id` is regenerated if `last_activity` is older than `SESSION_TIMEOUT_MS` (30 min). `last_activity` is bumped **once per CLI invocation at `runCli()` startup** (fire-and-forget, best-effort) — zombiectl runs one handler per invocation, so a startup bump is exactly one bump per command boundary. Per-`runCommand` bumps would be redundant writes with no behaviour change.

Implementation default: `SESSION_TIMEOUT_MS = 30 * 60 * 1000` from Supabase's `identity.ts`. File permissions mirror `credentials.json` (0o600).

### §2 — Consent Chain Extension (DROPPED)

Original §2 proposed a three-tier consent chain (env → session-file → default) plus a `consentSource` discriminator. Dropped per Captain decision (May 17, 2026): `DISABLE_TELEMETRY=0` is already a sufficient surface; persisting consent to session.json adds discoverability cost without a behaviour win. `resolveConfig()` keeps its env-only check.

### §3 — `zombiectl telemetry` Subcommand (DROPPED)

Original §3 proposed a `telemetry on|off|status` command tree. Dropped per Captain decision (May 17, 2026) for the same reason: the env-var path is the consent surface; a new CLI command would require operator discovery of a flow that `DISABLE_TELEMETRY=0` already satisfies. `commands/telemetry.ts` is not created. `AUTH_EXEMPT` stays `{"login"}`.

### §4 — NDJSON Trace Files ✅ DONE

What this delivers: `runCommand()` appends one line to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson` for every command execution. Record shape:

```json
{"ts":"<iso-8601>","command":"<name>","session_id":"<uuid>","device_id":"<uuid>","exit_code":0,"duration_ms":1234}
```

Written after the handler resolves (both success and failure paths). The file is opened in append mode; the write is best-effort — if disk is full or permissions fail, the trace is silently dropped (telemetry must never break CLI UX).

`cleanupTraces()` scans `~/.config/zombiectl/traces/` for files matching `YYYY-MM-DD.ndjson` and deletes any older than `TRACE_RETENTION_DAYS` (7 days). Called once at CLI startup in `runCli()`, best-effort, never blocks.

Implementation default: `TRACE_RETENTION_DAYS = 7` from Supabase's `tracing.layer.ts`. Use `fs.appendFile` for atomic-ish appends. No file locking — single-invocation CLI, no concurrent writers.

---

## Interfaces

**New internal functions (state.ts):**
```
loadSession() → Promise<{ device_id: string, session_id: string, last_activity: number|null }>
saveSession(session) → Promise<void>
appendTrace(record) → Promise<void>      // best-effort, no throw
cleanupTraces(tracesDir?) → Promise<void> // idempotent, best-effort
```

**New base properties on every PostHog event** (added by `run-command.ts` buildProps):
```
session_id: <uuid>   // when handlerCtx.session_id is present
device_id: <uuid>    // when handlerCtx.device_id is present
```

**No new CLI commands.** Consent stays env-var only (`DISABLE_TELEMETRY=0`).
**No new event-name constants.** No event is introduced by this milestone.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Session file unreadable | Disk full, permission denied, corrupt JSON | Fall back to fresh session (new device_id + session_id). Log nothing — telemetry is non-critical. |
| Trace file unwritable | Disk full, permission denied | Silently skip the append. No error surfaced to operator. |
| Trace cleanup fails | Permission denied on old trace files | Swallow error. Old files remain until next cleanup succeeds. |
| `telemetry on` called when already granted | Operator runs `on` twice | Idempotent. Re-writes consent + emits event (harmless duplicate). |
| `telemetry on` called with PostHog unavailable | Network down, bad key | Consent still written to session.json. The `consent_changed` event is best-effort — if PostHog client is null, skip the event. |
| Session file race | Two CLI invocations in rapid succession (same machine) | Last write wins. `last_activity` will be the timestamp of whichever process wrote last. Acceptable for a single-user CLI. |

---

## Invariants

1. `device_id` is generated once and never rotated — enforced by `loadSession()` only generating when `validString(raw.device_id)` returns null.
2. Telemetry defaults to `disabled` — enforced by `resolveConfig()` reading only `DISABLE_TELEMETRY` env var (unchanged from pre-spec behaviour).
3. Trace writes never block command execution — enforced by `appendTrace()` wrapping every fs call in try/catch and never rethrowing.
4. `session.json` is always 0o600 — enforced by `writeJson()` using `{ mode: 0o600 }` (existing pattern in `state.ts`).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_loadSession_first_run_generates_both_ids` | Fresh load with no session.json → returns new device_id (UUID) and session_id (UUID), both non-empty and distinct |
| `test_loadSession_keeps_device_id_on_reload` | Pre-written session.json with device_id "dev_X" → loadSession returns same device_id |
| `test_loadSession_rotates_session_after_ttl` | last_activity > 30 min ago → session_id is new UUID, device_id unchanged |
| `test_loadSession_preserves_session_within_ttl` | last_activity < 30 min ago → session_id unchanged, device_id unchanged |
| `test_loadSession_recovers_from_corrupt_file` | session.json is invalid JSON → loadSession returns fresh identity, no throw |
| `test_saveSession_writes_file_mode_0600` | After saveSession({...}), file exists with 0o600 |
| `test_runCommand_writes_trace_on_success` | Handler returns 0 → today's trace file has one line with exit_code=0, command name matches |
| `test_runCommand_writes_trace_on_apierror` | Handler throws ApiError → trace line has exit_code=1 |
| `test_runCommand_trace_survives_disk_full` | Stub state's appendTrace path (read-only dir or chmod) → runCommand still returns correct exit code, no throw |
| `test_runCommand_attaches_session_props` | handlerCtx.session_id/device_id set → cli_command_started + cli_command_finished payloads include both as base props |
| `test_cleanupTraces_deletes_old_files` | Pre-create trace file with date 8 days ago + one with today → cleanup removes the old, preserves today |
| `test_cleanupTraces_never_throws` | tracesDir does not exist → cleanup resolves without throwing |
| `test_session_properties_on_events_via_runCli` | runCli → cli_command_started payload includes session_id and device_id from loaded session |

---

## Acceptance Criteria

- [ ] `bun run lint` clean — verify: `(cd zombiectl && bun run lint)`
- [ ] `bun run typecheck` clean — verify: `(cd zombiectl && bun run typecheck)`
- [ ] `bun test` baseline + new rows pass — verify: `(cd zombiectl && bun test)`
- [ ] Acceptance tests pass — verify: `(cd zombiectl && bun run test:acceptance)`
- [ ] Session file creates at `$ZOMBIE_STATE_DIR/session.json` with 0o600 — verify: run any command, then `ls -la $ZOMBIE_STATE_DIR/session.json`
- [ ] Trace file written to `$ZOMBIE_STATE_DIR/traces/YYYY-MM-DD.ndjson` — verify: run any command, `ls` the file, `wc -l` is ≥1
- [ ] `gitleaks detect` clean
- [ ] No file over 350 lines added

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Lint (oxlint + audit-runtime-imports + audit-const-names + tsc)
(cd zombiectl && bun run lint) && echo "PASS" || echo "FAIL"

# E2: Unit tests
ZOMBIE_STATE_DIR=$(mktemp -d) bash -c '(cd zombiectl && bun test)' && echo "PASS" || echo "FAIL"

# E3: Session file permissions
DIR=$(mktemp -d) && ZOMBIE_STATE_DIR=$DIR node zombiectl/bin/zombiectl.js --help > /dev/null 2>&1
test "$(stat -f '%Mp%Lp' "$DIR/session.json")" = "0600" && echo "PASS: 0o600" || echo "FAIL"

# E4: Trace file exists with one JSON line per command
DIR=$(mktemp -d) && ZOMBIE_STATE_DIR=$DIR node zombiectl/bin/zombiectl.js auth status > /dev/null 2>&1
TRACE="$DIR/traces/$(date +%Y-%m-%d).ndjson"
test -f "$TRACE" && head -1 "$TRACE" | python3 -c 'import sys,json; json.loads(sys.stdin.read())' && echo "PASS" || echo "FAIL"

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3

# E6: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'
```

---

## Dead Code Sweep

N/A — no files deleted. Existing functions (`resolveConfig`, `runCommand`) are extended, not replaced.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits test coverage of the diff against this spec's Test Specification. | Skill returns clean. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec, Failure Modes, Invariants. | Skill returns clean OR every finding dispositioned. |
| After `gh pr create` opens the PR | `/review-pr` | Review-comments the open PR. | Comments addressed inline. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `cd zombiectl && bun test` | | |
| Acceptance tests | `cd zombiectl && bun run test:acceptance` | | |
| Lint | `cd zombiectl && bun run lint` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` | | |

---

## Out of Scope

- **`zombiectl telemetry on|off|status` subcommand** — dropped per Captain decision May 17, 2026. `DISABLE_TELEMETRY=0` env var is already the consent surface; a new CLI command added discoverability cost without a behaviour win.
- **Three-tier consent chain in `analytics.ts`** (env → session.json → default) — dropped with the subcommand. `resolveConfig()` stays env-only.
- **`session.json.consent` field** — not persisted; consent lives only in the env.
- **OTel/Sentry/Grafana remote trace export** — this spec only adds local NDJSON files. Remote export is a future spec.
- **Per-command span nesting** (child spans for HTTP calls inside a command) — Supabase does this with Effect-TS; zombiectl doesn't have the middleware layer for it yet.
- **Server-side persistence of `device_id`/`session_id`** — purely client-side identity. No zombied schema change, no API endpoint touches these. Egress is to PostHog only when DISABLE_TELEMETRY=0.
- **Session analytics dashboard in PostHog** — adding `device_id`/`session_id` to events is what this spec ships; building the dashboard is a separate PostHog configuration task.
