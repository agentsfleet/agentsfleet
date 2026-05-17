# M71_001: CLI Command Effects — Session Identity, Trace Files, Consent UX

**Prototype:** v1.0.0
**Milestone:** M71
**Workstream:** 001
**Date:** May 16, 2026
**Status:** PENDING
**Priority:** P1 — every operator session is a blind spot in analytics without session continuity; consent UX is frustrating (env-var-only)
**Categories:** CLI
**Batch:** B1 — solo workstream, no sibling dependencies
**Branch:** feat/m71-cli-command-effects (to be created)
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

**Goal (testable):** `session_id` rotates after 30 min of CLI inactivity; `device_id` is permanent and stable across installs; `zombiectl telemetry on|off|status` subcommand controls consent without touching env vars; every command run writes a one-line NDJSON trace to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson`.

**Problem:** Today every `zombiectl` invocation gets a fresh `distinctId` derived from the JWT token — there is no device-level or session-level continuity. Analysing operator behaviour across sessions is a guess. Consent is env-var-only (`DISABLE_TELEMETRY=0`) — no `zombiectl telemetry on` workflow. There's no offline trace file for debugging what happened when the network was down.

**Solution summary:** Three independent additions to the command effects layer, all implemented as additions to the existing `analytics.js` + `state.js` + `run-command.js` boundaries — no handler code changes required:

1. **Session identity** — `state.js` grows a `session.json` file holding `device_id` (UUID, generated once, permanent) and `session_id` (UUID, rotated if last activity > 30 min ago). `runCli()` reads it during lifecycle construction; `runCommand()` bumps `last_activity` on every command boundary. Both IDs are attached as base properties on every PostHog event.

2. **Telemetry consent subcommand** — a new `zombiectl telemetry on|off|status` command tree that reads/writes `session.json`'s `consent` field. The existing `resolveConfig()` in `analytics.js` extends its chain: `DISABLE_TELEMETRY` env → persisted consent → default (still `denied`). The subcommand prints the current consent state and how it was resolved.

3. **NDJSON trace file** — `runCommand()` appends a one-line JSON record per command boundary to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson`. Records include `timestamp`, `command`, `session_id`, `device_id`, `exit_code`, `duration_ms`. A `cleanupTraces()` function deletes files older than 7 days (called at CLI startup, best-effort, never blocks).

None of this changes handler signatures, HTTP client wiring, or the Commander tree structure.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/lib/state.js` | EDIT | Add `session.json` read/write, path resolution for `traces/` dir |
| `zombiectl/src/lib/analytics.js` | EDIT | Extend consent chain, expose session properties for base event fields |
| `zombiectl/src/lib/run-command.js` | EDIT | Append NDJSON trace record per command execution |
| `zombiectl/src/commands/telemetry.js` | CREATE | `telemetry on|off|status` handler implementations |
| `zombiectl/src/program/handlers-bind.js` | EDIT | Wire `telemetry` handler tree into `buildHandlers()` |
| `zombiectl/src/program/cli-tree.js` | EDIT | Add `telemetry` command + subcommands to Commander tree |
| `zombiectl/src/cli.js` | EDIT | Load session identity during lifecycle construction; call `cleanupTraces()` at startup |
| `zombiectl/src/constants/analytics-events.js` | EDIT | Add new event name constants for consent-change events |
| `zombiectl/test/analytics.unit.test.js` | EDIT | Test consent chain, session TTL, identity generation |
| `zombiectl/test/state.unit.test.js` | EDIT | Test session.json read/write, TTL rotation |
| `zombiectl/test/run-command.unit.test.js` | EDIT | Test NDJSON trace writes |
| `zombiectl/test/acceptance/flags-and-env.spec.js` | EDIT | Test `zombiectl telemetry on|off|status` output |
| `zombiectl/test/acceptance/command-matrix.js` | EDIT | Add telemetry subcommand to expected command list |

---

## Sections (implementation slices)

### §1 — Session Identity (`session.json` + `device_id` / `session_id`)

What this delivers: `state.js` gains `loadSession()` and `saveSession()` functions operating on `~/.config/zombiectl/session.json`. The file shape:

```json
{
  "device_id": "<uuid>",
  "session_id": "<uuid>",
  "last_activity": <iso-8601>,
  "consent": "granted" | "denied"
}
```

On first load, `device_id` and `session_id` are generated fresh. On subsequent loads, `device_id` is kept; `session_id` is regenerated if `last_activity` is older than `SESSION_TIMEOUT_MS` (30 min). `last_activity` is bumped by `runCommand()` on every command boundary (pre-handler, before the `cli_command_started` event).

Implementation default: `SESSION_TIMEOUT_MS = 30 * 60 * 1000` from Supabase's `identity.ts`. File permissions mirror `credentials.json` (0o600).

### §2 — Consent Chain Extension

What this delivers: `resolveConfig()` in `analytics.js` changes from a single env-var check to a three-tier chain:

1. `DISABLE_TELEMETRY` env var (takes priority — existing behaviour)
2. `session.json` `consent` field (new tier)
3. Default: `denied` (existing behaviour, unchanged)

`session.json` is read via `loadSession()` at config resolution time. The `resolveConfig()` return adds a `consentSource` field (`"env"` | `"session_file"` | `"default"`) so the `telemetry status` command can explain why the current state is what it is.

### §3 — `zombiectl telemetry` Subcommand

What this delivers: a new `telemetry` command with three subcommands:

```
zombiectl telemetry on     → sets consent=granted in session.json, emits consent_changed event
zombiectl telemetry off    → sets consent=denied in session.json, emits consent_changed event
zombiectl telemetry status → prints current consent state + source tier
```

Status output shape (human mode):
```
Telemetry: enabled (source: session file)
  To change: zombiectl telemetry on|off
  Override:  DISABLE_TELEMETRY=0 (env var takes priority)
