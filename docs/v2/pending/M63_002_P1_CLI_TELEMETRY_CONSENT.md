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

# M63_002: zombiectl telemetry consent on first login

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 002
**Date:** May 06, 2026
**Status:** PENDING
**Priority:** P1 — current behavior phones home by default with no opt-in; this is a privacy-posture fix tied to the same customer-onboarding moment as M63_001.
**Categories:** CLI
**Batch:** B1 — independent of M63_001; both ship under one branch.
**Branch:** feat/m63-zombiectl-customer-defaults (to be created at CHORE(open); shared with M63_001)
**Depends on:** None.

**Canonical architecture:** `docs/architecture/high_level.md` — zombiectl is the customer entry point; telemetry policy belongs at this layer.

---

## Implementing agent — read these first

1. `zombiectl/src/lib/analytics.js` — current `resolveConfig` (lines 15-20) and the `enabled = boolFromEnv(env.ZOMBIE_POSTHOG_ENABLED, key.length > 0)` line that makes the bundled key opt-out by default.
2. `zombiectl/src/lib/state.js` — `resolveStatePaths`, `loadCredentials`, `saveCredentials`, `clearCredentials`. The two existing JSON files (`credentials.json`, `workspaces.json`) and their on-disk shape, mode, and parsing fallback. New preference helpers mirror this exact pattern.
3. `zombiectl/src/commands/core.js` — login command flow. The new prompt fires inside the login handler, after token persistence succeeds and before the success message.
4. `zombiectl/src/cli.js` — context construction (lines 75-87). Preferences must be loaded into `ctx` so analytics resolution downstream can read them.
5. `zombiectl/test/login.unit.test.js`, `analytics.unit.test.js`, `state.unit.test.js` — testing conventions for these surfaces (node:test, no fakes for fs beyond the existing `helpers-fs.js`).
6. `zombiectl/src/program/io.js` — how interactive vs `--no-input` vs `--json` is currently signalled into command handlers; the prompt suppression rule must hook through this.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; specifically RULE TST-NAM (no milestone IDs in test names) and RULE FLL (file/function length).
- **`docs/BUN_RULES.md`** — JS module discipline.
- **`docs/LOGGING_STANDARD.md`** — applies to any new stderr/stdout emit on the consent path (one-line warning on corrupt-preferences fallback).

No Zig, schema, HTTP-handler, auth-flow, or UI-component surfaces touched — those rule files do not apply.

---

## Anti-Patterns to Avoid

Read the standard list in `docs/TEMPLATE.md`. Specific to this spec:

- Do NOT extend `credentials.json` with telemetry fields. Consent must survive `zombiectl logout`; coupling it to the auth file would either lose the decision on logout or leak telemetry-state into auth-clear semantics.
- Do NOT auto-write a default `posthog_enabled: false` to disk during `clearCredentials()` or any non-interactive path. Absence of the file means "not yet decided" — preserves the right to prompt on the next interactive run.
- Do NOT prompt outside the login flow. `zombiectl <command>` for already-authenticated users does not get retroactively prompted. The prompt is bound to the `login` command for as long as the file is absent.
- Do NOT rename or repurpose `ZOMBIE_POSTHOG_ENABLED`. It is the existing env override and stays that way.

---

## Overview

**Goal (testable):** A fresh `zombiectl login` on a customer machine prompts once for telemetry consent, persists the decision, never re-prompts on subsequent invocations, and the `ZOMBIE_POSTHOG_ENABLED` env var always wins over the persisted decision.

**Problem:** `analytics.js:18` makes PostHog opt-out by default whenever `DEFAULT_POSTHOG_KEY` is present — and the bundled key is always present in the published package. Customers installing zombiectl phone home to PostHog without ever being asked. There is no in-product mechanism for consent; the only way to opt out is to set an env var the customer has never heard of.

