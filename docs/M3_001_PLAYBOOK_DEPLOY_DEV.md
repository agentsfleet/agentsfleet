# M3_001: Playbook — Deploy DEV

**Milestone:** M3
**Workstream:** 001
**Updated:** Mar 21, 2026
**Owner:** Agent
**Prerequisite:** `docs/M1_001_PLAYBOOK_BOOTSTRAP.md`, `docs/M2_001_PLAYBOOK_PREFLIGHT.md`, `docs/M2_002_PLAYBOOK_PRIMING_INFRA.md`

This is the canonical step-by-step DEV deployment runbook.

---

## 1.0 Preflight Gate

1. Ensure required credentials exist:

```bash
ENV=dev ./scripts/check-credentials.sh
```

2. Ensure branch is clean and validated:

```bash
make lint
make test
```

3. Ensure `deploy-dev.yml` is present and healthy in `main`.

---

## 2.0 Trigger DEV Deploy

1. Merge/push changes to `main`.
2. Confirm GitHub Actions workflow `.github/workflows/deploy-dev.yml` starts.

Expected DEV pipeline order:

1. `check-credentials`
2. `build-dev` — cross-compiles and pushes `dev-latest` to GHCR
3. `deploy-fly-dev` — `flyctl deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest`
4. `verify-dev` — polls `https://api-dev.usezombie.com/healthz` until 200
5. `qa-dev` — Playwright smoke suite against live DEV API
6. `notify` — Discord

### 2.1 Immediate Fix — Fly Crash Loop on Role-Separated DB URL Guard

Symptom in Fly logs:

```text
fatal: DATABASE_URL_API and DATABASE_URL_WORKER must differ (role separation required)
```

Fix:

1. Set different Fly secrets for API vs worker DB URLs on `zombied-dev`:

```bash
fly secrets set \
  DATABASE_URL_API='...' \
  DATABASE_URL_WORKER='...' \
  --app zombied-dev
```

2. Redeploy:

```bash
gh workflow run deploy-dev.yml
```

3. Verify:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

---

## 3.0 Runtime Verification

Run after workflow is green:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

Optional operator checks:

```bash
npx zombiectl doctor
zombied doctor --format=json
```

---

## 4.0 Smoke Gate

DEV smoke must pass from CI (`qa-dev` job).

If smoke fails:

1. Open failing action run logs.
2. Fix issue on branch.
3. Merge to `main` and re-run deploy-dev pipeline.

No release tagging until DEV is green.

---

## 5.0 Evidence Capture

Capture and store:

1. `deploy-dev.yml` run URL
2. `verify-dev` output (`/healthz`, `/readyz`)
3. QA smoke artifact (`qa-dev-<sha>`)
4. Discord notify message link/screenshot

Recommended evidence location:

- `docs/evidence/M3_001_DEV_DEPLOY_<YYYYMMDD>.md`

---

## 6.0 CLI Acceptance Gate

Run the full CLI acceptance flow against DEV after the pipeline is green:

```bash
export ZOMBIE_API_URL=https://dev.api.<domain>

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

Expected outcomes:
- `login` — Clerk auth token stored in local config
- `workspace add` — workspace created, GitHub App installed on acceptance repo
- `specs sync` — spec files uploaded, count confirmed
- `run` — run ID returned; status transitions to `running` then `completed`; PR opened on acceptance repo
- `runs list` — run appears with `status: completed` and `pr_url` present

Spec-to-PR latency must be under 5 minutes. Record the actual time in evidence.

---

## 7.0 Exit Criteria

- DEV pipeline fully green
- `/healthz` and `/readyz` return success
- smoke tests pass
- CLI acceptance run complete (§6.0)
- evidence recorded (see M7_001_DEV_ACCEPTANCE.md §7.0)

When all pass, continue to `docs/M3_002_PLAYBOOK_DEPLOY_PROD.md`.