```

Status output shape (JSON mode):
```json
{"telemetry":{"enabled":true,"source":"session_file","override_available":"DISABLE_TELEMETRY"}}
```

The `on` and `off` subcommands write consent to `session.json`. They do NOT require auth (consent is a local-machine decision). The analytics event for consent changes uses the existing client (if telemetry was enabled before the flip, the event fires before the flip; if it was disabled, no event fires).

Implementation default: the `telemetry` command is exempt from the auth guard (like `login`). Use `zombiectl/src/commands/telemetry.js` as the handler file, mirroring the pattern from `src/commands/core.js` (export handler functions + error maps).

### §4 — NDJSON Trace Files

What this delivers: `runCommand()` appends one line to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson` for every command execution. Record shape:

```json
{"ts":"<iso-8601>","command":"<name>","session_id":"<uuid>","device_id":"<uuid>","exit_code":0,"duration_ms":1234}
```

Written after the handler resolves (both success and failure paths). The file is opened in append mode; the write is best-effort — if disk is full or permissions fail, the trace is silently dropped (telemetry must never break CLI UX).

`cleanupTraces()` scans `~/.config/zombiectl/traces/` for files matching `YYYY-MM-DD.ndjson` and deletes any older than `TRACE_RETENTION_DAYS` (7 days). Called once at CLI startup in `runCli()`, best-effort, never blocks.

Implementation default: `TRACE_RETENTION_DAYS = 7` from Supabase's `tracing.layer.ts`. Use `fs.appendFile` for atomic-ish appends. No file locking — single-invocation CLI, no concurrent writers.

---

## Interfaces

**New internal functions (state.js):**
```
loadSession() → Promise<{ device_id: string, session_id: string, last_activity: string|null, consent: string|null }>
saveSession(session) → Promise<void>
cleanupTraces(baseDir) → Promise<void>   // idempotent, best-effort
```

**New internal functions (analytics.js):**
```
resolveConfig(env, sessionConsent) → { key, host, enabled, consentSource }
  // consentSource ∈ {"env" | "session_file" | "default"}
```

**New CLI commands:**
```
zombiectl telemetry on       → exit 0 on success
zombiectl telemetry off      → exit 0 on success
zombiectl telemetry status   → exit 0, prints state to stdout
```

**Event name constants (new in analytics-events.js):**
```
EVT_CONSENT_CHANGED   = "consent_changed"
```

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

