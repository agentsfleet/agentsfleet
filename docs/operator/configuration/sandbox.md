# Sandbox configuration

## Overview

Every agent execution runs inside a sandbox that enforces resource limits and isolation policies. These settings control how much compute, memory, network access, and filesystem access an agent is allowed.

## Memory limit

| Setting | Variable | Default |
|---------|----------|---------|
| Memory cap | `EXECUTOR_MEMORY_LIMIT_MB` | `512` |

The executor creates a cgroups v2 memory scope for each agent execution. If the agent exceeds the memory limit, the kernel OOM-kills the process. The executor detects OOM events and records them as `UZ-EXEC-009` failures.

Setting this too low causes frequent OOM kills on compilation-heavy workloads. Setting it too high risks one runaway agent starving others on multi-concurrency workers.

## CPU limit

| Setting | Variable | Default |
|---------|----------|---------|
| CPU cap | `EXECUTOR_CPU_LIMIT_PERCENT` | `100` |

CPU is limited to a percentage of one core via cgroups v2 CPU bandwidth control. `100` means one full core. `50` means half a core. The limit prevents a single agent from monopolizing the machine.

## Network policy

Network policy is **hardcoded** in the executor's sandbox layer (`network.zig`). There is no environment variable to configure it. The default policy denies all egress.

Two policies exist in the codebase:

### deny_all

All outbound network access is blocked. The agent cannot reach the internet. This is the default and the most secure option. Suitable for workloads where all dependencies are pre-installed or vendored.

### registry_allowlist

Outbound access is permitted only to a predefined list of package registries:

| Registry | Hosts |
|----------|-------|
| npm | `registry.npmjs.org` |
| PyPI | `pypi.org`, `files.pythonhosted.org` |
| crates.io | `crates.io`, `static.crates.io` |
| Go modules | `proxy.golang.org`, `sum.golang.org` |

All other destinations remain blocked. This mode is for workloads that need to install dependencies during execution.

## Filesystem policy

Filesystem isolation uses Landlock (Linux 5.13+). The policy is applied per-execution and cannot be changed by the agent.

| Path | Access | Purpose |
|------|--------|---------|
| Workspace directory | Read-write | The cloned repo where the agent works. |
| `/usr`, `/lib`, `/bin` | Read-only | System binaries and libraries needed for compilation. |
| `/tmp` (private) | Read-write | Temporary files via `PrivateTmp`. Not shared with other processes. |
| Everything else | Denied | No access to host filesystem, other workspaces, or system config. |

## Kill grace period

| Setting | Variable | Default |
|---------|----------|---------|
| Grace period | `SANDBOX_KILL_GRACE_MS` | `5000` |

When a sandbox exceeds its time limit, the executor sends `SIGTERM` and waits for the grace period before sending `SIGKILL`. This gives the agent time to flush partial results.
