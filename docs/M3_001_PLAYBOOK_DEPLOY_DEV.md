# M3_001: Playbook — Deploy DEV

**Milestone:** M3
**Workstream:** 001
**Updated:** Mar 20, 2026
**Owner:** Agent
**Prerequisite:** `docs/M1_001_PLAYBOOK_BOOTSTRAP.md`, `docs/M2_001_PLAYBOOK_CREDENTIAL_CHECK.md`, `docs/M2_002_PLAYBOOK_PRIMING_INFRA.md`

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
2. `build-dev`
3. `verify-dev`
4. `qa-dev`
5. `notify`

---

## 3.0 Runtime Verification

Run after workflow is green:

```bash
curl -sf https://dev.api.usezombie.com/healthz
curl -sf https://dev.api.usezombie.com/readyz | jq -e '.ready == true'
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

## 6.0 Exit Criteria

- DEV pipeline fully green
- `/healthz` and `/readyz` return success
- smoke tests pass
- evidence recorded

When all pass, continue to `docs/M3_002_PLAYBOOK_DEPLOY_PROD.md`.
