# Handoff — M101_001 dashboard frontend perf

> Ephemeral. Delete before opening the PR (CLAUDE.md: HANDOFF_* must not ship in the diff).

## Scope/Status

Making the dashboard's workspace-scoped pages fast: stop the workspace-list call from blocking every data fetch, stream shells, code-split heavy islands. Spec: `docs/v2/active/M101_001_P1_UI_FRONTEND_PERF_WORKSPACE_AND_SPLITTING.md`. **Frontend-only PR** — backend endpoints (`GET /fleets/{id}`, fleet-status summary) are explicitly out of scope → M101_002. The deterministic "workspace_id always in the session JWT" design is a separate AUTH spec → **M102** (Indy-approved, see spec Discovery).

- ✅ **§1 resolver** — `resolveActiveWorkspaceId(token)` in `ui/packages/app/lib/workspace.ts`: cookie → claim → cached-list, `{id, source}` | null. **0 round-trips on the hint path** (the headline win — no more serial workspace→data chain).
- ✅ **§2 fallback + rewire** — `withWorkspaceScope(token, fn)` re-resolves + retries once on a stale-hint 403/404; returns null (no-workspace empty state) when the list is empty; `orFallback(fallback)` degrades real errors but re-throws workspace rejections so the retry fires. All 11 workspace-scoped routes rewired. List routes use `withWorkspaceScope`; detail routes (`fleets/[id]`, `approvals/[gateId]`) use plain `resolveActiveWorkspaceId`; settings derives the workspace object from the list it already fetches.
- ✅ **§4 billing dedup** — `getTenantBillingCached = cache(getTenantBilling)` in `lib/api/tenant_billing.ts`.
- ⏳ **§3 Suspense streaming** — NOT started. Wrap data regions of `/fleets`, `/events`, `/approvals` in `<Suspense>` + `Skeleton`, move the fetch into async children so `PageHeader` streams first (mirror `app/(dashboard)/page.tsx` StatusTiles/RecentActivity).
- ⏳ **§5 code-split islands** — NOT started. The heaviest island (`@assistant-ui` chat) is ALREADY split via `components/domain/FleetThreadDynamic.tsx` (`next/dynamic`, `ssr:false`). Remaining: wrap click-gated dialogs (Add/Edit credential, Create API key, Create workspace, Add runner), the install flow, ProviderSelector in the same shim pattern.
- ⏳ **§5.3 assistant-ui QA** — NOT started. Verify `FleetThread` is on-brand (design tokens, not raw assistant-ui defaults), reduced-motion-gated, no layout shift on stream, smooth autoscroll. Needs the live authenticated app (`/design-review` + browse on `/fleets/[id]`).
- ⏳ **e2e acceptance** — NOT run. `workspace-fetch-audit` should assert 0 list fetches on a soft nav with a valid `active_workspace_id` cookie. Needs the app running + Clerk auth fixtures (`AGENTSFLEET_E2E_AUDIT=1 bun run test:e2e:acceptance`).

## Working tree

- Clean. 2 commits on `feat/m101-frontend-perf`, **unpushed**:
  - `f68bf814` docs(m101): spec
  - `07cd4fa0` perf(m101): resolver + fallback + rewire + billing dedup + tests
- Worktree: `~/Projects/agentsfleet-m101-frontend-perf` (off `main`). Hydrated (`bun install` done).

## Branch / PR (GitHub)

- Branch: `feat/m101-frontend-perf`. No PR yet (parked before CHORE-close).

## Tests/Checks

- ✅ `bun run test` (full unit) — **1024 passed, 0 failed**.
- ✅ `bun run typecheck` — clean. ✅ `bun run lint` (oxlint + tsc) — clean. ✅ pre-commit HARNESS VERIFY — ALL GATES GREEN.
- ⏳ `bun run build` — NOT run yet this session.
- ⏳ e2e acceptance — NOT run (needs live env).
- New/updated tests: `tests/workspace.test.ts` (resolver A–G + fallback 2.1/2.2/2.4 + orFallback), `tests/helpers/dashboard-mocks.tsx` (derives the resolver split from the legacy `resolveActiveWorkspace` mock — keeps all consuming shards working), `tests/helpers/dashboard-app-mocks.tsx` (getTenantBillingCached), and 5 page-test mocks.

## Next steps (ordered)

1. `bun run build` to confirm the production build + bundle is clean.
2. §3 Suspense streaming on `/fleets`, `/events`, `/approvals` (+ tests: shell renders with Skeleton while data pending).
3. §5 `next/dynamic` shims for the click-gated dialogs/flows (+ test: route initial chunk excludes the dialog module).
4. §5.3 assistant-ui QA on `/fleets/[id]` (design-review + browse, evidence → spec Discovery).
5. e2e acceptance run (workspace-fetch-audit, no-2-calls proof).
6. CHORE-close: mark all Dimensions DONE, move spec → `done/`, changelog `<Update>`, **delete this HANDOFF.md**, push, `gh pr create`, `/review` → `/review-pr` → babysit greptile.

## Risks/gotchas

- **Next 16 cookies are read-only in Server Components** — `withWorkspaceScope` cannot clear a stale cookie mid-render; a stale cookie self-heals only on the next workspace switch (Server Action). Documented in the resolver. M102 (session-claim) eliminates this.
- **Detail routes vs list routes**: detail routes deliberately use the plain resolver (a 404 there means "resource not found", not "stale workspace") — don't blanket-convert them to `withWorkspaceScope`, the ambiguous-404 retry would be wrong.
- **`getFleet` is still an O(100) list-scan** and `StatusTiles` still counts 100 fleets client-side — both are backend gaps owned by M101_002, not this PR.
- Test mocks derive the new resolver from the legacy `resolveActiveWorkspace` mock; if a new test needs the real module, mock it self-contained (don't `importOriginal` — it pulls clerk/next-headers and collides with hoisted `auth`, as seen in `models-credentials-page.test.ts`).
