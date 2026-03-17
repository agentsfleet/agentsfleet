# M6_005: GitHub CI and Release Pipeline

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 005
**Date:** Mar 08, 2026
**Updated:** Mar 15, 2026
**Status:** DONE
**Priority:** P0 — release and quality gate
**Depends on:** M4_001 (Implement `zombiectl` CLI Runtime), M6_003 (Zig API Memleak and Perf Gates), M6_006 (V1 Acceptance E2E Gate)

---

## Components

| Component | Language / Runtime | CI flag |
|---|---|---|
| `zombied` | Zig binary | zombied |
| `zombiectl` | Node.js CLI / npm package | zombiectl |
| `website` | Vite + React | website |
| `app` | Next.js | app |

---

## Required Secrets

All secrets set in GitHub repo `usezombie/usezombie` → Settings → Secrets.

| Secret | Source | Used in |
|---|---|---|
| `GITHUB_TOKEN` | Auto-injected | release, containers |
| `GITLEAKS_LICENSE` | gitleaks.io | `gitleaks.yml` |
| `CODECOV_TOKEN` | codecov.io repo settings | `test-unit.yml` |
| `OP_SERVICE_ACCOUNT_TOKEN` | 1Password SA `usezombie-ci` | `release.yml` (npm + deploy stubs) |
| `VERCEL_ORG_ID` | Vercel team settings | future explicit deploy jobs |
| `VERCEL_PROJECT_ID_WEBSITE` | Vercel project settings | future explicit deploy jobs |
| `VERCEL_PROJECT_ID_AGENTS` | Vercel project settings | future explicit deploy jobs |
| `VERCEL_PROJECT_ID_APP` | Vercel project settings | future explicit deploy jobs |

### 1Password — Service Account

Name: `usezombie-ci`
Access: `ZMB_CD_DEV` + `ZMB_CD_PROD`

**Vault: `ZMB_CD_DEV`**

| Item | Fields | Used for |
|---|---|---|
| `zombied-dev-server` | `hostname`, `ssh-private-key`, `deploy-user` | dev SSH deploy (stub until server provisioned) |

**Vault: `ZMB_CD_PROD`**

| Item | Fields | Used for |
|---|---|---|
| `zombied-prod-server-1` | `hostname`, `ssh-private-key`, `deploy-user` | prod blue-green deploy |
| `zombied-prod-server-2` | `hostname`, `ssh-private-key`, `deploy-user` | prod blue-green second node |
| `npm-publish-token` | `credential` | zombiectl npm publish |
| `vercel-bypass-website` | `credential` | Vercel preview bypass for `usezombie-website` (smoke CI) |
| `vercel-bypass-agents` | `credential` | Vercel preview bypass for `usezombie-agents-sh` (smoke CI) |
| `vercel-bypass-app` | `credential` | Vercel preview bypass for `usezombie-app` (smoke CI) |
| `cloudflare-api-token` | `credential` | Cloudflare API token (Zone:Edit + Zone:Read) |

---

## Vercel Projects

Three projects connected to `usezombie/usezombie` via Vercel GitHub integration.
Vercel handles all deploys automatically — no explicit CI deploy step required.

| Project | Root directory | Production domain | Env var |
|---|---|---|---|
| `usezombie-website` | `ui/packages/website` | `usezombie.com` | `SITE_VARIANT=humans` |
| `usezombie-agents-sh` | `ui/packages/website` | `usezombie.sh` | `SITE_VARIANT=agents` |
| `usezombie-app` | `ui/packages/app` | `app.usezombie.com` | — |

Preview deploys happen automatically on PR. `smoke-post-deploy.yml` fires on `deployment_status` per project and routes to the correct Playwright smoke suite.

### Vercel Environment Variables (scoped per environment)

Configure in each project → Settings → Environment Variables.

**`usezombie-app`**

| Variable | Preview | Production |
|---|---|---|
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Clerk DEV publishable key | Clerk PROD publishable key |
| `CLERK_SECRET_KEY` | Clerk DEV secret key | Clerk PROD secret key |
| `NEXT_PUBLIC_API_URL` | `https://api.dev.usezombie.com` | `https://api.usezombie.com` |

**`usezombie-agents-sh`** and **`usezombie-website`**

| Variable | Preview | Production |
|---|---|---|
| `VITE_APP_BASE_URL` | `https://app.dev.usezombie.com` | `https://app.usezombie.com` |

> Without explicit Preview scoping, Vite builds with `import.meta.env.PROD=true` and falls back to the production URL. Explicit scoping ensures preview smoke tests hit the dev API/app.

### Vercel Bypass Secrets

Enable in each project → Settings → Deployment Protection → Protection Bypass for Automation.
Copy the generated token → store in `ZMB_CD_PROD` vault (see table above).
`smoke-post-deploy.yml` loads them from 1Password and injects `x-vercel-protection-bypass` header automatically.

---

## 1.0 Pull Request CI Contract

**Status:** DONE

On every PR, the following workflows run in parallel:

| Workflow | Scope | Make target / command | Blocks merge |
|---|---|---|---|
| `gitleaks.yml` | all files | gitleaks action | yes |
| `lint.yml` | all components | `make lint` | yes |
| `test-unit.yml` | all components | per-component (see below) | yes |
| `memleak.yml` | zombied | `make memleak` (valgrind container) | yes |
| `qa-smoke.yml` | zombied + website + app | `make qa-smoke` | yes |
| `cross-compile.yml` | zombied (path-gated) | `zig build` matrix (4 targets) | yes |
| Vercel (automatic) | website + agents-site + app | Vercel GitHub integration | informational |
| `smoke-post-deploy.yml` | website or app | Playwright smoke vs preview URL | informational |

