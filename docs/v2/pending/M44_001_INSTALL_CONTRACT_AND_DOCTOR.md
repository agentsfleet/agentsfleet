# M44_001: Install Contract Alignment + Doctor Extension + AUTH_EXEMPT Fix

**Prototype:** v2.0.0
**Milestone:** M44
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking. Three load-bearing bugs in shipped code that block the wedge end-to-end. Codex outside-voice review surfaced these as P0 contract mismatches.
**Categories:** API, CLI
**Batch:** B1 — parallel with M40, M41, M42, M43, M45.
**Branch:** feat/m44-install-contract (to be created)
**Depends on:** none — these are bug fixes + a deterministic preflight on existing surfaces.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.2 (Installing the zombie), §12 step 5 (innerCreateZombie atomic publish — depends on M40 too but the contract fix is independent).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/zombie.js:83` — current install POST shape
2. `src/http/handlers/zombies/api.zig:32` — current API expected shape (the mismatch site)
3. `src/zombie/config_parser.zig:108` — `tools:` vs `skills:` key disagreement
4. `zombiectl/src/cli.js:36` — `AUTH_EXEMPT_ROUTES` list (the security hole)
5. `zombiectl/src/program/routes.js:6` — existing `doctor` command (extend, don't replace)

---

## Overview

**Goal (testable):** `zombiectl install --from samples/platform-ops/` succeeds end-to-end against a fresh workspace. The install POST body matches what the API expects. The parser accepts the shipped sample's `tools:` key without modification. `zombie.install` is no longer in `AUTH_EXEMPT_ROUTES` — install requires a valid local token. `zombiectl doctor` returns a structured pass/fail report covering: (1) `auth_token_present`, (2) `server_reachable`, (3) `workspace_selected`, (4) `workspace_binding_valid`. The install-skill (M49) calls `zombiectl doctor --json` first; on any fail, it surfaces the doctor output and aborts.

**Problem (three concrete bugs Codex caught):**

1. **Install API contract mismatch.** `zombiectl install` POSTs `{source_markdown, trigger_markdown}`. API expects `{name, config_json, source_markdown}`. Result: API returns 400; user sees "bad request" with no clear path forward.
2. **Parser key mismatch.** Shipped `samples/platform-ops/TRIGGER.md` declares `tools: [...]`. The parser at `src/zombie/config_parser.zig:108` requires the key `skills:`. Result: even if the contract were fixed, the parser rejects the sample.
3. **Auth-exempt install.** `zombiectl/src/cli.js:36` lists `zombie.install` in `AUTH_EXEMPT_ROUTES`. Result: the CLI doesn't enforce local auth before posting; user gets confusing 401 from the server with no preflight to surface "you're not signed in." Security: install creates a tenant-bound `core.zombies` row and must require auth.

**Solution summary:** Three small, scoped fixes + one CLI extension.

- **Fix 1 (install contract)**: align the CLI POST shape with the API. Make the CLI derive `name` from the source frontmatter (`name:` field) and synthesize `config_json` from the parsed TRIGGER (or merged frontmatter once M46 lands). API accepts both new shape (`{name, config_json, source_markdown}`) and falls back gracefully if `name` is missing (derive on server-side). Net diff small; no backward compat needed since today's contract is broken anyway.
- **Fix 2 (parser)**: rename parser key from `skills` to `tools` in `src/zombie/config_parser.zig:108`. Every shipped sample uses `tools:`. Single-line rename with parser test coverage.
- **Fix 3 (auth)**: remove `"zombie.install"` from the `AUTH_EXEMPT_ROUTES` set. Install now requires valid local credentials per the auth-guard at `cli.js:93`.
- **Doctor extension**: extend the existing `zombiectl doctor` to verify the four conditions above. JSON output mode for skill consumption.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie.js` | EDIT | Fix install POST shape: derive `name` from frontmatter, synthesize `config_json`, send canonical body |
| `src/http/handlers/zombies/api.zig` | EDIT | Accept the canonical body shape; tighten validation |
| `src/zombie/config_parser.zig` | EDIT | Line 108: rename parser key `skills` → `tools` |
| `zombiectl/src/cli.js` | EDIT | Remove `"zombie.install"` from `AUTH_EXEMPT_ROUTES` set |
| `zombiectl/src/commands/doctor.js` | EXTEND | Add 4 deterministic checks; `--json` flag |
| `zombiectl/test/install.unit.test.js` | EXTEND | Cover the new contract shape |
| `zombiectl/test/doctor.unit.test.js` | EXTEND | Cover each of the 4 checks pass + fail variants |
| `tests/integration/install_contract_test.zig` | NEW | E2E: install a real sample → success; install with broken sample → clear error |
| `samples/platform-ops/TRIGGER.md` | NO EDIT | Already uses `tools:` — that's the spec, parser conforms to sample |
| `zombiectl/test/test_install_payload.json` | NEW | Stable payload fixture used by both unit and integration tests |

