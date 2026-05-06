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

# M63_001: zombiectl default API URL points to production

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 001
**Date:** May 06, 2026
**Status:** DONE
**Priority:** P1 — customer onboarding is silently broken without this fix.
**Categories:** CLI
**Batch:** B1 — independent of M63_002; both ship together but neither blocks the other.
**Branch:** chore/ui-app-single-lockfile (shared with M63_002 and the lockfile chore commit)
**Depends on:** None.

**Canonical architecture:** `docs/architecture/high_level.md` — zombiectl is the customer/operator entry point to the control plane.

---

## Implementing agent — read these first

1. `zombiectl/src/program/args.js` — current `DEFAULT_API_URL` constant and the precedence chain (`--api` flag > `ZOMBIE_API_URL` env > `API_URL` env > default).
2. `zombiectl/src/cli.js` — same constant is the floor for `creds.api_url` (line 76) and for the unreachable-API error message (line 294). Single source of truth must stay in `args.js`.
3. `zombiectl/test/args.unit.test.js` — existing default-URL assertion (line 60) under the "parseGlobalArgs / no env, no flag" case.
4. `zombiectl/test/workspace-add.test.js` — line 42 uses `http://localhost:3000` literally; verify whether it asserts via the default or by explicit construction; update only if it relies on the constant.
5. `.env.prod.tpl` line 22 — `https://api.usezombie.com` is the production control-plane host of record.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; specifically RULE TST-NAM (no milestone IDs in test names) and RULE ORP (cross-layer orphan sweep) for the constant flip.
- **`docs/BUN_RULES.md`** — applies because diff is JS; const/import discipline.

No Zig, schema, HTTP-handler, or auth-flow surfaces touched — those rule files do not apply.

---

## Anti-Patterns to Avoid

Read the standard list in `docs/TEMPLATE.md`. Specific to this spec:

- Do NOT introduce a `PROD_API_URL` / `DEV_API_URL` / `LOCAL_API_URL` triple. The default is one constant; engineers override via `ZOMBIE_API_URL` (already standard) or `--api`.
- Do NOT add a "first-run greeting" or banner change in this workstream — out of scope.
- Do NOT route the default through env interpolation at install time. The npm-published bundle has a single, baked default.

---

## Overview

**Goal (testable):** A fresh `npm install -g @usezombie/zombiectl` followed by `zombiectl login` (no env vars, no flags) reaches `https://api.usezombie.com` and surfaces the production login flow — no `ECONNREFUSED` against localhost.

**Problem:** `DEFAULT_API_URL` is hardcoded to `http://localhost:3000` (`args.js:1`). Customers installing the published package hit `connection refused` against their own loopback on first `zombiectl login` because nothing on their machine listens on port 3000. The CLI is the only customer-facing onboarding path; this default makes onboarding silently broken.

**Solution summary:** Flip the constant to `https://api.usezombie.com`. Engineers continue to override locally via the `ZOMBIE_API_URL` env (already in their `.zshrc`) or per-invocation `--api`. The override precedence chain is unchanged. One existing test assertion flips with the default.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/program/args.js` | EDIT | Flip `DEFAULT_API_URL` constant to production host. |
| `zombiectl/test/args.unit.test.js` | EDIT | Update the default-URL assertion (currently asserts `http://localhost:3000` for the no-env / no-flag case) to assert the new production value. |
| `zombiectl/test/workspace-add.test.js` | EDIT (only if needed) | Investigate whether the literal `http://localhost:3000` here flows from `DEFAULT_API_URL` or from explicit test setup. Update only if it relies on the default. |
| `zombiectl/README.md` | EDIT | Customer install snippet: confirm `npm i -g @usezombie/zombiectl && zombiectl login` reads as a complete getting-started flow with no env-var prelude. |

---

## Sections (implementation slices)

### §1 — Flip the constant

Change `DEFAULT_API_URL` in `zombiectl/src/program/args.js` from `http://localhost:3000` to `https://api.usezombie.com`. No other code edits in this slice.

**Implementation default:** the bare host with no trailing slash, matching the existing `normalizeApiUrl` contract that strips trailing slashes.

### §2 — Test alignment

Update the existing assertion in `args.unit.test.js` (the "no flag, no env" path). Audit other tests for any leaked dependence on the old default; convert to explicit URL setup where they meant to test localhost specifically (e.g., a test simulating a local-dev environment must stop relying on the default to mean localhost).

### §3 — README sanity check

Read the customer install section of `zombiectl/README.md`. If it currently says "set `ZOMBIE_API_URL` then login" or similar prelude, simplify it to the bare two-step flow. If it already reads as bare two-step, no edit needed — record that in the spec's Discovery section.

---

## Interfaces

No public interface changes. The CLI flag `--api` and the env var `ZOMBIE_API_URL` continue to override the default. The persistence shape of `~/.config/zombiectl/credentials.json` is unchanged.

The constant value is the only behavioral contract being changed:

```
DEFAULT_API_URL: "http://localhost:3000"  →  "https://api.usezombie.com"
```

The precedence chain stays:

