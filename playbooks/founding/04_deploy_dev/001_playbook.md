# M3_001: Playbook — Deploy DEV

**Milestone:** M3
**Workstream:** 001
**Updated:** Mar 27, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/founding/01_bootstrap/001_playbook.md`, `playbooks/founding/02_preflight/001_playbook.md`, `playbooks/founding/03_priming_infra/001_playbook.md`

This is the canonical step-by-step DEV deployment runbook.

---

## 1.0 Preflight Gate

**Status:** ✅ DONE

1. Ensure required credentials exist:

```bash
ENV=dev ./playbooks/founding/02_preflight/00_gate.sh
```

2. Ensure branch is clean and validated:

```bash
make lint-all
make test-unit-all
```

3. Ensure `deploy-dev.yml` is present and healthy in `main`.

4. Ensure `cloudflared-dev` Fly app is deployed (one-time prerequisite):

```bash
# Check if machines exist
flyctl machine list --app cloudflared-dev

# If no machines — deploy once; CI handles restarts after this
flyctl deploy --app cloudflared-dev --config deploy/fly/cloudflared-dev/fly.toml

# Verify TUNNEL_TOKEN secret is set
flyctl secrets list --app cloudflared-dev | grep TUNNEL_TOKEN
```

> After the first deploy, CI's `verify-dev` job automatically restarts or redeploys `cloudflared-dev` if machines are down. No manual intervention needed on subsequent runs.

---

## 2.0 Trigger DEV Deploy

**Status:** ✅ DONE

1. Merge/push changes to `main`.
2. Confirm GitHub Actions workflow `.github/workflows/deploy-dev.yml` starts.

Expected DEV pipeline order:

1. `check-credentials`
2. `build-dev` — cross-compiles and pushes `dev-latest` to GHCR
3. `deploy-fly-dev` — `flyctl deploy --app agentsfleetd-dev --image ghcr.io/usezombie/agentsfleetd:dev-latest`
4. `verify-dev` — polls `https://api-dev.usezombie.com/healthz` until 200
5. `qa-dev` — Playwright smoke suite against `https://usezombie-app.vercel.app`
6. `notify` — Discord

> **HTTP concurrency knobs** live in `deploy/fly/zombied-dev/fly.toml` under
> `[env]` (`API_HTTP_THREADS = "32"` — matched to prod so dev surfaces pool
> saturation first — and `API_HTTP_WORKERS = "1"` on this 512mb box).
> `API_HTTP_THREADS` is the per-worker handler-pool size; the one long-lived
> handler that holds a thread for the connection's life is the SSE stream (the
> runner lease is a non-blocking single poll). The default of `1` lets a single
> SSE stream saturate the pool. See `deploy/fly/agentsfleetd-prod/fly.toml` for the
> full rationale. To change: edit the `[env]` block, redeploy, watch
> handler-pool saturation on `/metrics`.

---

## 3.0 Runtime Verification

**Status:** ✅ DONE

Run after workflow is green:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

Optional operator checks (requires `agentsfleet` CLI — not yet available):

```bash
npx agentsfleet doctor
agentsfleetd doctor --format=json
```

---

## 4.0 Smoke Gate

**Status:** ✅ DONE

DEV smoke must pass from CI (`qa-dev` job).

If smoke fails:

1. Open failing action run logs.
2. Fix issue on branch.
3. Merge to `main` and re-run deploy-dev pipeline.

No release tagging until DEV is green.

---

## 5.0 Evidence Capture

**Status:** ✅ DONE (CI evidence; CLI evidence blocked on `agentsfleet`)

Captured:

1. `deploy-dev.yml` run 23630635008 — all green
2. `verify-dev` output: `/healthz` 200, `/readyz` `ready:true`
3. QA smoke artifact: `qa-dev-ccbad03...` (artifact ID 6136852031)
4. Discord notify: success embed sent

Evidence location:

- `docs/evidence/M3_001_DEV_DEPLOY_<YYYYMMDD>.md`

---

## 6.0 CLI Acceptance Gate

**Status:** PENDING — blocked: `agentsfleet` CLI not yet built/published

Run the full CLI acceptance flow against DEV after the pipeline is green:

```bash
export ZOMBIE_API_URL=https://api-dev.usezombie.com

npx agentsfleet login
npx agentsfleet workspace add <ACCEPTANCE_REPO_URL>
npx agentsfleet specs sync docs/spec/
npx agentsfleet run
npx agentsfleet runs list
```

Expected outcomes:
- `login` — Clerk auth token stored in local config
- `workspace add` — workspace created, GitHub App installed on acceptance repo
- `specs sync` — spec files uploaded, count confirmed
- `run` — run ID returned; status transitions to `running` then `completed`; PR opened on acceptance repo
- `runs list` — run appears with `status: completed` and `pr_url` present

---

## 7.0 Exit Criteria

- ✅ DEV pipeline fully green
- ✅ `/healthz` and `/readyz` return success
- ✅ smoke tests pass
- ⏳ CLI acceptance run complete (§6.0) — **blocked on `agentsfleet`**
- ✅ evidence recorded (see M7_001_DEV_ACCEPTANCE.md §7.0)

When all pass, continue to `playbooks/founding/05_deploy_prod/001_playbook.md`.
