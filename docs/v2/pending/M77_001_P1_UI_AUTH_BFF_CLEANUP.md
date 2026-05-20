# M77_001: Flow 2 dashboard BFF — `/api` is the single browser→zombied trust boundary

**Prototype:** v2.0.0
**Milestone:** M77
**Workstream:** 001
**Date:** May 20, 2026
**Status:** PENDING
**Priority:** P1 — closes the last browser-held-credential path on the operator dashboard; security boundary.
**Categories:** UI
**Batch:** B1 — standalone; no concurrent workstream depends on it.
**Branch:** {feat/m77-flow2-bff — added at CHORE(open)}
**Depends on:** M74_002 (Stage 1 single-token collapse — shipped §9; this builds directly on the customized session token).
**Provenance:** agent-generated (pre-spec, `docs/AUTH.md` §Roadmap — Flow 2 dashboard cleanup, Stage 2).

> **Provenance is load-bearing.** Agent-generated from the AUTH.md roadmap — cross-check every claim against `ui/packages/app/` before EXECUTE; the roadmap describes the target, the code is the baseline.

**Canonical architecture:** `docs/AUTH.md` §"Roadmap — Flow 2 dashboard cleanup" → "Stage 2 — Option 3: BFF on top of single-token" (the auth source of truth; Architecture Consult & Update Gate — the doc wins until reconciled, and §6.4 below reconciles it).

---

## Implementing agent — read these first

1. `docs/AUTH.md` §820–1043 (Roadmap — Flow 2 dashboard cleanup) — the Stage-1-shipped baseline, the Stage 2 target architecture, the two wire-shape diagrams, and the four-row threat model. This spec is its execution.
2. `ui/packages/app/app/backend/v1/workspaces/[workspaceId]/zombies/[zombieId]/events/stream/route.ts` — the **one** existing route handler; the known-good in-repo pattern for "read the cookie, mint the session token server-side via `auth().getToken()`, forward upstream." Every new `/api/*` handler mirrors its token-handling shape.
3. `ui/packages/app/lib/actions/with-token.ts` — the Server-Action token wrapper (`withToken` → `ActionResult<T>` discriminated union). Mutations migrated in §3 reuse it; do not invent a second error shape.
4. `src/auth/audit.zig` + `src/auth/audit_events.zig` — the M74_002 pseudonymized audit shape (HMAC-SHA256 over a pepper, `*HashHex` / `*Prefix` helpers; raw identifiers never leave the process). §4's `/api/*` audit emit mirrors this shape in TypeScript.
5. `tests/grep-gates/no-api-template-mint.test.ts` — the existing grep-gate asserting exactly one `getToken({template:"api"})` survives. §3 extends this gate to also forbid client-side `getToken()` / `Authorization: Bearer`.

---

## PR Intent & comprehension handshake

> The bridge from spec to merged PR.

- **PR title (eventual):** Flow 2 BFF: dashboard calls route through `/api`, no browser token
- **Intent (one sentence):** every dashboard→zombied call funnels through Next.js `/api/*` route handlers and Server Actions that mint the session token server-side, so the browser carries only the `__session` cookie and zero usable credential ever lands in the JavaScript heap.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list `ASSUMPTIONS I'M MAKING: …`. Name at least: (1) the customized session token from Stage 1 is the only credential `/api/*` mints — Backend-for-Frontend (BFF) does not re-introduce Token B; (2) the CLI carve-out at `app/cli-auth/[session_id]/page.tsx` is untouched; (3) zombied's verifier is unchanged — no `src/auth/**` diff. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

> Re-read before EXECUTE, re-check during VERIFY.

