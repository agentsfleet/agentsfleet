# Executor sidecar

## Overview

`zombied-executor` runs as a separate systemd service alongside the worker. It is the only component that executes untrusted agent code. The executor receives stage payloads from the worker over a Unix socket, applies sandbox policies, runs the agent, and returns execution results.

## Communication

The executor listens on a Unix socket at `/run/zombie/executor.sock`. The worker sends JSON-RPC requests over this socket. There is no network listener — the executor is never reachable from the network.

```
Worker --[JSON-RPC]--> /run/zombie/executor.sock --> Executor
```

### RPC methods

| Method | Purpose |
|--------|---------|
| `startStage` | Begin executing an agent stage with the provided payload |
| `cancelStage` | Cancel a running stage execution |
| `healthCheck` | Verify the executor is ready to accept work |

## Agent runtime

The executor embeds the **NullClaw** agent runtime. NullClaw is the internal agent execution engine that interprets stage payloads and drives the agent through implementation, file edits, and command execution within the sandbox boundary.

## Sandbox policies

Every agent execution runs under four isolation layers:

| Layer | Mechanism | Effect |
|-------|-----------|--------|
| Filesystem | Landlock | Workspace directory is read-write. System paths are read-only. Everything else is denied. |
| Memory and CPU | cgroups v2 | Memory capped at 512 MB (default). CPU limited to one core. OOM kills are detected and recorded. |
| Network | Network namespace deny | All outbound network access is denied by default. Optional allowlist for package registries. |
| Process | Systemd hardening | `PrivateTmp`, `ProtectSystem`, `NoNewPrivileges` restrict the process environment. |

See [Sandbox enforcement](/operator/security/sandbox) for full details on each layer.

## Systemd service

```ini
# zombied-executor.service
[Unit]
Description=zombied executor sidecar

[Service]
Type=simple
EnvironmentFile=/opt/zombie/.env
ExecStart=/opt/zombie/bin/zombied-executor
Restart=on-failure
RestartSec=3

# Hardening
PrivateTmp=true
ProtectSystem=strict
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

## macOS development mode

On macOS, the executor falls back to in-process execution without sandboxing. Landlock, cgroups, and network namespaces are Linux-only kernel features. This mode is for local development only and must never be used in production.

```bash
# Dev mode — no sandbox enforcement
zombied-executor  # Detects macOS, logs warning, runs without sandbox
```

## Failure handling

If the executor process crashes or becomes unresponsive, the worker detects the failure through socket health checks. The systemd `Restart=on-failure` directive restarts the executor automatically. Any in-flight stage execution is marked as failed with error code `UZ-EXEC-003`.