1. `device_id` is generated once and never rotated — enforced by `loadSession()` guarding the generate path behind `!existing.device_id`.
2. Telemetry defaults to `disabled` — enforced by `resolveConfig()` defaulting `disabled: true` when no env var and no session consent exist.
3. Trace files never block command execution — enforced by try/catch around every `fs.appendFile` call in `runCommand()`.
4. `session.json` is always 0o600 — enforced by `writeJson()` using `{ mode: 0o600 }` (existing pattern in `state.js`).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_loadSession_first_run_generates_both_ids` | Fresh load with no session.json → returns new device_id (UUID) and session_id (UUID), both non-empty and distinct |
| `test_loadSession_keeps_device_id_on_reload` | Second load → device_id matches first load; session_id may rotate if TTL expired |
| `test_loadSession_rotates_session_after_ttl` | last_activity > 30 min ago → session_id is new UUID, device_id unchanged |
| `test_loadSession_preserves_session_within_ttl` | last_activity < 30 min ago → session_id unchanged, device_id unchanged |
| `test_resolveConfig_env_overrides_session` | DISABLE_TELEMETRY=0 env + denied in session.json → enabled=true, consentSource="env" |
| `test_resolveConfig_session_wins_without_env` | No DISABLE_TELEMETRY env + granted in session.json → enabled=true, consentSource="session_file" |
| `test_resolveConfig_defaults_to_denied` | No env, no session file → enabled=false, consentSource="default" |
| `test_telemetry_status_shows_source` | After `telemetry on` → stdout contains "enabled" and "source: session file" |
| `test_telemetry_on_writes_consent` | `telemetry on` → session.json consent field is "granted" |
| `test_telemetry_off_writes_consent` | `telemetry off` → session.json consent field is "denied" |
| `test_telemetry_status_json_mode` | --json flag → stdout is valid JSON with telemetry.enabled and telemetry.source |
| `test_runCommand_writes_trace_on_success` | Handler returns 0 → trace file has one line with exit_code=0, duration_ms > 0 |
| `test_runCommand_writes_trace_on_error` | Handler throws ApiError → trace file has one line with exit_code=1, duration_ms > 0 |
| `test_runCommand_trace_survives_disk_full` | Stub fs.appendFile to reject → runCommand still returns correct exit code, no throw |
| `test_cleanupTraces_deletes_old_files` | Trace file with date 8 days ago → cleanup removes it; file with date 1 day ago → preserved |
| `test_cleanupTraces_never_throws` | Stub fs.readdir to reject → cleanup returns without throwing |
| `test_telemetry_on_noop_when_already_granted` | `telemetry on` twice → second call succeeds, consent still "granted" |
| `test_session_properties_on_events` | After loadSession() → cli_command_started payload includes session_id and device_id |

---

## Acceptance Criteria

- [ ] `make lint` clean — verify: `cd zombiectl && bun run lint`
- [ ] `bun test` passes — verify: `cd zombiectl && bun test`
- [ ] Acceptance tests pass — verify: `cd zombiectl && bun run test:acceptance`
- [ ] `zombiectl telemetry status` prints correct consent state — verify: run with no prior state, then `zombiectl telemetry on`, then `zombiectl telemetry status`
- [ ] Session file creates at `~/.config/zombiectl/session.json` with 0o600 — verify: `ls -la ~/.config/zombiectl/session.json`
- [ ] Trace file written to `~/.config/zombiectl/traces/YYYY-MM-DD.ndjson` — verify: run any command, check file exists with one JSON line
- [ ] `gitleaks detect` clean
- [ ] No file over 350 lines added

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Lint
cd zombiectl && bun run lint && echo "PASS" || echo "FAIL"

# E2: Unit tests
cd zombiectl && bun test && echo "PASS" || echo "FAIL"

# E3: Acceptance tests
cd zombiectl && bun run test:acceptance && echo "PASS" || echo "FAIL"

# E4: Session file permissions
test "$(stat -f '%Mp%Lp' ~/.config/zombiectl/session.json)" = "0600" && echo "PASS: 0o600" || echo "FAIL: wrong perms"

# E5: Trace file exists after command
cd zombiectl && node ./bin/zombiectl.js zombie list --help > /dev/null 2>&1
ls ~/.config/zombiectl/traces/$(date +%Y-%m-%d).ndjson > /dev/null 2>&1 && echo "PASS" || echo "FAIL"

# E6: Telemetry status
cd zombiectl && node ./bin/zombiectl.js telemetry status && echo "PASS" || echo "FAIL"

# E7: Gitleaks
gitleaks detect 2>&1 | tail -3

# E8: 350-line gate
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

- **OTel/Sentry/Grafana remote trace export** — this spec only adds local NDJSON files. Remote export is a future spec.
- **Per-command span nesting** (child spans for HTTP calls inside a command) — Supabase does this with Effect-TS; zombiectl doesn't have the middleware layer for it yet.
- **Consent prompt on first run** (interactive opt-in like `npm`) — CLI is machine-first; consent is default-off with explicit `telemetry on` or env var.
- **Session analytics dashboard in PostHog** — adding `device_id`/`session_id` properties to events; building the dashboard is a separate PostHog configuration task.
