# M3_001: Playbook — Deploy DEV

**Milestone:** M3
**Workstream:** 001
**Updated:** Mar 27, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/founding/01_bootstrap/001_playbook.md`, `playbooks/founding/02_preflight/001_playbook.md`, `playbooks/founding/03_priming_infra/001_playbook.md`

This is the canonical step-by-step DEV deployment runbook.

> **Worker deploy gate.** The `deploy-worker-dev` job in `.github/workflows/deploy-dev.yml`
> stays gated `if: vars.DEV_WORKER_READY == 'true'` and ships nothing until
> `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` completes — that
> playbook provisions the host's `agt_r` runner-token (vault field `runner-token`)
> that the bare-metal `agentsfleet-runner` daemon authenticates with. This
> runbook (Fly.io API + smoke) does **not** depend on the worker; the worker is
> deployed separately once `06_runner_bootstrap_dev` flips `DEV_WORKER_READY=true`.

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

# If no machines — deploy once; CI handles restarts after this.
# NB: the positional path is the BUILD CONTEXT (where config.yml lives). Required
# by flyctl >=0.4.5x, which no longer infers context from --config's directory —
# `--config` from repo root fails the Dockerfile COPY ("/config.yml not found").
flyctl deploy deploy/fly/cloudflared-dev --app cloudflared-dev

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
3. `deploy-fly-dev` — `flyctl deploy --app agentsfleetd-dev --image ghcr.io/agentsfleet/agentsfleetd:dev-latest`
4. `verify-dev` — polls `https://api-dev.agentsfleet.net/healthz` until 200
5. `qa-dev` — Playwright smoke suite against `https://agentsfleet-app.vercel.app`
6. `notify` — Discord

> **HTTP concurrency knobs** live in `deploy/fly/agentsfleetd-dev/fly.toml` under
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
curl -sf https://api-dev.agentsfleet.net/healthz
curl -sf https://api-dev.agentsfleet.net/readyz | jq -e '.ready == true'
```

Operator checks (require the `agentsfleet` CLI — not yet published):

```bash
npx agentsfleet doctor
agentsfleetd doctor --format=json
```

> **Expected-failure rule.** Do NOT skip. Run the checks; if `command -v
> agentsfleet` returns non-zero the step FAILS — and that failure is the signal,
> not noise: it means `@agentsfleet/cli` is still unpublished (a known, tracked
> gap). Leave it red until the CLI ships; the `curl` checks above are the binding
> DEV runtime pass-condition in the meantime.
>
> **Binary placement.** `agentsfleet` is the client CLI (runs on the operator
> machine); `agentsfleetd doctor` queries the server daemon (the deployed
> `agentsfleetd-dev` Fly app), so run it where the daemon lives.

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

Run the full CLI acceptance flow against DEV after the pipeline is green.

> **Expected-failure rule.** Do NOT skip. This gate FAILS while `command -v
> agentsfleet` returns non-zero — surfacing that `@agentsfleet/cli` is still
> unpublished (the known gap tracked by the §6.0 status above and the §7.0 exit
> criterion). It is meant to stay red until the CLI ships; don't paper over it.
>
> **`<ACCEPTANCE_REPO_URL>`** is an operator-supplied input, not a repo constant:
> the clone/HTTPS URL of the throwaway GitHub repository the acceptance run opens
> its PR against (the GitHub App is installed on it during `workspace add`).
> Supply your own; there is no committed default.

```bash
export AGENTSFLEET_API_URL=https://api-dev.agentsfleet.net

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
