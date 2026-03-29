# Credentials

## Overview

UseZombie handles three categories of credentials, each with a different injection path and lifecycle. No credential is ever exposed as an environment variable to the executor process.

## Anthropic API key

The Anthropic API key powers the agent runtime (Claude). It is stored in 1Password and deployed to worker machines via the `.env` file.

| Property | Value |
|----------|-------|
| Storage | 1Password vault |
| Injection | `.env` file on worker machine |
| Scope | Per-worker |
| Rotation | Manual, via 1Password |
| Agent exposure | **None** — the key is never set in the executor environment. It is passed inside the `startStage` JSON-RPC payload so the executor can make API calls on behalf of the agent without leaking the key to agent code. |

The key never appears in logs, error messages, or PR artifacts. The worker strips it from all output before recording results.

## GitHub App installation token

The UseZombie GitHub App is installed on target repositories. The worker requests short-lived installation tokens scoped to the specific repository for each run.

| Property | Value |
|----------|-------|
| Storage | GitHub App private key in 1Password vault |
| Injection | Worker signs a JWT using the private key at runtime |
| Scope | Per-repository, per-run |
| TTL | 1 hour (GitHub default) |
| Auto-refresh | Worker refreshes at 55 minutes if the run is still active |
| Permissions | Contents (read/write), Pull requests (read/write), Metadata (read) |

Token lifecycle:

1. Run is claimed by a worker.
2. Worker signs a JWT using the GitHub App private key.
3. Worker exchanges the JWT for an installation token scoped to the target repo.
4. Token is used for clone, push, and PR creation.
5. If the run exceeds 55 minutes, the worker requests a fresh token.
6. Token expires naturally after 1 hour if not refreshed.

## Package registry credentials

Network policy is hardcoded to `deny_all` in v1. For future workloads that need dependencies, registry access will be handled in two phases:

### Phase 1: Explicit allowlist (current)

Network policy allows outbound connections only to known registry hosts. Public registries (npm, PyPI, crates.io, Go proxy) do not require authentication. Private registries are not supported in Phase 1.

### Phase 2: Internal mirror (planned)

An internal package mirror will cache approved packages. The executor will be configured to use the mirror as its sole registry source. This eliminates direct internet access from the sandbox entirely and enables support for private registries through the mirror's credential store.
