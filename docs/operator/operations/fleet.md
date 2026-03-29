# Fleet management

## Overview

Worker fleet management covers how to safely upgrade, scale, and maintain the pool of worker machines. The key constraint is that workers may have in-flight runs that must complete before the worker is shut down.

## Drain

Before upgrading a worker, drain it so no new work is claimed and in-flight runs complete gracefully.

```bash
# Signal the worker to stop claiming new runs
systemctl kill -s SIGUSR1 zombied-worker

# Wait for in-flight runs to finish (check via logs or metrics)
journalctl -u zombied-worker -f
# Look for: "drain complete, 0 active runs"

# Stop the worker
systemctl stop zombied-worker
```

The worker responds to `SIGUSR1` by entering drain mode:
- Stops claiming new runs from Redis.
- Continues executing in-flight runs to completion.
- Exits cleanly when all runs finish.

If a run is stuck, the `RUN_TIMEOUT_MS` limit will eventually force it to terminate.

## Rolling restart

To upgrade the entire fleet without downtime:

1. Drain worker A (send `SIGUSR1`, wait for active runs to finish).
2. Stop worker A (`systemctl stop zombied-worker`).
3. Deploy new binaries to worker A.
4. Restart executor on worker A (`systemctl restart zombied-executor`).
5. Start worker A (`systemctl start zombied-worker`).
6. Verify worker A is healthy (`zombied doctor worker`).
7. Repeat for worker B, C, etc.

Always restart the executor **before** the worker. The worker has a hard dependency on the executor (`Requires=zombied-executor.service`).

## Canary deploy

For high-risk upgrades, deploy to a single worker first and verify before rolling out to the fleet.

**Step 1 — Deploy canary**

Pick one worker and deploy the new version:

```bash
# On the canary worker
cd /opt/zombie
./deploy/deploy.sh
```

**Step 2 — Verify canary**

Run diagnostics and monitor the first few runs:

```bash
zombied doctor worker
```

Watch for:
- All doctor checks pass.
- Runs complete successfully (check `sessions_created_total` and `failures_total` metrics).
- No new error codes in logs.
- Executor sandbox enforcement is working (check `oom_kills_total`, `landlock_denials_total`).

Let the canary process at least 3-5 runs before proceeding.

**Step 3 — Roll out**

If the canary is healthy, proceed with rolling restart across the remaining workers.

## Systemd ordering

The executor must always start before the worker. This is enforced by the systemd unit dependency:

```
zombied-worker.service
  Requires=zombied-executor.service
  After=zombied-executor.service
```

If the executor crashes, systemd will also stop the worker. When the executor restarts, the worker must be manually started (or configure `PartOf=` for automatic restart propagation).

## Scaling

To add a new worker to the fleet:

1. Provision a bare-metal machine on OVHCloud.
2. Join it to the Tailscale network.
3. Deploy the standard directory structure to `/opt/zombie/`.
4. Configure `.env` from the 1Password vault.
5. Enable and start both systemd services.
6. Verify with `zombied doctor worker`.

The new worker will automatically start claiming runs from Redis. No registration step is needed — Redis consumer groups handle worker discovery.