```
--api flag > ZOMBIE_API_URL env > API_URL env > creds.api_url > DEFAULT_API_URL
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| DNS fails for `api.usezombie.com` | Customer offline / DNS misconfig | Existing http-client error path surfaces "cannot reach usezombie API" with the resolved URL printed — already implemented at `cli.js:294`. |
| Customer runs against staging/dev | Customer expectation mismatch | `--api https://api-dev.usezombie.com` or `ZOMBIE_API_URL=...` overrides; documented in README. |
| Engineer forgets `ZOMBIE_API_URL` in shell | Shell rc not sourced | Engineer hits prod by mistake. Mitigation: zombiectl prints the resolved API URL on every invocation that touches the network (existing behavior in error path; verify it's also printed on success). |
| Pre-existing test file asserts old default literally | Stale localhost literal | Caught by §2 audit + `make test` failure. |

---

## Invariants

1. **Flag wins over env** — `--api X ZOMBIE_API_URL=Y` resolves to `X`. Enforced by existing test in `args.unit.test.js`.
2. **Env wins over default** — `ZOMBIE_API_URL=Y` with no flag resolves to `Y`. Enforced by existing test.
3. **Persisted creds win over default** — `creds.api_url=Z` with no flag/env resolves to `Z`. Enforced by existing test.
4. **Default is exactly one constant** — `grep -rn "DEFAULT_API_URL" zombiectl/src/` returns one definition + reads. Enforced by code review and orphan sweep.
5. **Default is the production host** — new invariant added by this spec; enforced by the updated `args.unit.test.js` assertion.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `parseGlobalArgs default api url is production` | With empty argv and empty env, `derived.apiUrl === "https://api.usezombie.com"`. Replaces the existing localhost assertion at `args.unit.test.js:60`. |
| `ZOMBIE_API_URL env beats default` | Existing test stands; re-verify it does not pin the default value implicitly. |
| `--api flag beats env` | Existing test stands; same re-verification. |
| `normalizeApiUrl strips trailing slash on production host` | New: `normalizeApiUrl("https://api.usezombie.com/") === "https://api.usezombie.com"`. Mirrors the existing localhost round-trip case. |

No new fixtures needed.

---

## Acceptance Criteria

- [x] `zombiectl/src/program/args.js` line 1 reads `const DEFAULT_API_URL = "https://api.usezombie.com";` — verify: `grep -n "^const DEFAULT_API_URL" zombiectl/src/program/args.js`
- [x] `make test-unit-zombiectl` passes — verify: `make test-unit-zombiectl`
- [x] No file in `zombiectl/src/` other than `args.js` defines `DEFAULT_API_URL` — verify: `grep -rn "^const DEFAULT_API_URL\|^export const DEFAULT_API_URL" zombiectl/src/`
- [x] Customer install snippet in `zombiectl/README.md` is `npm install -g @usezombie/zombiectl && zombiectl login` with no env-var prelude — verify: README §Install + §Quick start show the bare two-step flow.
- [x] `gitleaks detect` clean — verify: `gitleaks detect`
- [x] No file in diff over 350 lines — verify: standard 350L gate command from `docs/gates/file-length.md`.
- [x] `make lint` clean — verify: `make lint`

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Default URL is production
grep -n '^const DEFAULT_API_URL' zombiectl/src/program/args.js && echo "PASS" || echo "FAIL"

# E2: Tests
make test-unit-zombiectl

# E3: No leaked localhost literals as the default
grep -rn 'DEFAULT_API_URL.*localhost' zombiectl/src/ | grep -v test/ | head
echo "E3: empty above = pass"

# E4: Lint
make lint 2>&1 | tail -3

# E5: Gitleaks
gitleaks detect 2>&1 | tail -3

# E6: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'
```

---

## Dead Code Sweep

N/A — no files deleted, no symbols renamed. Pure constant flip.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this Test Specification. | Clean run; iteration count in Discovery. |
| After tests pass | `/review` | Adversarial diff review against this spec, BUN_RULES.md, RULES.md. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments on the PR diff. | All comments addressed before merge. |
| After every push | `kishore-babysit-prs` | Polls greptile, triages findings. | Stops on two consecutive empty polls. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `cd zombiectl && bun test` | 320 pass / 0 fail across 31 files | ✅ |
| Default URL grep | `grep '^const DEFAULT_API_URL' zombiectl/src/program/args.js` | one hit, value `https://api.usezombie.com` | ✅ |
| Single source of truth | `grep -rn "DEFAULT_API_URL" zombiectl/src/` | one definition (args.js); read sites in args.js + cli.js | ✅ |
| README customer-install snippet | manual read of `zombiectl/README.md` | §Install: bare `npm install -g @usezombie/zombiectl@next`; §Quick start: `zombiectl login` is the first command | ✅ |
| Gitleaks | pre-commit hook on every commit on this branch | scanned, no leaks | ✅ |

---

## Discovery (consult log)

- **Branch consolidation** — Captain elected to land M63_001 + M63_002 in the existing `chore/ui-app-single-lockfile` branch instead of opening a fresh `feat/m63-zombiectl-customer-defaults` worktree. The lockfile chore (commit `60f02ebe`) and the M63 work ship under one PR. PR description carries the scope blend explicitly.
- **No leaked dependencies on the old default** — `workspace-add.test.js:42` was the only non-`args.js` test asserting `http://localhost:3000` literally; it was the API origin in a fetch-URL assertion (no preceding `--api` / `ZOMBIE_API_URL`), so it relied on the default and was updated to the new production host.
- **README already clean** — `zombiectl/README.md` §Install and §Quick start needed no edits for M63_001; the install snippet was already `npm install -g @usezombie/zombiectl@next` followed by `zombiectl login` with no env-var prelude.

---

## Out of Scope

- Telemetry / PostHog consent — covered by M63_002.
- Engineer-mode wrapper (Makefile target / shell script) — Captain solved this via `.zshrc` env exports per session decision; no change needed in this milestone.
- Backend (`.env`, zombied) and dashboard (`ui/packages/app`) defaults — separate concerns.
- Multi-environment profile system (`--profile dev` / `--profile prod`) — possible v2 follow-up; not in this milestone.
