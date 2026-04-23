# M19_003: `zombiectl install --from <path>` — Unify Install Under One Command

**Prototype:** v2.0.0
**Milestone:** M19
**Workstream:** 003
**Date:** Apr 23, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — unblocks the platform-ops sample's acceptance script (M37_001). Also collapses the CLI install surface for every future sample.
**Batch:** B2 — schedule before M37_001 can close its §4 dims.
**Branch:** feat/m19-003-install-from-path
**Depends on:** M19_001 (DONE — `POST /v1/workspaces/{ws}/zombies` accepts `{source_markdown, trigger_markdown}` and returns `{zombie_id, webhook_url}`).

---

## Why this workstream exists

Today the CLI has two install paths:

1. `zombiectl install <template>` — **local scaffolder**. Copies `zombiectl/templates/<name>/{SKILL.md,TRIGGER.md}` into `./<name>/` in cwd. No HTTP.
2. `zombiectl up` — reads a zombie dir from cwd + POSTs to `/v1/workspaces/{ws}/zombies`. Server registers **and activates atomically** (see `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md §3, §7, §8-inv-#1`: INSERT → XGROUP CREATE → XADD `zombie:control` → 201; watcher spawns the zombie thread before the ACK).

The server activates on create — no separate start/trigger step. That makes the install/up split ceremony. And the bundled templates (`lead-collector`, `slack-bug-fixer`) are v1-era demo helpers; `samples/lead-collector` is already absent from the repo and M39_001 tears down the last references.

This workstream collapses both commands into **one**: `zombiectl install --from <path>`. It reads the two files off disk, POSTs, and the zombie is live by the time the command returns. No bundled templates, no `zombie up`. Samples ship as directories in `samples/` and operators install them with one command.

---

## Goal (testable)

From a fresh checkout with a seeded vault:

```
$ zombiectl install --from samples/homelab
🎉 homelab is live.
  Zombie ID: zom_01xyz
```

Invariants:

- Zombie is active by the time the command returns. Verified by `zombiectl status` listing it immediately (worker thread is consuming `zombie:{id}:events`).
- No writes to cwd.
- Webhook URL not printed in pretty mode. Available via `zombiectl status` / `zombiectl list`. Included in `--json` output for scripting parity.

Post-conditions on the install surface:

