# Runtime Configuration Contract

Date: Mar 22, 2026
Status: Active

## Goal

Define the minimal runtime contract for API, worker, and `sandbox-executor` without stale or duplicate configuration stories.

## Principles

1. Security-critical values are env-only.
2. Non-secret operator ergonomics may use narrow CLI overrides.
3. The worker and executor must agree on one execution contract.
4. If sandbox prerequisites are missing, startup fails closed.

## Required Runtime Roles

### API

- auth configuration
- DB/Redis API role URLs
- GitHub App configuration

### Worker

- DB/Redis worker role URLs
- queue/retry/time-budget settings
- executor connection settings

### Sandbox Executor

- sandbox backend selection
- resource-cap settings
- network allowlist settings
- kill/lease timing

## Execution Configuration

### Required or core keys

| Key | Scope | Notes |
|---|---|---|
| `SANDBOX_BACKEND` | worker + executor | `host`, `bubblewrap`, later backend values must be explicit |
| `SANDBOX_KILL_GRACE_MS` | executor | grace before forced kill |
| `RUN_TIMEOUT_MS` | worker | stage/run time budget |
| `WORKER_CONCURRENCY` | worker | worker claim concurrency |

### Resource governance keys

| Key | Scope | Notes |
|---|---|---|
| `SANDBOX_MEMORY_MB` | executor | hard memory ceiling |
| `SANDBOX_CPU_QUOTA` | executor | cpu cap |
| `SANDBOX_DISK_WRITE_MB` | executor | disk write budget |
| `SANDBOX_NETWORK_ALLOWLIST` | executor | explicit egress allowlist |

### Connection keys

| Key | Scope | Notes |
|---|---|---|
| `SANDBOX_EXECUTOR_ADDR` | worker | local Unix socket or loopback address |
| `SANDBOX_EXECUTOR_STARTUP_TIMEOUT_MS` | worker | fail if executor cannot be reached |
| `SANDBOX_EXECUTOR_LEASE_TIMEOUT_MS` | worker + executor | liveness/lease timeout |

## Non-goals

This document does not try to duplicate every auth, storage, and notification variable in the repo. It exists to keep the execution-runtime contract clear and current.

If broader env validation rules change, update:
- this file for execution/runtime behavior
- startup validation code
- any `.env.example` or deploy docs that expose the same knobs

