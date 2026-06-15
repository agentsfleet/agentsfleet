# usezombie — Agent Delivery Control Plane

## What usezombie does
Hosts long-lived, event-driven autonomous workers (Agents) scoped to a
workspace. Inbound events arrive via webhooks (or other configured triggers)
and are appended to each Agent's event stream; the control plane assigns
them to a host-resident `agentsfleet-runner` via a lease, which runs the Agent's
loop — calling tools, updating state, and emitting further events. Operators steer or kill running Agents through the
control-plane API.

## API endpoints

Status transitions ride PATCH on the agent resource — body
`{ status: "active" | "stopped" | "killed" }`. The `paused` state is
platform-only (set by the anomaly gate) and rejected if requested via API.

| operationId | Method | Path | Body |
|---|---|---|---|
| `create_agent` | POST | `/v1/workspaces/{workspace_id}/agents` | install bundle |
| `update_agent` | PATCH | `/v1/workspaces/{workspace_id}/agents/{agent_id}` | `{config_json}` |
| `stop_agent` | PATCH | `/v1/workspaces/{workspace_id}/agents/{agent_id}` | `{status:"stopped"}` |
| `resume_agent` | PATCH | `/v1/workspaces/{workspace_id}/agents/{agent_id}` | `{status:"active"}` |
| `kill_agent` | PATCH | `/v1/workspaces/{workspace_id}/agents/{agent_id}` | `{status:"killed"}` |
| `delete_agent` | DELETE | `/v1/workspaces/{workspace_id}/agents/{agent_id}` | — (must kill first) |
| `post_agent_message` | POST | `/v1/workspaces/{workspace_id}/agents/{agent_id}/messages` | steer message |
| `list_agent_events` | GET | `/v1/workspaces/{workspace_id}/agents/{agent_id}/events` | — |
| `stream_agent_events` | GET | `/v1/workspaces/{workspace_id}/agents/{agent_id}/events/stream` | — (Server-Sent Events) |
| `ingest_agent_webhook` | POST | `/v1/webhooks/{agent_id}` | provider-shaped event |
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
- `safe`: `list_agent_events`, `stream_agent_events`, `get_tenant_billing`, `get_tenant_billing_charges`, `get_tenant_metering_periods` — allow by default
- `sensitive`: `create_agent`, `update_agent`, `stop_agent`, `resume_agent`, `kill_agent`, `post_agent_message`, `ingest_agent_webhook` — require explicit confirmation
- `critical`: `delete_agent` — irreversible row + history purge; require double confirmation

## Revenue model
self-managed (bring your own Large Language Model (LLM) API key) + credit-pool metering: event receipts are free; active runtime is $0.0001/sec under both postures; platform-managed also adds provider token costs. New tenants get a $5 starter credit that never expires.
usezombie never stores or marks up LLM provider costs.