---

## Sections (implementation slices)

### §1 — Install POST contract alignment

**CLI side** (`zombiectl/src/commands/zombie.js`):

```
1. Read SKILL.md from --from <path>; parse frontmatter
2. Read TRIGGER.md from same dir (or look for merged frontmatter post-M46)
3. Derive `name` from SKILL.md frontmatter `name` field (required, error if missing)
4. Synthesize `config_json` from TRIGGER fields (tools, credentials, network, budget, context, trigger.type)
5. POST { name, config_json, source_markdown: <SKILL.md raw> }
```

**API side** (`src/http/handlers/zombies/api.zig`):

```
1. Accept { name, config_json, source_markdown } as the canonical shape
2. Validate: name (1-64 chars, /^[a-z0-9-]+$/), source_markdown non-empty, config_json valid JSON
3. INSERT core.zombies(...) → returns id, status=active
4. M40's atomic publish handles XGROUP CREATE + XADD zombie:control
5. Return 201 { id, name, ... }
```

**Implementation default**: if the CLI-derived `name` is unset or invalid, return 400 with `{"error": "name_required", "hint": "Add `name: <kebab-case>` to your SKILL.md frontmatter"}`. Make the failure self-documenting.

### §2 — Parser key rename

`src/zombie/config_parser.zig:108`: change the keyword from `skills` to `tools`. Update parser tests. Verify shipped `samples/platform-ops/TRIGGER.md` parses cleanly post-rename.

**Implementation default**: do NOT support both keys (no `tools` OR `skills`). One canonical key. If a legacy spec uses `skills:`, it errors with `unknown_key: skills (did you mean tools:?)`.

### §3 — Remove auth-exempt install

`zombiectl/src/cli.js:36`:

```diff
-const AUTH_EXEMPT_ROUTES = new Set(["login", "doctor", "zombie.install"]);
+const AUTH_EXEMPT_ROUTES = new Set(["login", "doctor"]);
```

The auth guard at `cli.js:93` will now check for local credentials before any install call. Test: run `zombiectl install --from <path>` with no token → should fail with `not_authenticated` from the CLI before any HTTP call.

### §4 — Doctor extension: 4 checks

`zombiectl/src/commands/doctor.js` (extend existing):

Each check returns `{check: string, ok: boolean, detail: string}`. Aggregate result is `{ok: boolean, checks: [...]}`.

| Check | What it verifies | Pass condition |
|---|---|---|
| `auth_token_present` | Local credentials file exists with a non-empty token | File present, token decodable |
| `server_reachable` | `GET /healthz` against the configured `ZOMBIED_URL` | 200 within 5s timeout |
| `workspace_selected` | Local config has a selected workspace_id | Non-empty workspace_id in state |
| `workspace_binding_valid` | The token is bound to the selected workspace (server confirms) | `GET /v1/workspaces/{ws}` returns 200 |

`--json` flag emits structured JSON to stdout (consumed by M49 install-skill). Default human-readable output: green `✓` per check, red `✗` per failure with `detail`.