- **`docs/greptile-learnings/RULES.md`** — universal. Named IDs the diff trips: **RULE ORP** (cross-layer orphan sweep on the `app/backend/`→`app/api/` rename and `lib/api/*` delete — the load-bearing one), **RULE NDC** (no dead code at write time), **RULE NLR** (touch-it-fix-it cleanup of residue), **RULE NLG** (pre-2.0 — no "legacy"/"V2"/compat-shim framing; removed Next routes 404, never 410), **RULE UFS** (the upstream origin, the `/api` path prefix, the audit scope name, and Insecure-Direct-Object-Reference (IDOR) error strings are named constants), **RULE EMS** (401/403 responses follow the standard error-message structure).
- **`docs/BUN_RULES.md`** + TypeScript-strict (M68 §14): no `as any` / `!` / `@ts-expect-error` to silence strictness — catch null/undefined at compile time.
- **`docs/LOGGING_STANDARD.md`** — §4's audit emit adds log emits in TypeScript; structured, no secrets, identifiers pseudonymized.
- **UFS manual carve-out:** the UFS audit skips `ui/` files — extract repeated literals and union tags (`as const`) **by hand** in `ui/` TypeScript; the harness will not catch them.

---

## Applicable Gates

> Which Action-Triggered Guards fire, and how each stays clean. Blast radius is `ui/packages/app/**` (TypeScript/TSX) + one `docs/AUTH.md` reconciliation. No Zig, no schema.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | zombied verifier unchanged; no `*.zig` touched. |
| PUB / Struct-Shape / LIFECYCLE | no | no Zig surface. |
| File & Function Length (≤350/≤50/≤70) | yes | `.ts`/`.tsx` are in the length surface — one small handler per endpoint; shared mint+forward+IDOR logic in a server-only helper module; split before any file nears 350. |
| UFS (repeated/semantic literals) | yes | UFS audit skips `ui/` — extract the upstream origin, `/api` prefix, audit scope, and error codes to named `const`s **by hand**; reuse the existing `ERROR_CODE` table. |
| UI Substitution / DESIGN TOKEN | no | data-plane work; `ZombieThread.tsx` edit in §3 swaps a client fetch for a Server Action — no new raw HTML elements, no arbitrary Tailwind. Re-check if markup changes. |
| LOGGING | yes | §4 audit emit — pseudonymized identifiers, no secrets, errors to the structured sink; mirror `src/auth/audit.zig` shape per `LOGGING_STANDARD.md`. |
| ERROR REGISTRY | no | error codes live in the `ui/` TypeScript `ERROR_CODE` table, not `src/**`/`zombiectl/**` `UZ-*` registry. |
| MILESTONE-ID | yes | no `M77`/`§`/`dim N.M` strings in code, comments, or test names (RULE TST-NAM); spec prose is exempt. |

---

## Overview

**Goal (testable):** every dashboard→zombied request originates server-side from a Next.js `/api/*` route handler or a `"use server"` Server Action that mints the customized session JSON Web Token (JWT) in-function and forwards it as `Authorization: Bearer`; the browser's outgoing requests carry only the `__session` cookie, and a grep-gate proves zero client-side `getToken()` / `Authorization: Bearer` outside the CLI carve-out.

**Problem:** post-Stage-1 the dashboard is *mostly* server-side, but two browser-credential leaks remain: `steerZombie` is fetched directly from the `ZombieThread.tsx` client component (token in the JavaScript heap), and there is no single audited trust boundary at which cross-tenant access (IDOR) and replay are checked before the upstream call. The `/backend/:path*` rewrite also exposes a raw proxy path with no per-request authorization.

**Solution summary:** rename `app/backend/` → `app/api/`, retire the raw rewrite, and add one route handler per zombied endpoint the dashboard touches. Each handler reads `__session`, mints the session token server-side, runs an IDOR check (session `tenant_id` vs URL `workspace_id`) and an audit emit *before* the upstream fetch, then forwards. Server pages invoke handler functions in-process; client mutations (including `steerZombie`) route through Server Actions. `lib/api/*` is deleted. Outcome: `/api/*` is the single browser→zombied trust boundary; the browser is no longer a credential courier.

---

## Prior-Art / Reference Implementations