- `zombiectl up` does not exist — returns unknown-command with suggestion.
- `zombiectl install` (no flags, no positional) prints usage pointing at `--from <path>`.
- `zombiectl install <anything-without-dashes>` (old bundled usage) prints usage error.
- `zombiectl/templates/` directory is gone; `BUNDLED_TEMPLATES` constant is gone.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie.js` | EDIT | Delete `commandUp`. Delete `BUNDLED_TEMPLATES`, `TEMPLATES_DIR` + template-copy code. Rewrite `commandInstall` to be `--from`-only (reads files via the new loader, calls the new submit helper). Remove `up` dispatch from `commandZombie`. Update usage strings + error-message hints that reference the old flow (e.g., `status` empty-list hint, `up` "no zombie directory" message). Pass `workspaces` through to `commandInstall`. |
| `zombiectl/src/cli.js` | EDIT | Delete `zombieUp` handler block. |
| `zombiectl/src/program/routes.js` | EDIT | Delete the `{ key: "zombie.up", match: ... }` row. |
| `zombiectl/src/program/command-registry.js` | EDIT | Delete `"zombie.up": handlers.zombieUp`. |
| `zombiectl/src/program/suggest.js` | EDIT | Delete `"up"` from the completion list. |
| `zombiectl/src/lib/load-skill-from-path.js` | CREATE | Reads `<path>/{SKILL.md,TRIGGER.md}`; returns `{ skill_md, trigger_md, name }` (name = first `^name:` line of TRIGGER.md, fallback `basename(path)`). Throws typed errors `ERR_PATH_NOT_FOUND`, `ERR_SKILL_MISSING`, `ERR_TRIGGER_MISSING`. ≤100 lines. |
| `zombiectl/src/lib/submit-zombie.js` | CREATE | POST + response-parse helper. Input: `(ctx, workspaces, deps, { skill_md, trigger_md })`. Output: `{ zombie_id, webhook_url, name }` on success; writes errors via `deps.writeError` on failure. ~50 lines. Sole caller: `commandInstall`. |
| `zombiectl/templates/` | DELETE | The entire bundled-template directory (`lead-collector/`, `slack-bug-fixer/`). |
| `zombiectl/test/zombie-up-woohoo.unit.test.js` | DELETE | Tests a command being removed. |
| `zombiectl/test/zombie-install-from-path.unit.test.js` | CREATE | Unit tests per dims below. |
| `samples/homelab/README.md` | EDIT | Currently says `zombiectl up if running locally` in the Prerequisites block (conflates zombiectl with `zombied` daemon startup). Rewrite to the correct daemon start + point the sample install at `zombiectl install --from samples/homelab`. |
| `docs/v2/surfaces.md` | EDIT | Remove `zombiectl up` + `install <template>` rows; add `install --from`. |

**Explicitly out of scope in this file-list:**

- Backend endpoints, OpenAPI rows, web UI install form (M19_001).
- `samples/lead-collector/` — absent from repo already; M39_001 owns its spec teardown.
- Historical files under `docs/nostromo/` and `docs/v2/done/` — frozen per spec discipline; no edits.
- `zombied` daemon start/stop surfaces — different concern.

---

## Sections

### §1 — New flow: `install --from <path>`

**Dimensions:**

- 1.1 DONE — target: `zombiectl install --from samples/homelab` with seeded auth + workspace
  - expected: reads both files, POSTs, prints `🎉 homelab is live.` + `Zombie ID: zom_...`, exits 0; webhook URL not printed in pretty mode
  - test_type: unit (API mocked)
- 1.2 DONE — target: `install --from ./missing-dir`
  - expected: exits 1 with `ERR_PATH_NOT_FOUND: <path>`; no POST fired
  - test_type: unit
- 1.3 DONE — target: `install --from <path>` where only `SKILL.md` exists
  - expected: exits 1 with `ERR_TRIGGER_MISSING: <path>/TRIGGER.md`; no POST fired
  - test_type: unit
- 1.4 DONE — target: `install --from <path>` where only `TRIGGER.md` exists
  - expected: exits 1 with `ERR_SKILL_MISSING: <path>/SKILL.md`; no POST fired
  - test_type: unit
- 1.5 DONE — target: `install --from <path>` where TRIGGER.md has no `^name:` line
  - expected: POST fires with content; display name = `basename(path)`; exits 0
  - test_type: unit

### §2 — Server response handling

**Dimensions:**

- 2.1 DONE — target: install twice from the same path in the same workspace
  - expected: second call surfaces the server's 409 (ZOMBIE_NAME_CONFLICT) as a clear CLI error citing the existing zombie name; exit 1
  - test_type: unit (API mock returns 409)
- 2.2 DONE — target: `install --from <path>` with network failure (request throws)
  - expected: exits 1 with `IO_ERROR` via `writeError`; no partial success print
  - test_type: unit

### §3 — Shared pre-flight

**Dimensions:**

- 3.1 DONE — target: `install --from <path>` when not authenticated
  - expected: exits 1 with `NOT_AUTHENTICATED`; no POST fired
  - test_type: unit
- 3.2 DONE — target: `install --from <path>` when no workspace selected
  - expected: exits 1 with `NO_WORKSPACE`; no POST fired
  - test_type: unit
- 3.3 DONE — target: `install --from <path> --json`
  - expected: stdout is a single JSON object `{"status":"installed","zombie_id":"zom_...","webhook_url":"https://..."}`; no pretty prose; exits 0
  - test_type: unit

### §4 — Removed commands (teardown)

**Dimensions:**

- 4.1 DONE — target: `zombiectl up` (any args)
  - expected: exits 2 (unknown command), suggest output does not list `up`
  - test_type: unit (route resolver + suggest)
- 4.2 DONE — target: `zombiectl install` (no flags, no positional)
  - expected: exits 2 with usage pointing at `--from <path>`; no POST fired
  - test_type: unit
- 4.3 DONE — target: `zombiectl install lead-collector` (old bundled usage)
  - expected: exits 2 with usage error; no POST fired; message points at `--from <path>`
  - test_type: unit
- 4.4 DONE — target: post-merge filesystem
  - expected: `zombiectl/templates/` absent; `zombiectl/test/zombie-up-woohoo.unit.test.js` absent
  - test_type: filesystem grep (manual acceptance before CHORE(close))

### §5 — Error-message + hint sweep

**Dimensions:**

- 5.1 DONE — target: `zombiectl status` with no zombies
  - expected: hint says `zombiectl install --from <path>`; no `<template>` token, no `zombiectl up`
  - test_type: unit (assert on stderr/stdout)
- 5.2 DONE — target: repo-wide residue
  - expected: `grep -rn 'zombiectl up\|install <template>\|BUNDLED_TEMPLATES' zombiectl/src/ samples/ docs/v2/pending/ docs/v2/active/` returns no live references (historical `docs/v2/done/` + `docs/nostromo/` exempt)
  - test_type: CI grep (or CHORE(close) check)

---

## Acceptance Criteria

- [ ] `zombiectl install --from samples/homelab` against a running `zombied` returns 201 and prints the zombie ID — manual smoke
- [ ] `zombiectl status` shows the zombie active immediately after install — confirms liveness per `ARCHITECTURE_ZOMBIE_EVENT_FLOW.md §7`
- [ ] `zombiectl up` returns unknown-command
- [ ] `zombiectl install` (no args) returns usage pointing at `--from`
- [ ] All §1–§5 dims green — `bun test` in `zombiectl/`
- [ ] `zombiectl/templates/` is gone; `grep -rn 'BUNDLED_TEMPLATES' zombiectl/src` returns nothing
- [ ] No changes to `POST /v1/workspaces/{ws}/zombies` shape — `grep -n "POST.*zombies" public/openapi/paths/zombies.yaml` shows unchanged
- [ ] M37_001 §4.1 (install from `samples/platform-ops`) can now execute

---

## Out of Scope

- Backwards-compat aliases for `zombie up` / `install <template>`. Pre-v2.0 teardown per repo memory (`feedback_pre_v2_api_drift.md`) — removed commands return unknown-command, not deprecation warnings.
- Multi-file skill bundles (`SKILL.md` + child files). Two-file sample is the MVP surface.
- Client-side validation of cron / tool / credential names — backend is authority.
- `zombied` daemon startup/shutdown surfaces.
- M39_001 `samples/lead-collector/` teardown — different dir, different workstream.

---

## Eval Commands

```bash
cd zombiectl && bun test test/zombie-install-from-path.unit.test.js   # new dims
cd zombiectl && bun test                                              # full regression
cd zombiectl && bun bin/zombiectl.js install --from ../samples/homelab  # smoke
zombiectl status                                                      # confirm live
zombiectl up                                                          # expect unknown-command
```

---

## Discovery (findings during implementation)

- **M39_001 scope overlap.** M39_001 (`docs/v2/pending/P1_API_CLI_UI_M39_001_LEAD_COLLECTOR_SAMPLE_TEARDOWN.md`) prescribes removing `"lead-collector"` from `BUNDLED_TEMPLATES` (§3.3 / invariant 3 / E3). M19_003 deletes `BUNDLED_TEMPLATES` entirely and removes `zombiectl/templates/lead-collector/`. Those M39_001 dims are now obsolete. Remaining M39_001 scope: renaming `lead-collector` fixture strings in Zig tests, UI test files, and marketing copy (`ui/packages/website/src/pages/Agents.tsx`). M39_001 owner should amend their spec before starting.
- **OpenAPI stale reference.** `public/openapi/paths/zombies.yaml:15` said "Obtained from `POST /v1/zombies` or `zombiectl up`" — both wrong (path moved workspace-scoped in M24_001; `zombiectl up` removed this workstream). Updated in this workstream to `POST /v1/workspaces/{workspace_id}/zombies` and `zombiectl install --from <path>`.
- **`zombiectl/src/program/io.js:46` help text** referenced `install <template>` + `up [<path>]`. Updated to `install --from <path>`; `up` row removed. Caught by the §5.2 residue grep, not by the original file-change table.
- **`samples/homelab/README.md` conflated `zombiectl up` with starting the `zombied` daemon.** Correct daemon start is `zombied serve` per `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md §1`. Step 3 now fetches the webhook URL from `zombiectl status --json` instead of assuming it was printed by install.