**Solution summary:** Introduce a small preferences file (`~/.config/zombiectl/preferences.json`) that records the consent decision plus a timestamp. Add a one-shot consent prompt to the login flow when the file is absent and the session is interactive. Update `analytics.resolveConfig` so the precedence becomes `ZOMBIE_POSTHOG_ENABLED env > preferences.json > default false`. Non-interactive sessions (`--no-input`, `--json`) skip the prompt and default to off without writing the file, so the next interactive run still prompts.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/lib/state.js` | EDIT | Add `loadPreferences` / `savePreferences` / `preferencesPath` helpers + extend `resolveStatePaths` with `preferencesPath`. Mirror the existing JSON-file pattern (mode 0600, ENOENT-as-default fallback). |
| `zombiectl/src/lib/analytics.js` | EDIT | `resolveConfig` reads preferences alongside env; precedence: env > preferences > default false. |
| `zombiectl/src/commands/core.js` | EDIT | Login flow invokes the consent prompt when preferences are absent and session is interactive; persists the decision. |
| `zombiectl/src/cli.js` | EDIT | Load preferences into `ctx` once at startup; pass through to analytics + commands. |
| `zombiectl/src/program/io.js` | EDIT (if needed) | Surface a small `promptYesNo`-style helper if the project doesn't already have one in a shared module. Reuse first; add only if no equivalent exists. |
| `zombiectl/test/state.unit.test.js` | EDIT | New cases for preferences load/save/missing/corrupt. |
| `zombiectl/test/analytics.unit.test.js` | EDIT | New cases for env > preferences > default precedence. |
| `zombiectl/test/login.unit.test.js` | EDIT | New cases for prompt-on-first-login, no-prompt-on-subsequent-login, no-prompt-under-`--no-input`/`--json`, preserved-across-logout. |
| `zombiectl/README.md` | EDIT | Document the consent prompt and the env-var override; mention `~/.config/zombiectl/preferences.json` and how to flip the decision. |

---

## Sections (implementation slices)

### §1 — Preferences file shape and helpers

Add `preferencesPath` to `resolveStatePaths()` alongside `credentialsPath` and `workspacesPath`. Add `loadPreferences()` that returns the parsed JSON or a sentinel "not decided" shape on ENOENT. Add `savePreferences(next)` writing mode `0600`. Reuse `readJson` / `writeJson` / `ensureBaseDir` — no new fs primitives.

**Implementation default:** the sentinel for "not decided" is the absence of `posthog_enabled` (i.e. the loader returns `{ posthog_enabled: null, decided_at: null, schema_version: 1 }` on ENOENT). Callers test for `null` to distinguish "never asked" from "explicitly false". Corrupt JSON is treated identically to ENOENT — return the sentinel and emit a one-line warn to stderr; do not auto-clobber the file.

### §2 — Login-time consent prompt

Inside the `login` command handler, after the token has been persisted but before the success message, check: (a) preferences file says decided, (b) `ZOMBIE_POSTHOG_ENABLED` env is set (in which case respect the env and skip prompting), (c) session is non-interactive (`--no-input` / `--json` / TTY check). If any is true, do not prompt. Otherwise, prompt: `zombiectl reports anonymous usage metrics to help us improve. Enable? [Y/n]`. Persist the decision via `savePreferences`. Ctrl-C / EOF at the prompt is treated as no-decision: do not write the file, default off this session.

**Implementation default:** `[Y/n]` with empty-input → yes. Rationale: this is presented at a moment of customer goodwill (just successfully logged in) and the data we collect is anonymized (existing analytics client). The prompt text is fixed and tested for exact-string match.

### §3 — Analytics precedence

Update `resolveConfig` in `analytics.js` to accept a `preferences` argument (or read it from `ctx`). The new precedence: if `env.ZOMBIE_POSTHOG_ENABLED` is set, use `boolFromEnv` of that. Else if `preferences.posthog_enabled` is non-null, use that. Else default to `false` (regardless of `key.length > 0`). Note the change from today's `enabled = boolFromEnv(env.ZOMBIE_POSTHOG_ENABLED, key.length > 0)` — the fallback is no longer "enabled because key exists"; it is "disabled because consent absent".

### §4 — Persistence semantics across logout

`clearCredentials()` MUST NOT touch `preferences.json`. The two files have orthogonal lifecycles: credentials are auth state; preferences are user choice. A user who logs out and back in is not asked again. A user who explicitly wants to be re-prompted can `rm ~/.config/zombiectl/preferences.json` (document this in README).

---

## Interfaces

**New file on disk: `~/.config/zombiectl/preferences.json`**

```json
{
  "schema_version": 1,
  "posthog_enabled": true,
  "decided_at": 1730854920000
}
```

- `schema_version` — integer; `1` for this spec. Reserved for future evolution. Loader rejects unknown schema versions by returning the sentinel + warn.
- `posthog_enabled` — boolean; `null` only as the in-memory sentinel for "never decided", never written to disk.
- `decided_at` — ms epoch; `Date.now()` at write time.
- File mode: `0600` (matches `credentials.json`).

**New env var precedence (analytics resolution):**

```
ZOMBIE_POSTHOG_ENABLED env > preferences.posthog_enabled > false
```

Compare against the old chain: `ZOMBIE_POSTHOG_ENABLED env > (key.length > 0 ? true : false)`.

**Prompt contract:**

```
Stdout: "zombiectl reports anonymous usage metrics to help us improve. Enable? [Y/n] "
Stdin:  any of "y", "Y", "yes", "" (empty + Enter) → enabled = true
        any of "n", "N", "no" → enabled = false
