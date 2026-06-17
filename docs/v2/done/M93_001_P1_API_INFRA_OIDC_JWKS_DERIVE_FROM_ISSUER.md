# M93_001: Derive OIDC JWKS URL from issuer (kill the issuer/jwks-url drift bug class)

**Prototype:** v2.0.0
**Milestone:** M93
**Workstream:** 001
**Date:** Jun 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a drifted JWKS URL is a silent, total auth outage; the fix removes the field that can drift.
**Categories:** API, INFRA
**Batch:** B1 — standalone hardening, no concurrent dependants.
**Branch:** feat/m93-oidc-jwks-derive
**Test Baseline:** unit=1958 integration=190 (`make _lint_zig_test_depth`, CHORE(open) Jun 17 2026)
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-4-8, Jun 17 2026) — from a live prod-config incident investigation.

> **Provenance is load-bearing.** LLM-drafted — cross-check every claim against `src/agentsfleetd/auth/` and `config/` before EXECUTE; do not trust the prose over the code.

**Canonical architecture:** `docs/AUTH.md` §"The three flows at a glance" + the OIDC env-var table (`OIDC_JWKS_URL` / `OIDC_ISSUER` rows) — the source of truth for the verifier's claim checks and key-fetch caching.

---

## Implementing agent — read these first

