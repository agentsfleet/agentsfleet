# UseZombie — Agent Delivery Control Plane

## What UseZombie does
Hosts long-lived, event-driven autonomous workers (Zombies) scoped to a
workspace. Inbound events arrive via webhooks (or other configured triggers)
and are appended to each Zombie's event stream; the worker watcher dispatches
them to the Zombie's loop, which can call tools, update state, and emit
further events. Operators steer or kill running Zombies through the
control-plane API.

## API endpoints

| operationId | Method | Path |
|---|---|---|
| `create_zombie` | POST | `/v1/workspaces/{workspace_id}/zombies` |
| `update_zombie` | PATCH | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}` |
| `kill_zombie` | POST | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/kill` |
| `post_zombie_message` | POST | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages` |
| `list_zombie_events` | GET | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events` |
| `stream_zombie_events` | GET | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events/stream` |
| `ingest_zombie_webhook` | POST | `/v1/webhooks/{zombie_id}` |

## Authentication
`Authorization: Bearer <api_key>`

## Machine-readable surfaces
- OpenAPI spec: `/openapi.json`
- Agent manifest (JSON-LD): `/agents`
- This file: `/skill.md`
- LLMs discovery: `/llms.txt`

## Policy classes
- `safe`: `list_zombie_events`, `stream_zombie_events` — allow by default
- `sensitive`: `create_zombie`, `update_zombie`, `kill_zombie`, `post_zombie_message`, `ingest_zombie_webhook` — require explicit confirmation
- `critical`: destructive/permission changes — require double confirmation (none currently)

## Revenue model
BYOK (bring your own LLM API key) + per-agent-second compute billing.
UseZombie never stores or marks up LLM provider costs.
