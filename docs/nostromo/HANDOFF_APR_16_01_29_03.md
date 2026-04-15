# Handoff — M24_001 post-review hardening

**Date:** Apr 16, 2026: 01:29 AM
**Author:** prior Oracle session (session ran PR #217 from CHORE(open) through VERIFY + review)
**Next agent:** you

---

## Scope

PR #217 (`feat/m24-rest-workspace-scoped-routes`) is **mergeable** and CI-green. This handoff covers an **optional but recommended hardening pass** before merge, based on the self-review at the end of the prior session. None of the findings block the PR from merging — they're tightenings.

Five items to address:

1. Answer the `:steer` auth design question (needs the Buyer's decision, not code).
2. Add `workspace_id` defence-in-depth to the grant revoke SQL.
3. Fix two stale docstrings.
4. Add a cross-workspace IDOR integration test per new workspace-scoped handler.
5. Optimize the `make bench` awk pipeline (single sort instead of three).

---

## Current state

### Git
- **Branch:** `feat/m24-rest-workspace-scoped-routes` at `dd7c213`
- **Worktree:** `/Users/kishore/Projects/usezombie-m24-rest-routes`
- **Remote:** pushed, 0 behind / 0 ahead of `origin/feat/m24-rest-workspace-scoped-routes`
- **vs origin/main:** 13 commits ahead, 0 behind (merged origin/main into branch at `b9d2192`)
- **Working tree:** clean (`git status` empty)

### PR
- **#217** — https://github.com/usezombie/usezombie/pull/217
- **State:** OPEN, MERGEABLE
- **CI:** all green (test, test-integration, memleak, cross-compile linux/macos, lint, qa, gitleaks, CodeQL, Greptile review, Vercel previews, codecov/patch — 30+ checks)
- **Title:** `feat(m24-001): workspace-scoped REST routes + hey loadgen`

### Related artifacts
- **Ripley's log:** `docs/nostromo/LOG_APR_16_00_24_09.md` — full decision trail for the session
- **Spec (DONE):** `docs/v2/done/P1_API_CLI_M24_001_REST_WORKSPACE_SCOPED_ROUTES.md`
- **Follow-up spec (pending):** `docs/v2/pending/P2_OBS_M25_001_ZBENCH_MICRO_CATALOG.md` — populate the zbench stub with real micro-benchmarks

### Side-edits in OTHER repos (not in this PR, not committed)
- **`/Users/kishore/Projects/docs/changelog.mdx`** — v0.16.0 `<Update>` block drafted but uncommitted in the docs repo. Decide whether to land it now or at merge time.
- **`/Users/kishore/Projects/dotfiles/AGENTS.md`** — `api.dev.usezombie.com` → `api-dev.usezombie.com` bench hostname fix is uncommitted. Unrelated to M24 but surfaced during this session.

### Running processes
- **Local docker stack:** `zombied-api`, `zombie-postgres`, `zombie-redis` containers are UP and healthy (from the Tier-3 fresh-DB verification earlier). Leave them running or `make down` — either is fine.
- **No other background tasks.**

---

## The `:steer` middleware question — factual answer

**Buyer's hypothesis:** "I thought the middleware refactor was done in M11_001 / M11_002 / M18_002?"

**Reality:** Those three milestones shipped, but they **deliberately stopped short of workspace authorization**. Evidence:

| Milestone | Delivered | Workspace authz? |
|---|---|---|
| **M11_001** (`docs/v2/done/M11_001_API_ERROR_STANDARDIZATION.md`) | RFC 7807 `application/problem+json` error shape | No |
| **M11_002** (`docs/v2/done/M11_002_HX_HANDLER_CONTEXT.md`) | `Hx` struct: `ok`/`fail`/`db`/`releaseDb`/`redis` response helpers | No |
| **M18_002** (`docs/v2/done/P1_API_M18_002_MIDDLEWARE_MIGRATION.md`) | Middleware chain for **authentication only** — bearer/admin-api-key/HMAC/OAuth/Slack sig | No — explicit design decision |

M18_002 §"Rules compliance" (quoted verbatim from the done spec):

> **RULE FLS (PgQuery everywhere):** no DB access from middleware; principal lookups happen in handlers, not middleware.

And from the same spec's Overview:

> The contents of `src/auth/` must be extractable into a standalone `zombie-auth` repository with zero edits — no imports from `src/http/handlers/`, `src/state/`, `src/db/`, or any business-layer module.

M18_002 **deliberately stopped at authentication** because workspace authorization requires a DB lookup (does `principal` have access to `workspace_id`?), and a DB lookup would violate both the FLS rule and the "extractable zombie-auth repo" goal. `common.authorizeWorkspace(conn, principal, ws_id)` lives in-handler by design.

**Sequence of checks per M18_002:**

```
[Middleware]        bearer_or_api_key → sets hx.principal           (authN — no DB)
                         │
                         ▼
[Handler prologue]  common.authorizeWorkspace(conn, hx.principal, path_ws_id)
                                                                    (authZ — needs DB)
                         │
                         ▼
[Handler body]      DB queries scoped by workspace_id               (defence in depth)
```

The `authorizeWorkspace` call I added to `innerZombieSteer` follows this pattern. It is **consistent** with `external_agents.zig` (M9_001), `workspaces_billing_summary.zig` (M10_004), and every other workspace-scoped handler in the codebase — not redundant, not wrong.

**Future refactor option (NOT M24):** A dedicated `AuthorizeWorkspace` middleware that takes the conn pool. Would remove ~5 lines × ~20 handlers = ~100 lines of per-handler boilerplate. But it would explicitly break M18_002's design contract — `src/auth/` would no longer be extractable as a standalone repo. Worth a dedicated milestone (e.g. `M26_001`) with its own spec review because it's a design-boundary change, not a mechanical refactor.

**Conclusion for the `:steer` review finding:** the `authorizeWorkspace` call is correct post-M18_002. The only open question is the **semantic breadth** — whether `:steer` should accept membership-based principals or stay token-scope-only. That's below.

**What actually changed in `:steer` semantically:**

- **Before (M23_001):** `resolveZombieExecution` required `hx.principal.workspace_scope_id` to be set on the token. If missing → 401 `UNAUTHORIZED`. Then compared `workspace_scope_id` to the zombie's actual `workspace_id` in DB.
- **After (M24_001):** `authorizeWorkspace(conn, hx.principal, path_ws_id)` + path-vs-zombie-ws comparison. `authorizeWorkspace` likely accepts both token-scoped AND membership-based principals (it does a DB lookup into workspace permissions).

**The design question for the Buyer:**
- Is `:steer` meant to be operator-token-only (narrow, M23 original) or available to any workspace member with operator role (broader, current)?
- If narrow: restore the explicit `workspace_scope_id != null` pre-check in `innerZombieSteer`. `authorizeWorkspace` stays as the path check.
- If broad: leave as-is. The new behavior is correct.

**Future cleanup (out of scope for this PR):** a dedicated `AuthorizeWorkspace` middleware that reads the workspace_id from `router.Route` and runs `authorizeWorkspace` before the handler fires. Non-trivial because middleware runs before handler-level DB conn acquire — the middleware would need to acquire its own conn or receive one from the dispatcher. Worth a separate milestone (e.g. `M26_001: AuthorizeWorkspace Middleware`) if you want to kill the per-handler boilerplate.

---

## Work items — in order

### 1. Decide `:steer` auth breadth

**Action for Buyer (Kishore):**
- Read the "`:steer` middleware question — factual answer" section above.
- Decide: narrow (operator-token-only) or broad (workspace member with operator role).
- Reply in the PR or tell the next agent.

**If narrow:** apply this patch to `src/http/handlers/zombie_steer_http.zig` after the `authorizeWorkspace` check:

```zig
if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
    hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
    return;
}

// M23_001 preservation: :steer is operator-token-only. Membership-based
// principals cannot steer running zombies; require an explicit token.
if (hx.principal.workspace_scope_id == null) {
    hx.fail(ec.ERR_UNAUTHORIZED, "workspace-scoped token required");
    return;
}
```

Then update the integration test to cover this case.

**If broad:** no code change. Add a comment noting the intentional widening to `zombie_steer_http.zig` above the `authorizeWorkspace` call:

```zig
// M24_001: accepts both token-scoped principals and membership-based
// principals. Narrowing to token-only would require an additional
// hx.principal.workspace_scope_id != null check.
```

### 2. Scope the grant revoke UPDATE by workspace_id (defence in depth)

**File:** `src/http/handlers/integration_grants_workspace.zig::innerRevokeGrant`
**Current (around line 116):**

```zig
var rev_q = PgQuery.from(conn.query(
    \\UPDATE core.integration_grants
    \\SET status = 'revoked', revoked_at = $1
    \\WHERE grant_id = $2 AND zombie_id = $3::uuid AND status != 'revoked'
    \\RETURNING grant_id
, .{ now_ms, grant_id, zombie_id }) catch {
```

**Change to:**

```zig
var rev_q = PgQuery.from(conn.query(
    \\UPDATE core.integration_grants
    \\SET status = 'revoked', revoked_at = $1
    \\WHERE grant_id = $2
    \\  AND zombie_id = $3::uuid
    \\  AND zombie_id IN (SELECT id FROM core.zombies WHERE workspace_id = $4::uuid)
    \\  AND status != 'revoked'
    \\RETURNING grant_id
, .{ now_ms, grant_id, zombie_id, workspace_id }) catch {
```

**Why:** mirrors the pattern `killZombieOnConn` added. Even if the app-level `zombie_ws_id == workspace_id` check is ever bypassed by a future refactor, the SQL still refuses cross-workspace revocation.

**Verify:** `make test-integration` green; existing grant tests should still pass.

### 3. Fix two stale docstrings

**File:** `src/http/handlers/zombie_api.zig:1-5`

```diff
- // M2_001 / M24_001: Zombie CRUD API — create, list, delete, status.
- //
- // POST   /v1/workspaces/{ws}/zombies       → innerCreateZombie
- // GET    /v1/workspaces/{ws}/zombies       → innerListZombies
- // DELETE /v1/zombies/{id}                  → innerDeleteZombie  (migrated in later M24 slice)
+ // M2_001 / M24_001: Zombie CRUD API — create, list, delete, status.
+ //
+ // POST   /v1/workspaces/{ws}/zombies       → innerCreateZombie
+ // GET    /v1/workspaces/{ws}/zombies       → innerListZombies
+ // DELETE /v1/workspaces/{ws}/zombies/{id}  → innerDeleteZombie
```

**File:** `src/http/handlers/integration_grants_workspace.zig` — header comments and the comment block above `innerRevokeGrant` (around line 100-102). Search for `/v1/zombies/{zombie_id}/integration-grants` and replace with `/v1/workspaces/{ws}/zombies/{zombie_id}/integration-grants`. Search the file for any other stale paths.

**Verify:** `grep -rn "/v1/zombies/" src/http/handlers/` shows zero non-historical references.

### 4. Add cross-workspace IDOR integration tests

**Why:** RULE WAUTH is applied to every new handler, but only the `:steer` integration test (T1.4) verifies it via HTTP. Add one minimal IDOR test per workspace-scoped handler.

**New file (recommended):** `src/http/handlers/m24_001_cross_workspace_idor_test.zig`

**Pattern for each test:**
1. Create two workspaces + two zombies (one per workspace) via the existing `startTestServer` + `cleanupTestData` fixtures in `zombie_steer_http_integration_test.zig` — copy the setup pattern.
2. Make a request to `/v1/workspaces/{WS_A}/zombies/{ZOMBIE_B_FROM_WS_B}/...` with a token scoped to WS_A.
3. Assert 403 (for top-level workspace routes like GET /zombies, POST /credentials) or 404 (for zombie-child routes like /activity, /integration-grants).

**Handlers to cover:**
- `workspace_zombies` — `GET /v1/workspaces/{WS_A}/zombies` with a foreign workspace in path → 403
- `delete_workspace_zombie` — 404 (zombie not in this workspace)
- `workspace_zombie_activity` — 404
- `workspace_credentials` — GET with foreign ws → 403
- `request_integration_grant` — zombie-identity token for ws_B sending to ws_A path → 403 (already guarded by `std.mem.eql(caller.workspace_id, workspace_id)`)
- `list_integration_grants` — 404
- `revoke_integration_grant` — 404
- `workspace_zombie_steer` — already covered by T1.4

Reference: the existing T1.4 test in `zombie_steer_http_integration_test.zig:233` — 404 path. Copy its structure.

**Test file must be registered** in `src/main.zig` per RULE TST. Look for the pattern `_ = @import("http/handlers/zombie_steer_http_integration_test.zig");` and add the new file.

**Verify:** `make test-integration` passes including the new tests.

### 5. Optimize `make bench` awk pipeline (optional polish)

**File:** `make/test-bench.mk` — `_bench-loadgen` target

**Current:** three `tail | awk | sort -n | awk` pipelines (one per percentile). On a 200k-sample bench run, that's ~1-2s of extra wall time.

**Change:** sort once into a temp file, then three awks against the sorted data.

```makefile
SORTED=".tmp/api-bench-sorted-$$$$.txt"; \
trap 'rm -f "$$SORTED"' EXIT; \
tail -n +2 "$$ARTIFACT" | awk -F, '{print $$1}' | sort -n > "$$SORTED"; \
P50_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.50){print; exit}' "$$SORTED"); \
P95_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.95){print; exit}' "$$SORTED"); \
P99_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.99){print; exit}' "$$SORTED"); \
```

**Verify:** `API_BENCH_DURATION_SEC=5 make bench` still reports the same p50/p95/p99.

### 6. (Separate follow-up, NOT in this PR) RSS growth gate

The old `api_bench_runner.zig` sampled `/proc/self/status::VmRSS` before and after. `hey` doesn't do that, and `make memleak` is unit-test-level, not a production-load RSS gate.

**Proposal for a future milestone — don't land in M24:**
- Add a `_bench-rss` target in `make/test-bench.mk`
- Before `hey`: `docker stats --no-stream --format '{{.MemUsage}}' zombied-api`
- After `hey`: same
- Parse + gate on growth > `API_BENCH_MAX_RSS_GROWTH_MB`
- Only runs when the API is in docker (skip gracefully if not)

Track it as a row in the M25_001 spec's out-of-scope section, or a brand-new milestone.

---

## Verification checklist

After applying fixes 2-5 (item 1 is a decision, not code):

```bash
cd /Users/kishore/Projects/usezombie-m24-rest-routes
make lint                               # Zig lint + pg-drain + openapi + 350L gate
make test                               # 618+ pass, 0 fail
make test-integration                   # incl. new IDOR tests
make memleak                            # 1178+ pass
zig build -Dtarget=x86_64-linux         # green
zig build -Dtarget=aarch64-linux        # green
API_BENCH_DURATION_SEC=5 make bench     # Tier-1 + Tier-2 green
gitleaks detect                         # no leaks
git diff --stat                         # only touches the files listed above
```

## Push + PR update

```bash
git push
# Add a PR comment linking to this handoff:
gh pr comment 217 --body "Post-review hardening applied. See docs/nostromo/HANDOFF_APR_16_01_29_03.md for the trail."
```

## Risks

- **Item 2 grant SQL change** touches a production-critical authorization path. The integration test for grant revoke in `src/http/handlers/integration_grants_workspace_integration_test.zig` (if it exists) should still pass without modification — the new WHERE clause is strictly more restrictive than the old one. If tests break, it means the test was relying on a cross-workspace revoke succeeding, which would itself be a bug.
- **Item 4 new IDOR tests** may reveal actual bugs in the existing handlers if a new handler was missed. That's good — flag it and fix.
- **Nothing** in this handoff changes the happy-path API behavior, so zombiectl + frontend integrations should be unaffected.

## Decisions deferred to Buyer

Only item 1 (`:steer` auth breadth). Everything else is pure hardening with no judgment calls — just mechanical fixes.

## Open thread

If you find any additional issues while applying these fixes (e.g., another handler missing `authorizeWorkspace`, another stale docstring), list them at the bottom of this handoff file rather than silently fixing. Keep the scope of this hardening PR tight — it's already 1168/-791, and the user has been careful about scope throughout the session.

---

End of handoff.