1. `docs/AUTH.md` — the OIDC env-var table (`OIDC_JWKS_URL` = "where to fetch Clerk's signing keys, cached 6h, refresh on kid miss"; `OIDC_ISSUER` = "required value of `iss`"). Read the Rotation procedure too — this change must not alter rotation semantics.
2. `src/agentsfleetd/config/runtime_loader.zig` `loadOidc` — current loader: `enabled` is gated on `OIDC_JWKS_URL` being non-empty; issuer/audience loaded alongside. This gate and the `MissingOidcJwksUrl` error move to issuer.
3. `src/agentsfleetd/auth/oidc.zig` + `auth/jwks.zig` — `Config { jwks_url, issuer, audience }` and the `Verifier`. The derivation lands at the boundary where `jwks_url` is resolved before `Verifier.init`.
4. `src/agentsfleetd/config/runtime_validate.zig` + `runtime_types.zig` — the `ValidationError` set + fatal-message mapping (`OidcRequired`, `MissingOidcJwksUrl`).
5. `src/agentsfleetd/cmd/doctor.zig` — the `oidc_jwks_reachability` check; must derive the URL the same way the runtime does, from one helper.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** derive OIDC JWKS URL from issuer; make OIDC_JWKS_URL optional
- **Intent (one sentence):** Operators configure exactly one Clerk identity value (`OIDC_ISSUER`); the daemon derives the JWKS endpoint from it, so issuer and key-source can never drift into a silent auth outage.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; reconcile any mismatch before editing.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an operator sets only `OIDC_ISSUER=https://clerk.agentsfleet.net` (no `OIDC_JWKS_URL`), deploys, and tokens verify on first request because the daemon fetched keys from `https://clerk.agentsfleet.net/.well-known/jwks.json` it derived itself.
2. **Preserved user behaviour** — an explicit `OIDC_JWKS_URL` still wins (override path); the `custom` provider with a non-standard JWKS path keeps working unchanged. Existing deployments that set both vars behave identically.
3. **Optimal-way check** — the unconstrained-optimal is full OIDC discovery (fetch `<issuer>/.well-known/openid-configuration`, read `jwks_uri`). Gap: that adds a startup network hop + a new failure mode for a value Clerk fixes at `/.well-known/jwks.json`. String derivation is deterministic and offline; the gap (won't follow a non-conventional `jwks_uri`) is covered by the explicit-override escape hatch.
4. **Rebuild-vs-iterate** — iterate. This is a config-loading refinement, not an auth-model change; the verifier internals are untouched. A refactor to an agentsfleet-native issuer is the v3 trajectory (`docs/AUTH.md` "Beyond Stage 2"), out of scope.
5. **What we build** — a single derivation helper, a moved enable-gate/validation (jwks_url → issuer), doctor parity, and deploy-workflow + vault cleanup that stops shipping the derivable value.
6. **What we do NOT build** — full OIDC discovery; multi-issuer support; any change to `aud`/`iss`/`exp` checking.
7. **Fit with existing features** — compounds with the Clerk OIDC verify path; must not destabilize the `agt_t`/`agt_r` non-JWKS middleware branches (they never touch this code).
8. **Surface order** — API-first (the daemon); INFRA (workflows/vault) follows in the same workstream because the contract change spans both.
9. **Dashboard restraint** — N/A (no UI surface).
10. **Confused-user next step** — a misconfigured issuer surfaces as `agentsfleet doctor`'s `oidc_jwks_reachability` check failing with the derived URL printed, plus the existing fatal message naming the missing var.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; specifically **NDC** (no dead code — the removed `MissingOidcJwksUrl`-on-jwks path must be fully excised, not left dangling), **NLR** (touch-it-fix-it on `loadOidc`), **UFS** (the `/.well-known/jwks.json` suffix and any provider strings become named constants, shared by runtime + doctor verbatim).
- **`dispatch/write_zig.md`** — diff is `*.zig`: tagged-union/error-set results, multi-step `errdefer` on the new allocation in the derivation helper, file ≤350 / fn ≤50, cross-compile both linux targets.
- **`docs/AUTH.md`** — auth-flow doc; re-read before EXECUTE (auth trigger).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile `x86_64-linux` + `aarch64-linux`; pg-drain N/A (no DB); errdefer on derived-string alloc. |
| PUB / Struct-Shape | yes | new derivation helper is a small pub fn or private; `OidcConfig` shape changes (jwks_url stays optional) — shape verdict at PLAN. |
| File & Function Length | yes | `loadOidc` must stay ≤50 lines after the gate move; extract the derivation helper rather than inline. |
| UFS | yes | `"/.well-known/jwks.json"`, `"clerk"`, env-var names → named constants; doctor imports the same helper, no second literal. |
| LOGGING | yes | log the derived JWKS URL at startup (info) so a misconfig is greppable; never log secrets (URLs are public). |
| ERROR REGISTRY | no | reuses existing fatal validation messages; no new `UZ-` code. |
| UI / DESIGN TOKEN / SCHEMA | no | N/A — no UI, no schema. |

---

## Overview

**Goal (testable):** `loadOidc` with `OIDC_ISSUER` set and `OIDC_JWKS_URL` unset returns `enabled=true` with `jwks_url = "<issuer>/.well-known/jwks.json"`; with both set, the explicit `OIDC_JWKS_URL` is returned verbatim.

**Problem:** Operators configure two independent Clerk values — issuer and JWKS URL — that must agree. In production they drifted (`issuer=https://clerk.agentsfleet.net`, `jwks-url=https://clerk.usezombie.com/...`, a dead domain). The daemon would have fetched keys from a dead host and rejected every token: a silent, total auth outage with no compile-time or deploy-time guard.

**Solution summary:** Make `OIDC_ISSUER` the single source of identity truth. The config loader derives the JWKS URL as `<issuer>/.well-known/jwks.json` unless `OIDC_JWKS_URL` is explicitly set (override, retained for non-standard providers). The OIDC enable-gate and the required-field validation move from `OIDC_JWKS_URL` to `OIDC_ISSUER`. Deploy workflows stop staging the derivable secret, and the redundant vault `jwks-url` fields are removed.

---

## Prior-Art / Reference Implementations

- **API** → mirror the existing `loadOidc` / `freeOidc` ownership pattern in `runtime_loader.zig` (owned-string load, `errdefer` free chain) and the `oidc.Config` → `jwks.Verifier.init` wiring in `auth/oidc.zig`. No new abstraction; the derivation is one helper feeding the existing `Config.jwks_url`.
- Not greenfield. No external library — string derivation only.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/config/runtime_loader.zig` | EDIT | derive jwks_url from issuer when override absent; move enable-gate to issuer. |
| `src/agentsfleetd/config/runtime_validate.zig` | EDIT | require issuer (not jwks_url) when OIDC requested; remap fatal message. |
| `src/agentsfleetd/config/runtime_types.zig` | EDIT | rename/replace `MissingOidcJwksUrl` → `MissingOidcIssuer` (or add) in the error set. |
| ~~`src/agentsfleetd/config/env_vars.zig`~~ | ~~EDIT~~ → **NOT TOUCHED** | CHORE(open) correction (provenance cross-check): this file is DB/Redis URL validation only — it has zero OIDC content. The well-known suffix constant + derivation helper live in `auth/oidc.zig` (row below). The `OIDC_JWKS_URL`-is-optional documentation lands in `docs/AUTH.md` (new row). |
| `src/agentsfleetd/auth/oidc.zig` | EDIT | host the `WELL_KNOWN_JWKS_SUFFIX` named constant + the derivation/resolution helper (shared by runtime loader + doctor); no verifier-logic change. |
| `src/agentsfleetd/cmd/doctor.zig` | EDIT | read `OIDC_ISSUER`, resolve via the shared helper so the doctor check matches runtime. |
| `src/agentsfleetd/cmd/serve.zig` | EDIT (logging-only) + orphan fix | wiring unchanged (already reads `oidc_jwks_url orelse ""`); adds the resolved `jwks_url` to the `startup.oidc_init_start` info log so a misconfig is greppable at boot (LOGGING gate); plus the `MissingOidcJwksUrl`→`MissingOidcIssuer` rename in its exhaustive `ValidationError` switch. |
| `docs/AUTH.md` | EDIT | document `OIDC_JWKS_URL` as an optional override (default derived from `OIDC_ISSUER`). |
| `.github/workflows/deploy-dev.yml` | EDIT | drop `OIDC_JWKS_URL` from the 1Password load + `flyctl secrets set`. |
| `.github/workflows/release.yml` | EDIT | drop `OIDC_JWKS_URL` from the prod load + `flyctl secrets set`. |
| `src/agentsfleetd/config/runtime_loader_test.zig` + OIDC/JWKS tests | EDIT | cover derive / override / missing-issuer. |
| 1Password `clerk-dev` + `clerk-prod` `jwks-url` field | DELETE (ops) | redundant once derived — removed after the deploy workflows stop reading it. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section for the daemon derivation+validation, one for the doctor parity, one for the INFRA (workflow + vault) cleanup. The daemon change must land before the workflows stop supplying `OIDC_JWKS_URL`, else a deploy between the two would have neither source.
- **Alternatives considered:** (a) full OIDC discovery — rejected (startup network hop + failure mode for a Clerk-fixed value; revisit only if a non-conventional IdP appears). (b) Remove `OIDC_JWKS_URL` entirely — rejected (drops the `custom`-provider escape hatch; keep as override).
- **Patch-vs-refactor verdict:** **patch** — it removes a config field and relocates a gate; the verifier and auth model are untouched.

---

## Sections (implementation slices)

### §1 — Derive JWKS URL from issuer in the config loader ✅ DONE

`loadOidc` resolves the effective JWKS URL: explicit `OIDC_JWKS_URL` (trimmed, non-empty) wins; otherwise derive `<issuer>/.well-known/jwks.json` from a trailing-slash-normalised issuer. The OIDC enable-gate becomes "issuer is non-empty". **Implementation default:** strip surrounding whitespace and **every** trailing `/` from the issuer (`std.mem.trimEnd`) before appending the well-known suffix, because Clerk issuers are emitted without a trailing slash and a double-slash path 404s. (Hardened during `/review` from the draft's "exactly one slash" — the goal is *no* `//`, so trim the whole class, not just one.)

- **Dimension 1.1** — issuer set, no override → derived url = `<issuer>/.well-known/jwks.json` → Test `test_oidc_derives_jwks_from_issuer`
- **Dimension 1.2** — explicit `OIDC_JWKS_URL` set → returned verbatim, no derivation → Test `test_oidc_explicit_jwks_overrides_derivation`
- **Dimension 1.3** — issuer has a trailing slash → derived url has no double slash → Test `test_oidc_issuer_trailing_slash_normalised`
- **Dimension 1.4** — enable-gate fires on issuer, not jwks_url → Test `test_oidc_enabled_when_issuer_present_only`

### §2 — Validation + doctor parity ✅ DONE

Required-field validation moves to issuer; the fatal message names `OIDC_ISSUER`. `agentsfleet doctor` derives the reachability-check URL through the **same** helper as the runtime, so the doctor never tests a different URL than the daemon will fetch.

- **Dimension 2.1** — OIDC requested (any OIDC var set) but issuer empty → `MissingOidcIssuer` fatal → Test `test_oidc_missing_issuer_rejected`
- **Dimension 2.2** — doctor's derived JWKS URL equals the loader's derived URL for the same issuer → Test `test_doctor_jwks_url_matches_runtime`

### §3 — INFRA: stop shipping the derivable secret ✅ DONE

Remove `OIDC_JWKS_URL` from both deploy workflows (1Password load block + `flyctl secrets set`), keeping `OIDC_ISSUER`. After deploys confirm green, remove the `jwks-url` field from the `clerk-dev` and `clerk-prod` vault items (ops step recorded in Discovery).

- **Dimension 3.1** — neither workflow references `OIDC_JWKS_URL` → Test `test_workflows_no_jwks_url` (grep assertion in CI hygiene / acceptance script)
- **Dimension 3.2** — both workflows still set `OIDC_ISSUER` + `OIDC_AUDIENCE` → Test `test_workflows_retain_issuer_audience`

---

## Interfaces

```
# Effective config resolution (runtime_loader.loadOidc):
OIDC_ISSUER        (required when OIDC enabled)  e.g. https://clerk.agentsfleet.net
OIDC_JWKS_URL      (OPTIONAL override)           default: <issuer>/.well-known/jwks.json
OIDC_AUDIENCE      (unchanged, required)         e.g. https://api.agentsfleet.net
OIDC_PROVIDER      (unchanged)                   default: clerk

# Derivation helper (signature is the agent's call; contract):
#   derive_jwks_url(alloc, issuer) -> owned []u8  == trimEnd(issuer,'/') ++ "/.well-known/jwks.json"
# OidcConfig.enabled == (trim(issuer).len > 0)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + observable) |
|------|-------|------------------------------------------|
| Missing issuer | OIDC vars present, `OIDC_ISSUER` empty | fatal at startup: `OIDC is required — set OIDC_ISSUER…`; process exits non-zero. |
| Issuer trailing slash(es) | operator sets `https://x/` or `https://x///` | all trailing slashes trimmed; derived URL has no `//`. |
| Stale explicit override | operator sets a wrong `OIDC_JWKS_URL` | override is honoured (escape hatch); `doctor` reachability check fails loudly with the URL printed. |
| Derived host unreachable | issuer host down at boot | existing JWKS-fetch failure path (cached-stale-serve / `JwksFetchFailed`) — unchanged. |
| OIDC disabled | no OIDC vars at all | `enabled=false`; unchanged (local/dev no-auth path). |

---

## Invariants

1. The runtime and `doctor` derive the JWKS URL from a **single** shared helper — enforced by both call sites importing the same fn + `test_doctor_jwks_url_matches_runtime` (no second literal; UFS gate backs this).
2. An explicit `OIDC_JWKS_URL` always takes precedence over derivation — enforced by `test_oidc_explicit_jwks_overrides_derivation`.
3. OIDC `enabled` ⟺ issuer non-empty — enforced by `test_oidc_enabled_when_issuer_present_only` + the validation in §2.
4. The well-known suffix exists exactly once in the codebase as a named constant — enforced by UFS gate.

---

## Test Specification (tiered)

Test names are the **actual prose names** shipped (RULE TST-NAM — milestone-free), reconciled from the spec's draft snake_case identifiers. All in `runtime_loader_test.zig`.

| Dimension | Tier | Test (actual name) | Asserts (concrete inputs → expected output) | ✅ |
|-----------|------|------|---------------------------------------------|----|
| 1.1 | unit | `"loadOidc derives the JWKS URL from issuer when no override is set"` | issuer `https://clerk.agentsfleet.net`, no override → `…/.well-known/jwks.json` | ✅ |
| 1.2 | unit | `"loadOidc returns an explicit OIDC_JWKS_URL verbatim, overriding derivation"` | both set → returns the explicit URL verbatim | ✅ |
| 1.3 | unit | `"loadOidc normalises an issuer trailing slash with no double slash"` | `https://x/` → `https://x/.well-known/jwks.json` (no `//`) | ✅ |
| 1.4 | unit | `"loadOidc is enabled when only OIDC_ISSUER is present"` | only issuer set → `enabled=true` | ✅ |
| 2.1 | unit | `"loadOidc rejects an OIDC slate that sets audience but no issuer"` | audience set, issuer empty → `MissingOidcIssuer` | ✅ |
| 2.2 | unit | `"doctor and loader resolve the same JWKS URL from one issuer"` | same issuer → loader URL == `oidc.resolveJwksUrl` (the fn doctor calls) | ✅ |
| 3.1 | acceptance grep | `! grep -rn OIDC_JWKS_URL .github/workflows/` | both workflows → zero `OIDC_JWKS_URL` references | ✅ |
| 3.2 | acceptance grep | `grep -c "OIDC_ISSUER\|OIDC_AUDIENCE" …` | both workflows still set `OIDC_ISSUER` + `OIDC_AUDIENCE` (3 each) | ✅ |

**Bonus coverage added:** `"… accepts custom provider"` now asserts the explicit override is retained, and `"… treats an empty OIDC_JWKS_URL as absent and derives from issuer"` pins the empty-override edge. **Five pre-existing OIDC tests** were updated to set `OIDC_ISSUER` (the enable-gate moved jwks_url → issuer).

**Regression:** the existing `oidc.zig`/`jwks_test.zig` happy-path + reject tests must still pass unchanged (verifier logic untouched). **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [x] Derive-from-issuer + override + missing-issuer all covered — `make test-unit-agentsfleetd` (8 dimensions + 2 bonus pass; exit 0)
- [x] Neither workflow references the removed var — `! grep -rn "OIDC_JWKS_URL" .github/workflows/` → 0 references ✓
- [x] Both workflows retain issuer + audience — `grep -c "OIDC_ISSUER\|OIDC_AUDIENCE" …` → deploy-dev=3, release=3 ✓
- [x] `make lint-zig` clean · `make test-unit-agentsfleetd` passes (note: repo has no `make test`/`make lint` umbrella — Zig lanes are `lint-zig` / `test-unit-agentsfleetd`)
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` → both OK
- [x] `gitleaks detect` clean (no leaks found) · no file over 350 lines (serve.zig held at 350) · `make memleak` 0 failed

---

## Eval Commands (post-implementation)

```bash
# E1: derivation tests
make test 2>&1 | grep -E "oidc_derives|explicit_jwks_overrides|missing_issuer|doctor_jwks_url_matches" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -3
# E3: Tests — make test
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: workflows no longer reference the derivable var (empty = pass)
grep -rn "OIDC_JWKS_URL" .github/workflows/ | head
# E8: Orphan sweep — old error symbol gone (empty = pass)
grep -rn "MissingOidcJwksUrl" src/ | head
```

---

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted (edits only; vault field removal is an ops action, not a repo file).

**2. Orphaned references** — zero remaining uses of the removed error symbol + the env var in workflows.

| Deleted symbol/ref | Grep | Expected |
|--------------------|------|----------|
| `MissingOidcJwksUrl` | `grep -rn "MissingOidcJwksUrl" src/ \| head` | 0 matches |
| `OIDC_JWKS_URL` (workflows) | `grep -rn "OIDC_JWKS_URL" .github/workflows/ \| head` | 0 matches |

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **CHORE(open) provenance cross-check (Jun 17 2026):** read `runtime_loader.zig`, `runtime_validate.zig`, `runtime_types.zig`, `env_vars.zig`, `oidc.zig`, `jwks.zig`, `doctor.zig`, `runtime.zig`, `serve.zig`, and the loader tests against the spec. Three corrections to Files-Changed: (1) `env_vars.zig` has **no** OIDC content (DB/Redis URL validation only) — the suffix constant + helper move to `auth/oidc.zig`, the optional-override doc moves to `docs/AUTH.md`; (2) `serve.zig` needs **no** change (`oidc_jwks_url orelse ""` already consumes the resolved value); (3) the enable-gate move (jwks_url → issuer) **breaks several existing loader tests** that set `OIDC_JWKS_URL` but no `OIDC_ISSUER` (`accepts custom provider`, `applies size defaults`, `rejects short/non-hex encryption key`, `rejects provider without required OIDC_JWKS_URL`, `rejects empty OIDC_JWKS_URL`) — these gain `OIDC_ISSUER` / are repurposed to the missing-issuer case, in-scope per §1/§2.
- **§3 is a guarded action:** `.github/workflows/**` edits are CI/CD-guarded (explicit approval required; auto-mode does not cover). Daemon §1+§2 proceed autonomously; §3 surfaces to Indy for go/no-go before the workflow edits land.
- **Pre-spec context (Jun 17 2026):** discovered during a CI-failure investigation. Sibling fixes already shipped/queued outside this spec: qa-dev Vercel-alias repoint (PR #419); Clerk `api` template `aud` typo `agentsfleeet`→`agentsfleet` (dashboard fix, Indy); dev+prod vault `jwks-url` corrected manually (prod was `clerk.usezombie.com`, dead). This spec removes the *ability* for jwks-url to drift again.
- **Ops step (do at §3, record here):** remove `clerk-dev`/`clerk-prod` `jwks-url` fields from 1Password only after both deploys confirm green on derived URLs.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-agentsfleetd` | exit 0; test-depth unit=1964 (baseline 1958, +6) integration=190 | ✅ |
| Lint (Zig) | `make lint-zig` | fmt ✓ · ZLint 0/0 across 468 files · pg-drain ✓ · schema-gate ✓ · no `-gnu` ✓ · line-limit ✓ | ✅ |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | X86_64_LINUX_OK + AARCH64_LINUX_OK | ✅ |
| Memleak | `make memleak` | 1249 passed; 380 skipped; 0 failed; allocator-leak gate passed | ✅ |
| Gitleaks | `gitleaks detect --no-banner --redact` | no leaks found | ✅ |
| CI gates | `make check-gh-actions-valid` + `check-playbooks` + `harness-verify` | actionlint + make-refs green · playbook refs resolve · gates green | ✅ |
| Workflow sweep | `grep -rn OIDC_JWKS_URL .github/workflows/` | 0 references | ✅ |
| Orphan sweep | `grep -rn MissingOidcJwksUrl src/` | 0 references (renamed → `MissingOidcIssuer`) | ✅ |

---

## Out of Scope

- Full OIDC discovery (`/.well-known/openid-configuration` → `jwks_uri`) — follow-up only if a non-conventional IdP appears.
- Moving the remaining public OIDC config (`issuer`, `audience`) out of the vault into committed per-env config / TOML — separate spec if pursued.
- Any change to `aud`/`iss`/`exp` verification, multi-issuer support, or the v3 agentsfleet-native issuer trajectory (`docs/AUTH.md` "Beyond Stage 2").
