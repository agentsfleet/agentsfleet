# Error codes

## Overview

UseZombie uses structured error codes prefixed with `UZ-` to classify failures. Every failed run or stage includes an error code in the API response, logs, and metrics. Use this reference to diagnose and remediate failures.

## Execution errors

Errors originating from the executor or agent runtime.

| Code | Meaning | Remediation |
|------|---------|-------------|
| `UZ-EXEC-001` | Stage payload invalid | Check the spec for syntax errors. Re-validate with `zombiectl spec validate`. |
| `UZ-EXEC-002` | Repository clone failed | Verify the GitHub App is installed on the target repo. Check network connectivity from the worker. |
| `UZ-EXEC-003` | Executor unavailable | The executor sidecar is down or unresponsive. Check `systemctl status zombied-executor` and restart if needed. |
| `UZ-EXEC-004` | Agent runtime crash | The agent process crashed unexpectedly. Check executor logs for panic traces. Retry the run. |
| `UZ-EXEC-005` | Gate loop exhausted | All gate retries failed. The agent could not self-repair the lint/test/build failures. Review the gate logs in the run output. |
| `UZ-EXEC-006` | Branch push failed | Failed to push the implementation branch to GitHub. Verify the GitHub App has `contents:write` permission on the repo. |
| `UZ-EXEC-007` | PR creation failed | Failed to open a pull request. Verify the GitHub App has `pull_requests:write` permission. Check for branch protection rules that may block. |
| `UZ-EXEC-008` | Stage timeout | The stage exceeded `RUN_TIMEOUT_MS`. Increase the timeout or simplify the spec. |
| `UZ-EXEC-009` | OOM killed | The agent exceeded `EXECUTOR_MEMORY_LIMIT_MB`. Increase the memory limit or reduce compilation parallelism. |
| `UZ-EXEC-010` | Network policy violation | The agent attempted a network connection that was blocked by policy. If the agent needs registry access, set `EXECUTOR_NETWORK_POLICY=registry_allowlist`. |
| `UZ-EXEC-011` | Filesystem policy violation | The agent attempted to access a path denied by Landlock. This usually indicates the agent tried to write outside the workspace. |
| `UZ-EXEC-012` | Spec validation failed | The spec failed server-side validation. Check for disallowed file paths or unsupported constructs. |
| `UZ-EXEC-013` | Workspace suspended | The workspace is paused or has exceeded its credit budget. Resume the workspace or add credits. |
| `UZ-EXEC-014` | Lease expired | The run's lease expired before completion. This usually means the worker crashed and the reconciler cleaned up the stale run. Retry the run. |

## Credential errors

Errors related to credential retrieval or validation.

| Code | Meaning | Remediation |
|------|---------|-------------|
| `UZ-CRED-001` | GitHub App token request failed | The JWT signature was rejected by GitHub. Verify the GitHub App private key in 1Password matches the installed app. Re-download the key from GitHub App settings if needed. |
| `UZ-CRED-002` | Anthropic API key invalid | The API key was rejected by Anthropic. Verify the key in 1Password is current. Rotate it at console.anthropic.com if needed. |

## Sandbox errors

Errors related to sandbox setup or enforcement.

| Code | Meaning | Remediation |
|------|---------|-------------|
| `UZ-SANDBOX-001` | Sandbox initialization failed | The executor could not set up the sandbox (Landlock, cgroups, or network namespace). Check kernel support with `zombied doctor worker`. On older kernels, upgrade or use `SANDBOX_BACKEND=host` for dev only. |
