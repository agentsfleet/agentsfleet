# usezombie — Agent Delivery Control Plane

## What usezombie does
Hosts long-lived, event-driven autonomous workers (Zombies) scoped to a
workspace. Inbound events arrive via webhooks (or other configured triggers)
and are appended to each Zombie's event stream; the control plane assigns
them to a host-resident `agentsfleet-runner` via a lease, which runs the Zombie's
loop — calling tools, updating state, and emitting further events. Operators steer or kill running Zombies through the
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
| `stream_zombie_events` | GET | `/v1/workspaces/{workspace_id}/zombies/{zombie_id}/events/stream` | — (Server-Sent Events) |
| `ingest_zombie_webhook` | POST | `/v1/webhooks/{zombie_id}` | provider-shaped event |
| `get_tenant_billing` | GET | `/v1/tenants/me/billing` | — |
| `get_tenant_billing_charges` | GET | `/v1/tenants/me/billing/charges` | — |
| `get_tenant_metering_periods` | GET | `/v1/tenants/me/billing/charges/{event_id}/telemetry` | — |

## Authentication
`Authorization: Bearer <api_key>`

## Machine-readable surfaces
- OpenAPI spec: `/openapi.json`
- Agent manifest (JSON Linked Data): `/agents`
- This file: `/skill.md`
- Large Language Model (LLM) discovery: `/llms.txt`

## Policy classes
- `safe`: `list_zombie_events`, `stream_zombie_events`, `get_tenant_billing`, `get_tenant_billing_charges`, `get_tenant_metering_periods` — allow by default
- `sensitive`: `create_zombie`, `update_zombie`, `stop_zombie`, `resume_zombie`, `kill_zombie`, `post_zombie_message`, `ingest_zombie_webhook` — require explicit confirmation
- `critical`: `delete_zombie` — irreversible row + history purge; require double confirmation

## Revenue model
self-managed (bring your own Large Language Model (LLM) API key) + credit-pool metering: event receipts are free; active runtime is $0.0001/sec under both postures; platform-managed also adds provider token costs. New tenants get a $5 starter credit that never expires.
usezombie never stores or marks up LLM provider costs.
