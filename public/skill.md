# agentsfleet — Fleet Delivery Control Plane

## What agentsfleet does
Hosts long-lived, event-driven autonomous workers (Fleets) scoped to a
workspace. Inbound events arrive via webhooks (or other configured triggers)
and are appended to each Fleet's event stream; the control plane assigns
them to a host-resident `agentsfleet-runner` via a lease, which runs the Fleet's
loop — calling tools, updating state, and emitting further events. Operators steer or kill running Fleets through the
control-plane API.

## API endpoints

Status transitions ride PATCH on the fleet resource — body
`{ status: "active" | "stopped" | "killed" }`. The `paused` state is
platform-only (set by the anomaly gate) and rejected if requested via API.

| operationId | Method | Path | Body |
|---|---|---|---|
| `create_fleet` | POST | `/v1/workspaces/{workspace_id}/fleets` | `SKILL.md` bytes or `{bundle_id}` |
| `list_fleet_bundles` | GET | `/v1/fleets/bundles` | — |
| `import_fleet_bundle` | POST | `/v1/workspaces/{workspace_id}/fleets/bundles/snapshots` | bundle Markdown snapshot |
| `get_fleet_bundle` | GET | `/v1/workspaces/{workspace_id}/fleets/bundles/snapshots/{bundle_id}` | — |
| `update_fleet` | PATCH | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` | `{config_json}` |
| `stop_fleet` | PATCH | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` | `{status:"stopped"}` |
| `resume_fleet` | PATCH | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` | `{status:"active"}` |
| `kill_fleet` | PATCH | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` | `{status:"killed"}` |
| `delete_fleet` | DELETE | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` | — (must kill first) |
| `post_fleet_message` | POST | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages` | steer message |
| `list_fleet_events` | GET | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events` | — |
| `stream_fleet_events` | GET | `/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream` | — (Server-Sent Events) |
| `ingest_connector_webhook` | POST | `/v1/ingress/{provider}` | provider-signed connector event |
| `ingest_qstash_schedule` | POST | `/v1/ingress/qstash/schedules` | QStash-signed scheduled fire |
| `ingest_fleet_webhook` | POST | `/v1/webhooks/{fleet_id}` | provider-shaped event |
| `get_tenant_billing` | GET | `/v1/tenants/me/billing` | — |
| `get_tenant_billing_charges` | GET | `/v1/tenants/me/billing/charges` | — |
| `get_tenant_metering_periods` | GET | `/v1/tenants/me/billing/charges/{event_id}/telemetry` | — |

## Authentication
`Authorization: Bearer <api_key>`

The QStash schedule ingress uses QStash delivery signatures instead of a bearer token.

## Machine-readable surfaces
- OpenAPI spec: `/openapi.json`
- agentsfleet manifest (JSON Linked Data): `/agentsfleet-manifest.json`
- This file: `/skill.md`
- Large Language Model (LLM) discovery: `/llms.txt`

## Policy classes
- `safe`: `list_fleet_events`, `stream_fleet_events`, `list_fleet_bundles`, `get_fleet_bundle`, `get_tenant_billing`, `get_tenant_billing_charges`, `get_tenant_metering_periods` — allow by default
- `sensitive`: `create_fleet`, `import_fleet_bundle`, `update_fleet`, `stop_fleet`, `resume_fleet`, `kill_fleet`, `post_fleet_message`, `ingest_fleet_webhook` — require explicit confirmation
- `critical`: `delete_fleet` — irreversible row + history purge; require double confirmation

## Revenue model
self-managed (bring your own Large Language Model (LLM) API key) + credit-pool metering: event receipts are free; active runtime is $0.0001/sec under both postures; platform-managed also adds provider token costs. New tenants get a $5 starter credit that never expires.
agentsfleet never stores or marks up LLM provider costs.