**`test-unit.yml` per-component breakdown:**

| Component | Command | Coverage upload |
|---|---|---|
| zombied | `make test-zombied` (unit + integration + depth gate) | — (kcov not available in CI) |
| zombiectl | `make test-unit-zombiectl` | — |
| website | `bun run test:coverage` in `ui/packages/website` | Codecov flag: `website`, file: `coverage/lcov.info` |
| app | `bun run test:coverage` in `ui/packages/app` | Codecov flag: `apps`, file: `coverage/lcov.info` |

**Dimensions:**
- 1.1 DONE zombied: lint-zig, test-zombied (unit+integration), memleak, cross-compile (4 targets, path-gated)
- 1.2 DONE website: lint-website, test-unit-website + Codecov upload (flag: `website`), qa-smoke e2e
- 1.3 DONE app+zombiectl: lint-apps, test-unit-app + Codecov upload (flag: `apps`), test-unit-zombiectl, qa-smoke e2e
- 1.4 DONE Codecov slug: `usezombie/usezombie`, component flags: `website`, `apps` — zombied coverage removed (kcov not available on ubuntu-latest)

---

## 2.0 Tag Release Contract

**Status:** DONE

Trigger: `git tag vX.Y.Z && git push --tags`

Job DAG:
```
verify-tag
├── binaries (4-target matrix) → create-release (GitHub Release + .tar.gz assets)
├── docker → ghcr.io/usezombie/zombied push
├── npm → zombiectl publish to npmjs.org
└── deploy-dev (stub) → deploy-prod (stub)
```

**Dimensions:**
- 2.1 DONE `verify-tag` asserts `git tag == cat VERSION` before all downstream jobs
- 2.2 DONE `binaries` cross-compiles zombied for x86_64-linux, aarch64-linux, x86_64-macos, aarch64-macos; attached to GitHub Release
- 2.3 DONE `npm` publishes zombiectl from `zombiectl/` directory; `NPM_TOKEN` fetched from `ZMB_CD_PROD/npm-publish-token/credential` via 1Password
- 2.4 DONE `docker` builds multi-arch image and pushes `latest` + versioned tags to GHCR

---

## 3.0 Deploy Pipeline

**Status:** STUBBED — passes with `echo`, unblocks release DAG

### On tag `vX.Y.Z`

```
create-release + docker
        ↓
    deploy-dev   ← stub: ZMB_CD_DEV/zombied-dev-server
        ↓
    deploy-prod  ← stub: ZMB_CD_PROD/zombied-prod-server-*
      ├── blue-green deploy
      └── worker deploy
```

### When ready to unwire stubs

Replace echo steps with:

```yaml
- uses: 1password/load-secrets-action@v2
  with:
    export-env: true
  env:
    OP_SERVICE_ACCOUNT_TOKEN: ${{ secrets.OP_SERVICE_ACCOUNT_TOKEN }}
    DEPLOY_HOST: op://ZMB_CD_PROD/zombied-prod-server-1/hostname
    DEPLOY_USER: op://ZMB_CD_PROD/zombied-prod-server-1/deploy-user
    DEPLOY_KEY:  op://ZMB_CD_PROD/zombied-prod-server-1/ssh-private-key
- name: Deploy via SSH
  run: |
    echo "$DEPLOY_KEY" > /tmp/deploy_key && chmod 600 /tmp/deploy_key
    ssh -i /tmp/deploy_key -o StrictHostKeyChecking=no \
      $DEPLOY_USER@$DEPLOY_HOST "bash /opt/zombied/deploy.sh ${{ github.ref_name }}"
```

---

## 4.0 Verification Units

**Dimensions:**
- 3.1 DONE CI contract: PR triggers gitleaks, lint, test-unit, memleak, qa-smoke, cross-compile
- 3.2 DONE Coverage: Codecov uploads target `usezombie/usezombie` with flags `website` and `apps`
- 3.3 DONE Release contract test: verify tag `vX.Y.Z` produces release notes, binary assets, npm publish, container push
- 3.4 PENDING Operator evidence pack: CI run link, release run link, published artifact URLs

---

## 5.0 Acceptance Criteria

- [x] 4.1 PR merges blocked unless all required CI jobs pass — enforce via GitHub branch protection (see below)
- [x] 4.2 Release tag flow is human/agent triggerable: `git tag vX.Y.Z && git push --tags`
- [x] 4.3 zombiectl npm publish is part of release workflow and validated post-publish
- [x] 4.4 GitHub Release contains cross-compiled zombied binaries for 4 targets
- [x] 4.5 Coverage reporting attributed to `usezombie/usezombie` for `website` and `apps` components

### Branch protection — required status checks for `main`

Configure in GitHub → Settings → Branches → `main` → Require status checks:

```
gitleaks / gitleaks
lint / lint
test-unit / test-unit
memleak / memleak
qa-smoke / qa-smoke
cross-compile / cross-compile (x86_64-linux)
cross-compile / cross-compile (aarch64-linux)
cross-compile / cross-compile (x86_64-macos)
cross-compile / cross-compile (aarch64-macos)
```

---

## 6.0 Out of Scope

- Migrating away from GitHub Actions
- Replacing Codecov with another coverage provider
- Non-GitHub package/release registries beyond npm and GHCR
- zombied kcov/line coverage in CI — not available on ubuntu-latest; revisit if self-hosted runner is added
