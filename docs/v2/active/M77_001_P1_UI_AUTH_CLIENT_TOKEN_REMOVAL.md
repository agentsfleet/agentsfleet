# M77_001: Dashboard carries no client-side token — remove token props, route consumers server-side

**Prototype:** v2.0.0
**Milestone:** M77
**Workstream:** 001
**Date:** May 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a live api-audience JWT is currently serialized into the dashboard's hydration payload; passive token exposure on a security boundary.
**Categories:** UI
**Batch:** B1 — standalone.
**Branch:** feat/m77-client-token-removal
**Depends on:** M74_002 (Stage 1 single-token collapse — shipped §9; this finishes Stage 1's "browser holds no token" goal).
**Provenance:** agent-generated (pre-spec, `docs/AUTH.md` §Roadmap — Flow 2 dashboard cleanup; scoped down from the full Backend-for-Frontend (BFF) proposal after an Indy scope review on May 21, 2026).

> **Provenance is load-bearing.** Agent-generated and deliberately *narrowed* from the full BFF. The full `/api` boundary (route handlers, `lib/api` teardown, IDOR, audit, proxy removal) is **deferred** — see *Out of Scope*. Cross-check the leak against `ui/packages/app/` before EXECUTE.

**Canonical architecture:** `docs/AUTH.md` §"Roadmap — Flow 2 dashboard cleanup" (current direction vs future direction; reconciled in the same change that lands this spec).

---

## Implementing agent — read these first

1. `docs/AUTH.md` §"Roadmap — Flow 2 dashboard cleanup" — the current-vs-future split. This spec is the *current* slice; the full BFF is *future*. Do not build the deferred parts.
2. `ui/packages/app/app/(dashboard)/zombies/actions.ts` — the six existing Server Actions (`installZombieAction`, `killZombie`, …) wrapped by `withToken`. The new `steerZombieAction` mirrors them exactly; do not invent a second mutation shape.
3. `ui/packages/app/lib/actions/with-token.ts` — `withToken` → `ActionResult<T>`; `auth().getToken()` server-side, token never returned to the client.
4. `ui/packages/app/components/domain/ZombieThread.tsx` — the leak site: `token: string \| null` prop (line ~66) consumed by `steerZombie` and `useZombieEventStream`. The prop is serialized into hydration data — that is what this spec removes.
5. `ui/packages/app/tests/grep-gates/no-api-template-mint.test.ts` — the existing carve-out grep-gate to extend.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Dashboard: no client-side token props — route steer + stream server-side
- **Intent (one sentence):** stop serializing a Clerk api-audience JWT into the dashboard's hydration payload by removing the `token` prop from client components and routing their token-needing calls through Server Actions / the existing Server-Sent Events (SSE) route handler.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. Name at least: (1) the customized session token (Stage 1) is what Server Actions mint — no Token B; (2) the cli-auth carve-out and zombied are untouched; (3) the full `/api` BFF is out of scope. A mismatch → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — named IDs the diff trips: **RULE NDC** (no dead code — the removed `token` props/params leave no orphans), **RULE NLR** (touch-it-fix-it on the components edited), **RULE NLG** (pre-2.0: no "legacy"/compat-shim framing), **RULE ORP** (orphan sweep on the removed prop/param across the component tree), **RULE UFS** (any shared literal/union tag extracted by hand — UFS audit skips `ui/`), **RULE EMS** (the Server-Action error path returns the standard `ActionResult` error shape).
- **`docs/BUN_RULES.md`** + TypeScript-strict (M68 §14): no `as any` / `!` / `@ts-expect-error`; the removed nullable token must not be replaced by a non-null assertion elsewhere.
- **UFS manual carve-out:** the UFS audit skips `ui/` — extract repeated literals + union tags (`as const`) by hand.

---

## Applicable Gates

> Blast radius is the zombie-detail client subtree under `ui/packages/app/` + one grep-gate. No Zig, no schema, no new logging.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE / PUB / LIFECYCLE / SCHEMA / ERROR REGISTRY | no | no Zig, no schema, no `src/**` error codes touched. |
| File & Function Length (≤350/≤50/≤70) | yes | `.ts`/`.tsx` in the surface; the new Server Action is small; keep it under the caps. |
| UFS | yes | UFS audit skips `ui/` — extract any shared literal/union tag by hand. |
| UI Substitution / DESIGN TOKEN | no | removing a prop and a fetch is not raw-HTML or arbitrary-Tailwind; re-check only if markup changes. |
| LOGGING | no | no new log emits (the deferred BFF's audit emit is out of scope). |
| MILESTONE-ID | yes | no `M77`/`§`/`dim N.M` in code, comments, or test names (RULE TST-NAM); spec prose is exempt. |

---

## Overview

**Goal (testable):** no dashboard `"use client"` component receives a Clerk token as a prop and none calls `getToken()`; `steerZombie` and the event-stream backfill run server-side; a grep-gate fails the suite if a token prop or client `getToken()` reappears (the cli-auth carve-out excepted).

**Problem:** the zombie-detail server page forwards `token={token}` into client components (`ZombieThread`, `ZombieApprovalsPanel`). A token in a client-component prop is serialized into the React hydration payload — a live ~60-second api-audience JWT is readable in the page's HTML/JS heap with no Cross-Site Scripting (XSS) required (shared screen, cached page, DOM-reading browser extension).

**Solution summary:** add `steerZombieAction` (Server Action) and route `ZombieThread`'s steer through it; serve the event stream's initial backfill server-side (Server Action or server-rendered initial data) so the stream hook needs no client token; the SSE connection keeps using the existing route handler that mints server-side. Drop the `token` prop from every dashboard client component and stop the server page forwarding it. Lock it with a grep-gate. The full `/api` BFF stays deferred.

---

## Prior-Art / Reference Implementations

- **Server Action pattern** → the six existing actions in `app/(dashboard)/zombies/actions.ts` + `lib/actions/with-token.ts`. `steerZombieAction` is the seventh, identical in shape. **Alignment: exact.**
- **SSE server-side mint** → the existing route handler `app/backend/.../events/stream/route.ts` already mints server-side; the stream connection reuses it unchanged. **Alignment: exact — no new handler.**
- No new architecture; this is subtraction (remove props) plus one Server Action.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/zombies/actions.ts` | EDIT | add `steerZombieAction` (mirrors the existing six). |
| `ui/packages/app/components/domain/ZombieThread.tsx` | EDIT | drop the `token` prop; call `steerZombieAction`; stop passing token to the stream hook. |
| `ui/packages/app/components/domain/ZombieThreadDynamic.tsx` | EDIT | drop the forwarded `token` prop. |
| `ui/packages/app/components/domain/` event-stream hook (`use-zombie-event-stream*`) + `useNewMessageHandler` | EDIT | remove the `token` param; backfill server-side; SSE via the existing route handler. |
| `ui/packages/app/components/domain/ZombieApprovalsPanel*` | EDIT | drop the `token` prop; use the existing approve/deny Server Actions. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx` | EDIT | stop forwarding `token={token}` to client components; pass server-rendered data instead. |
| `ui/packages/app/tests/grep-gates/no-api-template-mint.test.ts` | EDIT | extend to forbid client `getToken()` + token-typed props to client components. |
| `docs/AUTH.md` | EDIT | reconcile the stale Flow 2 section + Roadmap to current direction (this slice) vs future direction (the deferred full BFF). DOCUMENT stage. |

> Absent on purpose: `cli-auth/[session_id]/page.tsx`, `next.config.ts` (the `/backend` proxy stays — still used by the carve-out), all `lib/api/*` (kept), and any `src/**` (zombied unchanged).

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two Sections — remove the leak (§1), lock it (§2). Scoped to the zombie-detail client subtree.
- **Alternatives considered:** (a) the full `/api` BFF (route handlers, `lib/api` teardown, IDOR, audit, proxy removal) — deferred; its value is a single audited boundary + rate-limit home, neither needed now, and it would re-line ~12 Clerk-JWT handlers that the v3 capability-token work rewrites anyway. (b) Leave the token prop and only fix `steerZombie` — rejected; the stream hook reads the same prop, so the hydration leak would remain.
- **Patch-vs-refactor verdict:** a **focused patch** — subtract client token props, add one Server Action. The larger refactor (full BFF) is named in *Out of Scope* and belongs with v3.

---

## Sections (implementation slices)

### §1 — Remove client token props; route consumers server-side

The dashboard must hold no token in client state. `steerZombie` moves to a Server Action; the event-stream backfill runs server-side; the SSE connection keeps using the route handler. Every `token` prop on a dashboard client component is removed, and the zombie-detail page stops forwarding it. **Implementation default:** the stream's initial events are fetched in the server component and passed as *data* (not a token); live updates ride the existing SSE route handler.

- **Dimension 1.1** — `steerZombie` is invoked via `steerZombieAction`; `ZombieThread` contains no `getToken`, no `Authorization`, no `token` prop → Test `steer_routed_through_server_action`.
- **Dimension 1.2** — the event-stream hook needs no client token: backfill is server-side and the SSE connection carries only the `__session` cookie → Test `event_stream_carries_no_client_token`.
- **Dimension 1.3** — the `token` prop is gone from every dashboard client component and the zombie-detail page no longer forwards it → Test `no_token_prop_on_client_components`.

### §2 — Lock the invariant (grep-gate)

Make "the browser holds no dashboard token" enforceable so it cannot regress without the full BFF in place.

- **Dimension 2.1** — grep-gate: zero `getToken(` and zero `Authorization: Bearer` in any `"use client"` file, and zero token-typed props passed to a client component → Test `no_client_side_credentials_gate`.
- **Dimension 2.2** — the gate preserves the single api-template carve-out (`cli-auth/[session_id]/page.tsx`) — neither deleted nor duplicated → Test `api_template_carveout_preserved`.

---

## Interfaces

```
Server Action — "use server" steerZombieAction({ workspaceId, zombieId, message })
  Transport : React RPC (form-encoded, built-in same-origin check); reads __session cookie
  Server    : withToken(token => steerZombie(workspaceId, zombieId, message, token))
  Returns   : ActionResult<{ event_id: string }>
              = { ok:true, data } | { ok:false, error, status?, errorCode? }

ZombieThread / ZombieApprovalsPanel props: the `token` field is REMOVED.
  Initial event data arrives as a server-rendered prop; live updates via the
  existing SSE route handler (cookie-only). No JWT crosses the RSC→client boundary.
```

The customized session token shape, `NEXT_PUBLIC_API_URL`, the SSE route handler, and zombied are unchanged.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Session expired on steer | `auth().getToken()` returns null in the action | `ActionResult { ok:false, errorCode: AUTH_401 }`; UI shows a re-auth prompt; no partial send. |
| Upstream error on steer | zombied 4xx/5xx | `ActionResult` carries `status` + `errorCode`; optimistic message is rolled back. |
| Backfill auth failure | server-side backfill 401 | server page surfaces an auth error; no token ever in the client. |
| SSE connection drop | upstream stream closes | existing `EventSource` reconnect behaviour, unchanged. |
| Token prop reintroduced | a future edit adds `token={…}` to a client component | grep-gate (§2.1) fails the test suite in CI. |

---

## Invariants

> Each enforceable by code, not review discipline.

1. **No Clerk token in a client-component prop or hydration payload** — grep-gate forbids token-typed props to `"use client"` components (§2.1).
2. **No `getToken()` in a `"use client"` file** — grep-gate (§2.1).
3. **Exactly one api-template mint survives** — only `cli-auth/[session_id]/page.tsx` (existing gate, §2.2). Carry-forward from Stage 1; do not delete or duplicate.
4. **zombied + cli-auth untouched** — `git diff --name-only origin/main -- src/ ui/packages/app/app/cli-auth` is empty for this PR.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `steer_routed_through_server_action` | Playwright: sending a steer message triggers a Server Action; network trace shows no client `Authorization` header. |
| 1.2 | unit | `event_stream_carries_no_client_token` | the stream hook builds the SSE URL + consumes backfill data without a token argument; no token in the request. |
| 1.3 | unit (grep) | `no_token_prop_on_client_components` | scan: zero `token=` props passed to `"use client"` components; `ZombieThread`/`ZombieApprovalsPanel` props have no `token` field. |
| 2.1 | unit (grep) | `no_client_side_credentials_gate` | scan of `"use client"` files: `getToken(` and `Authorization: Bearer` counts == 0. |
| 2.2 | unit (grep) | `api_template_carveout_preserved` | `template: "api"` count == 1, only at the cli-auth carve-out. |

**Regression:** the zombie-detail page renders the same events + steer behaves identically after the token prop is removed (snapshot of rendered data unchanged). **Idempotency/replay:** N/A — no new retry semantics; steer keeps its existing optimistic-rollback behaviour.

---

## Acceptance Criteria

- [ ] No client token: e2e network trace has no client `Authorization` header — verify: `cd ui/packages/app && bun run test-e2e -- --grep steer`
- [ ] Grep-gates green (no client `getToken`/Bearer/token-prop; one api-template mint) — verify: `cd ui/packages/app && bun run test -- grep-gates`
- [ ] Token prop orphaned everywhere — verify: `grep -rn "token=" ui/packages/app/components/domain ui/packages/app/app/'(dashboard)' | grep -v node_modules` (no token props to client components)
- [ ] zombied + cli-auth untouched — verify: `git diff --name-only origin/main -- src/ ui/packages/app/app/cli-auth | wc -l` (0)
- [ ] `make lint` clean · `cd ui/packages/app && bun run test:coverage` passes the gate
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: grep-gate suite (the invariant lock)
cd ui/packages/app && bun run test -- grep-gates && echo "PASS" || echo "FAIL"
# E2: Build  — next build
cd ui/packages/app && bun run build 2>&1 | tail -5
# E3: Tests + coverage gate
cd ui/packages/app && bun run test:coverage 2>&1 | tail -8
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: e2e steer via Server Action
cd ui/packages/app && bun run test-e2e -- --grep steer 2>&1 | tail -5
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: zombied + cli-auth untouched (empty = pass)
git diff --name-only origin/main -- src/ ui/packages/app/app/cli-auth
```

---

## Dead Code Sweep

> No files deleted — this is prop/param removal. Orphan check on the removed `token` prop/param.

| Removed symbol | Grep | Expected |
|----------------|------|----------|
| `token` prop on dashboard client components | `grep -rn "token=" ui/packages/app/components/domain ui/packages/app/app/'(dashboard)'` | no token props to client components |
| client `getToken` | `grep -rn "getToken(" ui/packages/app/components ui/packages/app/app/'(dashboard)'` | 0 matches |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults, skill outcomes, and Indy-acked deferral quotes.

- **Scope decision (captured at authoring):** Indy chose to descope from the full BFF to this hygiene slice on May 21, 2026 — no near-term need for tenant rate-limiting or operator-facing authz audit; a real authz audit belongs in zombied (covers all flows), not the dashboard `/api` layer; v3 not imminent.
- **Consults / Skill chain / Deferrals** — appended during EXECUTE; any deferral needs an Indy-acked verbatim quote.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits diff coverage vs the Test Specification (bun marks erased import-type lines 0-hit — gate on the aggregate). | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | adversarial diff review vs this spec, `docs/AUTH.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | review-comments the open PR against the immutable diff. | Comments addressed before merge. |

Skill unavailable (MCP down) → document the skip in Discovery + the PR with a timestamp and "rerun before merge".

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + grep-gates | `cd ui/packages/app && bun run test` | {paste} | |
| Coverage gate | `cd ui/packages/app && bun run test:coverage` | {paste} | |
| e2e steer | `bun run test-e2e -- --grep steer` | {paste} | |
| Lint | `make lint` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| zombied/cli-auth untouched | `git diff --name-only origin/main -- src/ ui/.../cli-auth` | {paste} | |

---

## Out of Scope (deferred to a future v3-coupled spec)

- **The full `/api` Backend-for-Frontend** — rename `app/backend/`→`app/api/`, route handlers per endpoint, delete `lib/api/*`, in-process handler invocation, remove the `/backend` proxy.
- **`/api/*` defense-in-depth** — the Insecure-Direct-Object-Reference (IDOR) check and the authz audit emit. A real authz audit belongs in zombied (it sees CLI + dashboard + API-key flows); the dashboard layer would only capture a partial, dashboard-only slice.
- **Tenant / per-endpoint rate-limiting** — no near-term need.
- **Cross-Site-Scripting closure** — Content-Security-Policy + Subresource-Integrity; this spec does not stop a compromised page from minting a token via the cookie. Separate spec.
- **CLI carve-out + zombied** — `cli-auth/[session_id]/page.tsx` and the verifier are untouched.
