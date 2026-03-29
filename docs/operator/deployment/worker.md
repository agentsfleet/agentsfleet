# Worker

## Overview

The worker is started with `zombied worker`. It claims work from Redis Streams, orchestrates the run lifecycle (repo clone, stage execution, gate loop), and pushes branches and opens pull requests on GitHub when runs complete.

## Infrastructure

Workers run on OVHCloud bare-metal machines connected to the control plane via Tailscale. Bare-metal provides the CPU, memory, and disk performance needed for compilation-heavy workloads without the overhead of nested virtualization.

## Directory structure

Each worker machine follows a standard layout:

```
/opt/zombie/
  bin/
    zombied              # Main binary (worker mode)
    zombied-executor     # Executor sidecar binary
  deploy/
    zombied-worker.service       # Systemd unit for worker
    zombied-executor.service     # Systemd unit for executor
    deploy.sh                    # Deployment script
  .env                           # Environment config (from 1Password vault)
```

## Systemd services

The worker runs as a systemd service with a hard dependency on the executor sidecar. The executor must be running before the worker starts.

```ini
# zombied-worker.service
[Unit]
Description=zombied worker
Requires=zombied-executor.service
After=zombied-executor.service

[Service]
Type=simple
EnvironmentFile=/opt/zombie/.env
ExecStart=/opt/zombie/bin/zombied worker
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

## Deployment

The `deploy.sh` script handles binary replacement, service restart, and health verification:

```bash
# On the worker machine
cd /opt/zombie
./deploy/deploy.sh
```

The script:
1. Stops the worker service (waits for in-flight runs to drain).
2. Replaces binaries in `bin/`.
3. Restarts the executor service.
4. Starts the worker service.
5. Verifies health via `zombied doctor worker`.

## Run lifecycle

When a worker claims a run from Redis:

1. Clone the target repository using the GitHub App installation token.
2. Execute each stage by sending JSON-RPC requests to the executor sidecar.
3. Run the gate loop (`lint` -> `test` -> `build`) after each stage.
4. If gates fail, feed errors back to the agent for self-repair (up to 3 retries).
5. Push the implementation branch and open a pull request.
6. Record the scorecard and mark the run as complete.

## Networking

Workers connect to:

| Destination | Network | Purpose |
|-------------|---------|---------|
| PostgreSQL | Tailscale | Run state, workspace config |
| Redis | Tailscale | Work queue, distributed locks |
| GitHub | Public internet | Clone, push, PR creation |
| Executor | Unix socket | Stage execution |

See [Fleet management](/operator/operations/fleet) for drain, rollover, and canary deploy procedures.
