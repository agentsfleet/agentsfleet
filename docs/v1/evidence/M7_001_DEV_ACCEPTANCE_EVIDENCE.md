# M7_001 DEV Acceptance Gate — Evidence

**Date:** Mar 27, 2026
**Branch:** `main` (merged from `m7/001-dev-acceptance-gate` via PR #91, fix PRs #92, #93)
**Commit:** `ccbad03` (final deploy-dev trigger)

---

## 1. Deploy Pipeline (§1.0, §2.0)

**Run:** [23630635008](https://github.com/usezombie/usezombie/actions/runs/23630635008)
**Result:** All jobs green

| Job | Conclusion |
|-----|-----------|
| check-credentials | success |
| build-dev | success |
| deploy-fly-dev | success |
| verify-dev | success |
| qa-dev | success |
| notify | success |

---

## 2. API Health (§3.0)

**Verified:** Mar 27, 2026 — direct curl from local machine

### /healthz

```json
{"status":"ok","service":"zombied","database":"up"}
```

### /readyz

```json
{
  "ready": true,
  "database": true,
  "worker": true,
  "queue_dependency": true,
  "queue_depth": 0,
  "oldest_queued_age_ms": null,
  "queue_depth_breached": false,
  "queue_age_breached": false,
  "queue_depth_limit": null,
  "queue_age_limit_ms": null
}
```

### CI verify-dev log

```
✓ /healthz green after 10s
true  (jq -e '.ready == true')
```

---

## 3. UI Smoke (§4.0)

Vercel deployments all marked "Ready" by Vercel status checks:

| Project | Status | Spec Dimension |
|---------|--------|----------------|
| usezombie-app | Deployed | §4.1, §4.3 |
| usezombie-website | Deployed | §4.2, §4.4 |

Note: `usezombie-agents-sh` also deploys via Vercel but is out of scope for this acceptance gate (no spec dimension defined).

`smoke-post-deploy.yml` runs green for all three projects on every push to main.

---

## 4. Playwright QA Smoke (§5.0)

**qa-dev job:** Passed in run 23630635008
**Tests:** 4 tests across 2 projects (chromium + mobile-chromium)
**Artifact:** `qa-dev-ccbad03...` — [artifact 6136852031](https://github.com/usezombie/usezombie/actions/runs/23630635008/artifacts/6136852031)
**Target:** `https://usezombie-app.vercel.app` with Vercel bypass header

---

## 5. CLI Acceptance (§6.0)

**Status:** BLOCKED

`zombiectl` CLI is not yet built or published:
- No npm package (`npm view zombiectl` returns 404)
- No binary on PATH
- No CLI package in `ui/packages/`
- M4_001 spec exists in `docs/done/` but artifact is not in this repo

This gate cannot be completed until the CLI is available.

---

## 6. Prior PRs in This Session

| PR | Title | Status |
|----|-------|--------|
| #90 | Dual-stack HTTP fix + error hints + tests | Merged to main |
| #91 | Greptile fixes, env var rename, httpz migration spec | Merged to main |
| #92 | fix(ci): qa-dev smoke tests point at Vercel app | Merged to main |
| #93 | fix(ci): use correct Vercel domain + mask bypass secret | Merged to main |

---

## 7. P0 Security Actions (Must Complete Before Gate Close)

- [ ] **Rotate Vercel bypass secret** — `vercel-bypass-app` exposed in CI run 23629344140 logs. Rotate in Vercel dashboard → update 1Password vault → verify `::add-mask::` masks the new value.
- [ ] **Rotate Upstash Redis password** — credential appeared in prior session. Rotate via Upstash dashboard → `fly secrets set` on `zombied-dev` → verify worker reconnects.

## 8. Known Issues (Non-Security)

- **Redis worker WriteFailed** loop on DEV — non-fatal but noisy. Likely Upstash ACL issue.
