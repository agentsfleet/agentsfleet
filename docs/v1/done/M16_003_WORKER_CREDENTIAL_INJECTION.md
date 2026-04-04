# M16_003: Worker Credential Injection for Agent Execution

**Prototype:** v1.0.0
**Milestone:** M16
**Workstream:** 003
**Date:** Mar 28, 2026
**Status:** PENDING
**Priority:** P0 — BLOCKER for M16_001 (gate loop cannot run without credentials)
**Batch:** B1 — must complete before M16_001 gate loop execution
**Depends on:** M7_001 (DEV Acceptance — deploy pipeline must be green)

---

## 1.0 Anthropic API Key Injection

**Status:** PENDING

The bare-metal worker needs the Anthropic API key to pass a valid NullClaw config to the executor sidecar at run time. The key must not be injected as an OS-level environment variable into the executor process — the executor is sandboxed and must receive agent config only through the `StartStage` RPC payload. The worker reads the key from its own `.env` file (populated at deploy time via `op://` reference) and forwards it inside the RPC payload when opening each execution session.

The existing `.env` → `/etc/default/zombied-worker` deploy pattern (used today for `DATABASE_URL` and `REDIS_URL`) is extended with one new entry. No new deploy mechanism is introduced.

**Dimensions:**
- 1.1 PENDING Add `ANTHROPIC_API_KEY` to vault item `anthropic-dev` in `ZMB_CD_DEV` vault; reference as `op://ZMB_CD_DEV/anthropic-dev/credential` in worker `.env` template
- 1.2 PENDING Worker reads `ANTHROPIC_API_KEY` from its own environment at startup and validates non-empty; log `UZ-CRED-001` and halt startup if missing
- 1.3 PENDING Worker passes the key inside `StartStage` RPC payload (`agent_config.api_key`) — not via executor process environment — so the sandboxed executor never inherits it through `environ`
- 1.4 PENDING Executor `runner.zig` reads `api_key` from RPC-supplied `agent_config` overrides, not from `std.posix.getenv("ANTHROPIC_API_KEY")`; existing `Config.load()` RPC-override path is the insertion point

---

## 2.0 GitHub App Installation Token Flow

**Status:** PENDING

The worker already holds the GitHub App private key in vault (used elsewhere for App authentication). For each run that reaches the PR-creation stage, the worker requests a short-lived installation token scoped to the target repository. This token is per-run and passed to the executor for branch push and PR creation. The token is never stored in Postgres; it exists only in worker memory for the lifetime of one run.

If a token is issued near the end of its 1-hour TTL and the run is still in progress when it expires, the worker refreshes it before the PR-creation stage begins. Mid-stage refresh is not supported — the run must reach PR creation within one token lifetime.

**Dimensions:**
- 2.1 PENDING Worker calls `POST /app/installations/{installation_id}/access_tokens` with repo scope before the first stage that requires git access; signs the JWT using the GitHub App private key read from `op://ZMB_CD_DEV/github-app/private_key`
- 2.2 PENDING Token is held in worker memory, attached to the run context struct; never written to Postgres or logged
- 2.3 PENDING Before PR-creation stage, worker checks token age: if `issued_at + 55_minutes < now()`, re-request a fresh token using the same App JWT flow
- 2.4 PENDING If token request fails (HTTP 4xx/5xx from GitHub), classify as `FailureClass.policy_deny` (error code `UZ-CRED-002`) and transition the run to `BLOCKED`; do not retry the token request in a tight loop

---

## 3.0 Package Registry Network Allowlist

**Status:** PENDING

The executor's current network policy is `deny_all` via bubblewrap `--unshare-net`. Agent execution stages that install dependencies (npm, pip, cargo, go get) fail silently because all egress is blocked. Phase 1 extends the network policy to a static allowlist of known public package registries. The allowlist is baked into the executor config — it is not per-run and not workspace-configurable. Phase 2 will replace public registry access with an internal mirror (out of scope here).

The allowlist is additive: `deny_all` remains the base policy; named registry hostnames are explicitly permitted via bubblewrap `--bind` or equivalent network namespace configuration.

**Dimensions:**
- 3.1 PENDING Extend `src/executor/network.zig` with an `allowlist` policy variant alongside the existing `deny_all` stub; the allowlist permits egress to: `registry.npmjs.org`, `pypi.org`, `files.pythonhosted.org`, `static.crates.io`, `crates.io`, `proxy.golang.org`, `sum.golang.org`
- 3.2 PENDING Allowlist is read from a static compile-time config (`executor_network_policy.zig`) — no per-run override path; Phase 2 internal mirror replaces this
- 3.3 PENDING Executor logs each allowlist-permitted connection attempt at `debug` level: `executor.network.allowlist host={s} execution_id={hex}`; blocked attempts remain at `warn` level with error code `UZ-EXEC-008`
- 3.4 PENDING Add `EXECUTOR_NETWORK_POLICY` env var to executor (`deny_all` | `registry_allowlist`); default is `deny_all` for dev/macOS; bare-metal deploy sets `registry_allowlist`

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Worker fails startup with `UZ-CRED-001` log line if `ANTHROPIC_API_KEY` is absent from environment; does not silently start with a nil key
- [ ] 4.2 Executor receives the Anthropic API key via `StartStage` RPC payload only; `grep ANTHROPIC_API_KEY /proc/<executor-pid>/environ` returns empty on Linux
- [ ] 4.3 Worker successfully requests a GitHub App installation token and passes it to the executor; a real run completes and opens a PR against the target repo
- [ ] 4.4 Token refresh fires when run context token age exceeds 55 minutes; a run lasting over 55 minutes does not fail at PR-creation due to an expired token
- [ ] 4.5 Agent execution stage that runs `npm install` or `pip install` succeeds against the public registries on a bare-metal host with `EXECUTOR_NETWORK_POLICY=registry_allowlist`
- [ ] 4.6 Executor with `EXECUTOR_NETWORK_POLICY=deny_all` (default) blocks all egress — existing behavior is unchanged for dev and macOS

---

## 5.0 Out of Scope

- Per-run credential isolation (each run gets a unique vault sub-key) — v2 hardening
- Firecracker credential injection (credential passing into microVM guest) — v2 hardening
- Internal package mirror replacing public registry allowlist — Phase 2 of network policy
- Workspace-operator-configurable registry allowlist — v3 policy controls
- Credential rotation automation — separate SRE runbook concern
