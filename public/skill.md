# usezombie — Agent Delivery Control Plane

## What usezombie does
Hosts long-lived, event-driven autonomous workers (Zombies) scoped to a
workspace. Inbound events arrive via webhooks (or other configured triggers)
and are appended to each Zombie's event stream; the worker watcher dispatches
them to the Zombie's loop, which can call tools, update state, and emit
further events. Operators steer or kill running Zombies through the
control-plane API.

## API endpoints

Status transitions ride PATCH on the zombie resource — body
`{ status: "active" | "stopped" | "killed" }`. The `paused` state is
platform-only (set by the anomaly gate) and rejected if requested via API.

| operationId | Method | Path | Body |
|---|---|---|---|
| `create_zombie` | POST | `/v1/workspaces/{workspace_id}/zombies` | install bundle |
| `update_zombie` | PATCH | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` | `{config_json}` |
| `stop_zombie` | PATCH | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` | `{status:"stopped"}` |
| `resume_zombie` | PATCH | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` | `{status:"active"}` |
| `kill_zombie` | PATCH | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` | `{status:"killed"}` |
| `delete_zombie` | DELETE | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` | — (must kill first) |
| `post_zombie_message` | POST | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages` | steer message |
| `list_zombie_events` | GET | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events` | — |
| `stream_zombie_events` | GET | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events/stream` | — (SSE) |
| `ingest_zombie_webhook` | POST | `/v1/webhooks/{zombie_id}` | provider-shaped event |

## Authentication
`Authorization: Bearer <api_key>`

## Machine-readable surfaces
- OpenAPI spec: `/openapi.json`
- Agent manifest (JSON-LD): `/agents`
- This file: `/skill.md`
- LLMs discovery: `/llms.txt`

## Policy classes
- `safe`: `list_zombie_events`, `stream_zombie_events` — allow by default
- `sensitive`: `create_zombie`, `update_zombie`, `stop_zombie`, `resume_zombie`, `kill_zombie`, `post_zombie_message`, `ingest_zombie_webhook` — require explicit confirmation
- `critical`: `delete_zombie` — irreversible row + history purge; require double confirmation

## Revenue model
BYOK (bring your own LLM API key) + per-agent-second compute billing.
usezombie never stores or marks up LLM provider costs.