**Implementation default**: timeout each check at 5s individually; total `doctor` runtime cap at 20s. If any check fails, exit 1; if all pass, exit 0.

### §5 — Cleanup chore (optional in this milestone)

If trivial: namespace `login`/`logout` under `zombiectl auth login` / `zombiectl auth logout`. Keep `login`/`logout` as deprecated aliases that print a one-line warning and forward. Defer if it expands diff materially.

**Implementation default**: defer the rename to a follow-up if it touches >5 files. The auth subcommand namespace is a small UX polish, not load-bearing for the wedge.

---

## Interfaces

```
CLI:
  zombiectl install --from <path>
    POST /v1/workspaces/{ws}/zombies
      body: { name, config_json, source_markdown }
      → 201 { id, name, config_revision }

  zombiectl doctor [--json]
    runs 4 checks, exit 0/1
    --json: stdout = { ok: bool, checks: [...] }

  zombiectl auth login   (optional, this milestone or next)
  zombiectl auth logout  (optional)

CLI auth guard:
  - AUTH_EXEMPT_ROUTES = { "login", "doctor" }
  - All other routes require valid local token before HTTP call

Parser:
  TRIGGER.md / merged frontmatter key:
    tools: [string]    ← canonical (not "skills")
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `zombiectl install` without local auth | User skipped `auth login` | Fail BEFORE HTTP call: "Run `zombiectl auth login` first." |
| Install POST missing `name` | Frontmatter has no `name:` | 400 `{error: "name_required", hint: "..."}` |
| Parser hits legacy `skills:` key | Old sample on disk | Error: `unknown_key: skills (did you mean tools:?)` |
| Doctor `server_reachable` fails | ZOMBIED_URL wrong or service down | Exit 1; detail prints the URL tried + the error |
| Doctor `workspace_binding_valid` fails | Token revoked or workspace deleted | Exit 1; detail says "run `zombiectl workspace list` to reset" |

---

## Invariants

1. **No more auth-exempt install.** Every install attempt must have local credentials; if missing, fail before HTTP.
2. **One canonical install POST shape.** `{name, config_json, source_markdown}`. No backward-compat for the old shape because nobody has shipped against it.
3. **One canonical parser key.** `tools:`. The shipped sample is the spec; the parser conforms.
4. **Doctor is idempotent and read-only.** Running it 100 times in a row makes no state changes; safe to call from skills repeatedly.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_install_post_canonical_shape` | CLI sends `{name, config_json, source_markdown}` matching API contract |
| `test_install_missing_name_400` | Sample without `name:` in frontmatter → 400 with hint |
| `test_install_no_local_auth_fails_before_http` | No token + `zombiectl install` → fails before any HTTP, exits 1 |
| `test_parser_tools_key_accepted` | Shipped `samples/platform-ops/TRIGGER.md` parses cleanly |
| `test_parser_legacy_skills_key_rejected` | Synthetic spec with `skills:` → `unknown_key` error |
| `test_doctor_all_pass` | Valid token + reachable server + selected ws + valid binding → exit 0 |
| `test_doctor_no_token_fail` | Missing token file → `auth_token_present: false`, exit 1 |
| `test_doctor_unreachable_fail` | ZOMBIED_URL pointed at unreachable host → `server_reachable: false`, exit 1 |
| `test_doctor_json_output` | `--json` emits valid JSON matching schema |
| `test_e2e_install_then_steer` | doctor → install → steer round-trip succeeds against test server |

---

## Acceptance Criteria

- [ ] `make test` passes new unit tests
- [ ] `make test-integration` passes the install + doctor E2E tests
- [ ] Manual smoke: fresh laptop with no zombiectl state → `zombiectl auth login` → `zombiectl install --from samples/platform-ops/` → success
- [ ] Manual smoke: same flow without `auth login` → install fails with clear "run auth login" message before any HTTP
- [ ] Codex P0 findings 1, 2, 3 (install contract, parser, auth-exempt) all resolved verifiably
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
- [ ] `make check-pg-drain` clean (no schema changes here, but baseline)
