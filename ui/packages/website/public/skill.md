# UseZombie — Agent Delivery Control Plane

## What UseZombie does
Turns spec files into validated pull requests using a sequential NullClaw agent pipeline:

1. **Echo** (planner) — reads spec + repo, produces `plan.json`
2. **Scout** (builder) — implements plan, produces code + `implementation.md`
3. **Warden** (validator) — reviews against spec, runs tests, produces `validation.md` (T1-T4 tiered findings)
4. On failure: Warden's defects feed back into Scout for retry (up to 3 attempts)
5. On pass: PR is opened, `run_summary.md` is committed

## API endpoints (6 operations)

| operationId | Method | Path |
|---|---|---|
| `start_run` | POST | `/v1/runs` |
| `get_run` | GET | `/v1/runs/{run_id}` |
| `retry_run` | POST | `/v1/runs/{run_id}:retry` |
| `pause_workspace` | POST | `/v1/workspaces/{workspace_id}:pause` |
| `list_specs` | GET | `/v1/specs?workspace_id={id}` |
| `sync_specs` | POST | `/v1/workspaces/{workspace_id}:sync` |

## Authentication
`Authorization: Bearer <api_key>`

## Machine-readable surfaces
- OpenAPI spec: `/openapi.json`
- Agent manifest (JSON-LD): `/agents`
- This file: `/skill.md`
- LLMs discovery: `/llms.txt`

## Policy classes
- `safe`: get_run, list_specs, sync_specs — allow by default
- `sensitive`: start_run, retry_run, pause_workspace — require explicit confirmation
- `critical`: destructive/permission changes — require double confirmation (none in M1)

## Run lifecycle states
```
SPEC_QUEUED → RUN_PLANNED → PATCH_IN_PROGRESS → PATCH_READY
→ VERIFICATION_IN_PROGRESS → (PASS) PR_PREPARED → PR_OPENED → NOTIFIED → DONE
                            → (FAIL) VERIFICATION_FAILED → Scout retry → BLOCKED
```

## Artifacts per run (committed to feature branch under docs/runs/<run_id>/)
- `plan.json` — Echo's task plan
- `implementation.md` — Scout's implementation summary
- `validation.md` — Warden's T1-T4 findings
- `attempt_N_defects.md` — defects from failed attempt N
- `run_summary.md` — final summary: run_id, PR URL, token totals, artifact list

## Revenue model
BYOK (bring your own LLM API key) + per-agent-second compute billing.
UseZombie never stores or marks up LLM provider costs.