Ctrl-C / EOF → no write, session defaults to false, next interactive login re-prompts.
```

**Public-API impact:** none. No CLI flag added or removed. No HTTP surface change.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `preferences.json` ENOENT | First run, or user deleted the file | Loader returns sentinel `{ posthog_enabled: null }`. Login flow prompts (if interactive). |
| `preferences.json` invalid JSON | Manual edit, partial write, FS corruption | Loader returns sentinel + emits one-line warn to stderr (`zombiectl: preferences.json unreadable; treating as not-decided`). File is NOT auto-rewritten — preserves user's broken file for inspection. Next interactive login prompts and overwrites cleanly. |
| `preferences.json` unknown `schema_version` | Older binary reading newer file | Loader returns sentinel + warn. Older binary defaults to off; does not corrupt the newer file. |
| `savePreferences` write fails (EACCES, ENOSPC) | Permissions / disk full | Catch and emit one-line warn to stderr. Login still succeeds. Telemetry stays off this session. Next interactive login retries. |
| User Ctrl-C at prompt | Interrupt | Treated as no-decision. No file write. Default off this session. Next interactive login re-prompts. |
| `--no-input` flag set | Non-interactive automation | Skip prompt entirely. Default off this session. No file write. |
| `--json` flag set | Machine-readable mode | Skip prompt entirely (no stdout chatter mid-JSON). Default off this session. No file write. |
| `ZOMBIE_POSTHOG_ENABLED=false` set with `posthog_enabled: true` in file | Engineer-mode override | Env wins. No prompt. No file write. Telemetry off this session. |

---

## Invariants

1. **Env always wins.** `ZOMBIE_POSTHOG_ENABLED=false` with `preferences.posthog_enabled: true` resolves to disabled. Enforced by a unit test in `analytics.unit.test.js`.
2. **Preferences file is not touched by logout.** `clearCredentials()` leaves `preferences.json` byte-for-byte identical. Enforced by a unit test in `state.unit.test.js`.
3. **No prompt under `--no-input` or `--json`.** Enforced by unit tests that drive `login` with those flags and assert no readline activation and no preferences write.
4. **Default-off when consent absent.** `analytics.resolveConfig({}, { posthog_enabled: null })` returns `enabled: false`. Enforced by a unit test in `analytics.unit.test.js`.
5. **Single source of truth for the bundled key.** No code path reads `DEFAULT_POSTHOG_KEY` from outside `analytics.js`. Enforced by `grep -rn DEFAULT_POSTHOG_KEY zombiectl/src/` returning one definition.
6. **Mode 0600 on preferences file.** Enforced by a unit test that stats the file after write.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `state preferences load returns sentinel on missing file` | `loadPreferences()` on a fresh state dir returns `{ posthog_enabled: null, decided_at: null, schema_version: 1 }` and does not create the file. |
| `state preferences save writes mode 0600` | After `savePreferences({ posthog_enabled: true })`, the file exists with mode `0600` and round-trips through `loadPreferences()`. |
| `state preferences load returns sentinel on corrupt json` | A file containing `{ broken` is treated as missing and a warn is emitted; the file is NOT overwritten. |
| `state clear credentials does not touch preferences` | `savePreferences({ posthog_enabled: false })` then `clearCredentials()` then re-`loadPreferences()` returns the original boolean. |
| `analytics env beats preferences when disabled` | `resolveConfig({ ZOMBIE_POSTHOG_ENABLED: "false" }, { posthog_enabled: true })` resolves to `enabled: false`. |
| `analytics env beats preferences when enabled` | `resolveConfig({ ZOMBIE_POSTHOG_ENABLED: "true" }, { posthog_enabled: false })` resolves to `enabled: true`. |
| `analytics preferences win over default when env absent` | `resolveConfig({}, { posthog_enabled: true })` resolves to `enabled: true` and `resolveConfig({}, { posthog_enabled: false })` resolves to `enabled: false`. |
| `analytics defaults to off when env and preferences absent` | `resolveConfig({}, { posthog_enabled: null })` resolves to `enabled: false` even though `DEFAULT_POSTHOG_KEY.length > 0`. |
| `login prompts for telemetry on first run` | With preferences absent and a TTY-mode session, login prompts with the exact contract string and persists the decision. |
| `login does not prompt when preferences already set` | With `preferences.posthog_enabled: true`, login completes without invoking the prompt machinery. |
| `login does not prompt under no-input` | With `--no-input` and preferences absent, login completes, no prompt fires, no preferences file written. |
| `login does not prompt under json` | With `--json` and preferences absent, login completes, no prompt fires, no preferences file written, no extra stdout outside the JSON envelope. |
| `login does not prompt when env override present` | With `ZOMBIE_POSTHOG_ENABLED=false` and preferences absent, login completes, no prompt fires, no preferences file written. |
| `login consent yes persists true` | Prompt with stdin `y\n` → preferences file contains `posthog_enabled: true` and a `decided_at` timestamp within the test window. |
| `login consent empty persists true` | Prompt with stdin `\n` (empty + Enter) → preferences file contains `posthog_enabled: true`. |
| `login consent no persists false` | Prompt with stdin `n\n` → preferences file contains `posthog_enabled: false`. |
| `login consent ctrl-c persists nothing` | Prompt with EOF (simulated Ctrl-C) → no preferences file written; analytics off this session. |

Test isolation: each test creates a temp `ZOMBIE_STATE_DIR` and tears it down. Mirror the existing pattern in `zombiectl/test/state.unit.test.js`.

---

## Acceptance Criteria

- [ ] `zombiectl/src/lib/state.js` exports `loadPreferences` and `savePreferences` — verify: `grep -nE "export (async )?function (load|save)Preferences" zombiectl/src/lib/state.js`
- [ ] `zombiectl/src/lib/analytics.js` `resolveConfig` accepts a preferences argument and the new precedence is testable — verify: `make -C zombiectl test` includes the precedence cases.
- [ ] `make -C zombiectl test` passes — verify: `make -C zombiectl test`
- [ ] `gitleaks detect` clean — verify: `gitleaks detect`
- [ ] `make lint` clean — verify: `make lint`
- [ ] Every Failure Modes row has a matching test — verify: cross-reference Test Specification against Failure Modes.
- [ ] No file in the diff over 350 lines — verify: standard 350L gate.
- [ ] README documents the consent prompt and the env override — verify: human read of the relevant section, recorded in Verification Evidence.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Preferences helpers exist
grep -nE 'export (async )?function (load|save)Preferences' zombiectl/src/lib/state.js && echo "PASS" || echo "FAIL"

# E2: Tests
make -C zombiectl test

# E3: Default-off invariant — fresh state dir, no env, fresh login flow → analytics disabled
ZOMBIE_STATE_DIR=$(mktemp -d) ZOMBIE_POSTHOG_ENABLED= node -e '
  import("./zombiectl/src/lib/state.js").then(async (s) => {
    const p = await s.loadPreferences();
    console.log(p.posthog_enabled === null ? "PASS" : "FAIL", p);
  });
'

# E4: clearCredentials does not touch preferences
ZOMBIE_STATE_DIR=$(mktemp -d) node -e '
  import("./zombiectl/src/lib/state.js").then(async (s) => {
    await s.savePreferences({ posthog_enabled: true, decided_at: Date.now(), schema_version: 1 });
    await s.clearCredentials();
    const p = await s.loadPreferences();
    console.log(p.posthog_enabled === true ? "PASS" : "FAIL", p);
  });
'

# E5: Lint
make lint 2>&1 | tail -3

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3

# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E8: Single source of truth for bundled posthog key
grep -rn 'DEFAULT_POSTHOG_KEY' zombiectl/src/ | head
echo "E8: one definition + reads from analytics.js only = pass"
```

---

## Dead Code Sweep

N/A — no files deleted, no symbols renamed. New helpers added; old helpers untouched.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this Test Specification. Catches missing negative cases. | Clean run; iteration count in Discovery. |
| After tests pass | `/review` | Adversarial diff review against this spec, BUN_RULES.md, RULES.md, LOGGING_STANDARD.md. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments on the PR diff. | All comments addressed before merge. |
| After every push | `kishore-babysit-prs` | Polls greptile, triages findings. | Stops on two consecutive empty polls. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make -C zombiectl test` | TBD | |
| Lint | `make lint` | TBD | |
| Gitleaks | `gitleaks detect` | TBD | |
| 350L gate | `wc -l` per file in diff | TBD | |
| Preferences helpers exported | `grep -nE` (E1 above) | TBD | |
| Default-off invariant | E3 above | TBD | |
| Logout preserves preferences | E4 above | TBD | |
| Single posthog key source | E8 above | TBD | |
| README updated | manual read | TBD | |

---

## Discovery (consult log)

Empty at creation. Populated as Architecture / Legacy-Design consults fire during EXECUTE.

---

## Out of Scope

- Default API URL flip — covered by M63_001.
- A separate `zombiectl telemetry on/off` command — possible follow-up; for now the recovery path is `rm ~/.config/zombiectl/preferences.json` (documented in README) or `ZOMBIE_POSTHOG_ENABLED` env override.
- Migrating existing customers' implicit "opt-out" state — there is none; the previous default was opt-out without persistence, so every customer hits the prompt on next login.
- Multi-property telemetry consent (e.g., separate "crash reports" vs "usage metrics" toggles) — single boolean for now; expand later if product needs it.
- A consent prompt on `zombiectl doctor` or other commands — strictly login-bound in this spec.