- **Route-handler mint+forward** → the existing SSE (Server-Sent Events) handler `app/backend/.../events/stream/route.ts` — already does cookie-read → server-side `auth().getToken()` → upstream forward. Mirror its token handling for every new `/api/*` handler; do not invent a new mint path. **Alignment: exact.**
- **Server-Action token wrapper** → `lib/actions/with-token.ts` (`withToken` / `ActionResult<T>`). Migrated mutations reuse it verbatim. **Alignment: exact.**
- **Audit pseudonymization** → `src/auth/audit.zig` (`sessionIdHashHex` / `sessionIdPrefix`, HMAC over `AUDIT_LOG_PEPPER`). §4 mirrors the *shape* in TypeScript; it does not call the Zig code (zombied stays unchanged). **Divergence: cross-runtime — same algorithm, separate implementation; justified because the IDOR check must run in the Next runtime before the upstream hop.**

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/backend/` (whole subtree) | DELETE | renamed; the SSE route relocates under `app/api/`. |
| `ui/packages/app/app/api/v1/.../route.ts` (one per touched endpoint, ~12) | CREATE | mint server-side + IDOR + audit + forward; the new trust boundary. |
| `ui/packages/app/lib/server/api-handlers.ts` (or similar) | CREATE | shared mint+forward+IDOR helper invoked by route handlers and in-process by server pages. |
| `ui/packages/app/next.config.ts` | EDIT | remove the `/backend/:path*` rewrite — handlers serve every path directly. |
| `ui/packages/app/lib/api/{approvals,client,credentials,errors,events,retry,tenant_billing,tenant_provider,workspaces,zombies}.ts` | DELETE | logic absorbed into route handlers / shared helper. |
| `ui/packages/app/app/(dashboard)/**/page.tsx` (10 server pages) | EDIT | call handler functions in-process; drop `lib/api` imports. |
| `ui/packages/app/app/(dashboard)/**/actions.ts` (7 Server Actions) | EDIT | point at the new path; keep `withToken`. |
| `ui/packages/app/components/domain/ZombieThread.tsx` | EDIT | replace the direct `steerZombie` client fetch with a `steerZombieAction` Server Action call. |
| `ui/packages/app/app/(dashboard)/zombies/actions.ts` | EDIT | add `steerZombieAction`. |
| `ui/packages/app/tests/grep-gates/no-api-template-mint.test.ts` | EDIT | extend to forbid client-side `getToken()` / `Authorization: Bearer`. |
| `docs/AUTH.md` | EDIT | reconcile the stale Flow 2 section (lines 366–461) to the `/api` BFF wire shape (DOCUMENT stage). |

> The CLI carve-out `ui/packages/app/app/cli-auth/[session_id]/page.tsx` is **deliberately absent** — it is not edited. No `src/**` file appears — zombied is unchanged.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections — (1) the `/api` substrate, (2) server-page/action consumers + `lib/api` delete, (3) the client-mutation close-out, (4) defense-in-depth. The split tracks the trust-boundary build-up: substrate before consumers before hardening.
- **Alternatives considered:** (a) keep the `/backend` rewrite and only fix `steerZombie` — rejected; leaves a raw unauthenticated proxy path and no single audit/IDOR chokepoint. (b) Bundle the v3 usezombie-native issuer work — rejected; that replaces Clerk as issuer and is explicitly out of scope (AUTH.md *Beyond Stage 2*).
- **Patch-vs-refactor verdict:** a **scoped refactor** of the dashboard data plane — not a patch (the rewrite + `lib/api` layer is removed wholesale) and not a platform refactor (zombied, Clerk, and the CLI are untouched). The larger "usezombie-native capability token" refactor is named in *Out of Scope* as the v3 follow-up.

---

## Sections (implementation slices)

### §1 — `/api` route-handler substrate

Establish the boundary. Rename `app/backend/`→`app/api/` (the SSE route relocates), delete the `next.config.ts` rewrite, and create one route handler per distinct zombied endpoint the dashboard reads (the agent enumerates them from the deleted `lib/api/*` call sites — ~12 today). Each handler reads `__session` via `auth()`, mints the session token server-side, and forwards upstream. **Implementation default:** mint+forward+IDOR live in one shared server-only helper that each handler calls, so the per-handler file stays under the length cap.

- **Dimension 1.1** — `app/backend/` removed; SSE route served from `app/api/...`; the browser `EventSource` URL points at `/api/...` → Test `events_stream_served_from_api_path`.
- **Dimension 1.2** — `/backend/:path*` rewrite deleted from `next.config.ts`; a request to `/backend/...` 404s (RULE NLG: not 410) → Test `backend_rewrite_removed_returns_404`.
- **Dimension 1.3** — each read endpoint has an `/api/v1/...` handler that mints server-side and forwards upstream status+body → Test `api_handler_forwards_upstream_with_bearer`.

### §2 — Server pages + Server Actions consume `/api`; delete `lib/api/*`

Wire the consumers and remove the old layer. Server pages import the handler function and invoke it in-process (no loopback fetch); Server Actions point at the new path while keeping `withToken`; all ten `lib/api/*.ts` files are deleted with zero remaining imports.

- **Dimension 2.1** — the ten `(dashboard)` server pages render via in-process handler calls; no page issues a loopback `fetch("/api...")` to itself → Test `server_pages_invoke_handler_in_process`.
- **Dimension 2.2** — all `lib/api/*.ts` deleted; repo grep for `@/lib/api` and `lib/api/` returns zero imports (RULE ORP) → Test `lib_api_fully_orphaned`.

### §3 — Close the client-mutation hole

`steerZombie` is the lone mutation still fetched from a client component. Move it to a `steerZombieAction` Server Action; confirm the seven named mutations (steer / kill / approve / deny / install-zombie / delete-credential / set-provider) are all Server Actions; extend the grep-gate to forbid client-side credential handling.

- **Dimension 3.1** — `ZombieThread.tsx` invokes `steerZombieAction`; the component contains no `getToken` and no `Authorization` header → Test `steer_routed_through_server_action`.
- **Dimension 3.2** — grep-gate: zero `getToken()` / `getToken({template:"api"})` / `Authorization: Bearer` in any `"use client"` component, and exactly one surviving api-template mint (the CLI carve-out) → Test `no_client_side_credentials_gate`.

### §4 — `/api/*` defense-in-depth (IDOR + audit)

The boundary earns its keep: every accept and every cross-tenant block is checked and audited before the upstream hop. **Implementation default:** the IDOR check reads `metadata.tenant_id` from the session and asserts the URL `workspace_id` belongs to it; the audit emit uses an `AUDIT_LOG_PEPPER` available to the Next.js runtime (same secret name as zombied's, provisioned to the Vercel project as part of this slice if backend-only today) and mirrors `sessionIdHashHex`.

- **Dimension 4.1** — a request whose URL `workspace_id` belongs to a different tenant returns 403 **before** any upstream fetch → Test `cross_tenant_request_blocked_pre_upstream`.
- **Dimension 4.2** — a request with no `__session` cookie returns 401 with no upstream call → Test `unauthenticated_request_rejected`.
- **Dimension 4.3** — the audit emit carries pseudonymized identifiers + client-IP divergence flags and never logs a raw token, raw `session_id`, or the pepper → Test `audit_emit_redacts_secrets`.

---

## Interfaces

> The contract the agent must not change without amending this spec.

```
Route handler — GET/POST /api/v1/workspaces/{ws}/...
  Request  : Cookie: __session=<customized session JWT>   (NO Authorization header from the browser)
  Behaviour: auth() → null  ⇒ 401 { error, errorCode: AUTH_401 }     (no upstream call)
             session.tenant_id ≠ ws.tenant_id ⇒ 403 { error, errorCode: AUTH_403 }  (no upstream call)
             else: getToken() → Bearer → fetch upstream → forward {status, body}
  Audit    : emit on every accept AND every 401/403 — pseudonymized actor + ws + ip-divergence

Server Action — "use server" steerZombieAction({ zombieId, message })
  Transport: React RPC (form-encoded, built-in same-origin check); reads __session cookie
  Returns  : ActionResult<T> = { ok:true, data } | { ok:false, error, status?, errorCode? }
```

The customized session JWT shape, the upstream origin env var (`NEXT_PUBLIC_API_URL`), and zombied's verifier are unchanged from Stage 1.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unauthenticated | no `__session` cookie | route handler `auth()` → null ⇒ 401, no upstream call; browser sees 401 JSON. |
| Cross-tenant (IDOR) | URL `workspace_id` ∉ session tenant | 403 before upstream; audit emit records the blocked attempt. |
| Token mint failure | `getToken()` returns null mid-request | 401 `AUTH_401`; no partial upstream call; caller sees a re-auth prompt. |
| Upstream timeout / 5xx | zombied slow or erroring | handler forwards upstream status (502/504 surfaced); no token leaked in the error body. |
| Malformed `workspace_id` | non-UUID / injected path segment | reject at the parse boundary (narrow type) ⇒ 400 before mint/upstream. |
| SSE upstream disconnect | zombied closes the event stream | handler closes the client stream cleanly; `EventSource` reconnects per existing behaviour. |
| Server-Action CSRF | attacker origin invokes `steerZombieAction` | React Server-Action transport's same-origin check rejects; no bare public POST route exists to bypass it. |
| Token expiry mid-request | ~60s session JWT lapses | Clerk SDK refreshes on the next `getToken()`; handler mints fresh per request — no long-lived heap copy to expire. |

---

## Invariants

> Each enforceable by code, not review discipline.

1. **Zero client-side credentials (the Stage-2 deliverable)** — no `getToken()` / `Authorization: Bearer` in any `"use client"` component; the browser holds no copy of the *customized session token* → grep-gate test (§3.2), fails the suite on violation.
2. **Exactly one api-template mint survives (Stage-1 carry-forward, NOT a Stage-2 target)** — only `app/cli-auth/[session_id]/page.tsx`. This is a non-regression guard: the BFF must neither delete the CLI carve-out nor add new `getToken({template:"api"})` sites. Stage 1 already reduced api-template mints to this one; Stage 2 leaves it untouched (it serves the CLI's ~15-min on-disk token, which the ~60s session token cannot) → existing grep-gate, extended.
3. **No `app/backend/` path remains** — the directory is gone → grep-gate + `next build` (no route resolves under `/backend`).
4. **IDOR check precedes the upstream hop** — integration test asserts the upstream fetch is never called on a tenant mismatch (mock asserts zero calls).
5. **`lib/api/*` fully orphaned** — repo grep for `@/lib/api` returns zero (RULE ORP) → §2.2 test.
6. **zombied unchanged** — `git diff origin/main -- src/` is empty for this PR → Eval E8.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `events_stream_served_from_api_path` | `GET /api/v1/.../events/stream` returns the SSE stream; `/backend/...` does not resolve. |
| 1.2 | integration | `backend_rewrite_removed_returns_404` | `GET /backend/v1/anything` → 404 (not 410, not proxied). |
| 1.3 | integration | `api_handler_forwards_upstream_with_bearer` | mocked upstream receives `Authorization: Bearer <sess JWT>`; handler returns the upstream status+body. |
| 2.1 | integration | `server_pages_invoke_handler_in_process` | rendering a dashboard page issues no self-loopback HTTP fetch (spy asserts in-process call). |
| 2.2 | unit (grep) | `lib_api_fully_orphaned` | repo scan: `@/lib/api` and `lib/api/` import count == 0. |
| 3.1 | e2e | `steer_routed_through_server_action` | Playwright: submitting a steer message triggers a Server Action; network trace shows no browser `Authorization` header. |
| 3.2 | unit (grep) | `no_client_side_credentials_gate` | scan of `"use client"` files: `getToken`/`Authorization: Bearer` count == 0; api-template mint count == 1. |
| 4.1 | integration | `cross_tenant_request_blocked_pre_upstream` | session tenant A + URL workspace of tenant B → 403; upstream mock asserted called 0 times. |
| 4.2 | integration | `unauthenticated_request_rejected` | no `__session` cookie → 401; upstream mock called 0 times. |
| 4.3 | unit | `audit_emit_redacts_secrets` | emitted record contains a hash/prefix, never the raw token / `session_id` / pepper. |

**Regression:** the ten dashboard pages render the same data post-refactor (snapshot of rendered data unchanged). **Idempotency/replay:** §4.3 covers replay-surfacing via audit; the ~60s TTL is the replay-window control (no new dedup store).

---

## Acceptance Criteria

- [ ] Browser carries zero token: e2e network trace has no client `Authorization` header — verify: `cd ui/packages/app && bun run test-e2e -- --grep steer`
- [ ] Grep-gates green (zero client credentials, one api-template mint) — verify: `cd ui/packages/app && bun run test -- grep-gates`
- [ ] IDOR + unauthenticated blocked pre-upstream — verify: `cd ui/packages/app && bun run test -- api`
- [ ] `lib/api/*` orphaned, `app/backend/` gone — verify: `grep -rn "@/lib/api\|app/backend" ui/packages/app/ | grep -v node_modules` (empty)
- [ ] zombied untouched — verify: `git diff --name-only origin/main -- src/ | wc -l` (0)
- [ ] `make lint` clean · `cd ui/packages/app && bun run test:coverage` passes the gate
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: Client carries no credential — grep-gate suite
cd ui/packages/app && bun run test -- grep-gates && echo "PASS" || echo "FAIL"
# E2: Build  — next build
cd ui/packages/app && bun run build 2>&1 | tail -5
# E3: Tests + coverage gate
cd ui/packages/app && bun run test:coverage 2>&1 | tail -8
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: e2e user path (steer via Server Action)
cd ui/packages/app && bun run test-e2e -- --grep steer 2>&1 | tail -5
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Orphan + zombied-untouched sweep (both empty = pass) —
grep -rn "@/lib/api\|app/backend\|fetch(\"/backend" ui/packages/app/ | grep -v node_modules
git diff --name-only origin/main -- src/
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/lib/api/*.ts` (10 files) | `test ! -d ui/packages/app/lib/api` |
| `ui/packages/app/app/backend/` (subtree) | `test ! -d ui/packages/app/app/backend` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `@/lib/api` imports | `grep -rn "@/lib/api" ui/packages/app/ \| grep -v node_modules` | 0 matches |
| `/backend` fetch paths | `grep -rn 'fetch("/backend\|"/backend/' ui/packages/app/ \| grep -v node_modules` | 0 matches |
| `/backend` rewrite | `grep -n "/backend" ui/packages/app/next.config.ts` | 0 matches |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: question asked + Indy's decision.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results.
- **Deferrals** — each needs an Indy-acked verbatim quote: `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <item, why>`.
- **Open at authoring:** §4 assumes `AUDIT_LOG_PEPPER` can be provisioned to the Next.js runtime — confirm at PLAN whether it is backend-only today; if so, provisioning it to Vercel is in §4 scope.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits diff coverage vs the Test Specification (note: bun marks erased import-type lines 0-hit — gate on the aggregate, not per-file). | Clean; iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | adversarial diff review vs this spec, `docs/AUTH.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

Skill unavailable (MCP down) → document the skip in Discovery + the PR description with a timestamp and "rerun before merge".

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + grep-gates | `cd ui/packages/app && bun run test` | {paste} | |
| Coverage gate | `cd ui/packages/app && bun run test:coverage` | {paste} | |
| e2e (user-centric steer) | `bun run test-e2e -- --grep steer` | {paste} | |
| Lint | `make lint` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| Orphan + zombied sweep | `grep -rn "@/lib/api" …` · `git diff --name-only origin/main -- src/` | {paste} | |

---

## Out of Scope

- **Usezombie-native capability tokens** (replacing Clerk as issuer; CLI off the Clerk JWT) — the v3 trajectory per AUTH.md *Beyond Stage 2*; its own milestone, never bundled here.
- **CLI carve-out** — `app/cli-auth/[session_id]/page.tsx` keeps its api-template mint (Flow 1); untouched.
- **Content-Security-Policy / Subresource-Integrity / dependency pinning** — the XSS/supply-chain closure the threat model defers; separate spec.
- **Flow 1 (CLI) and Flow 3 (tenant API keys)** — unaffected by this work.
