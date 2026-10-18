# UI requests

**Branch:** `feat/m229-postgres-schema-clean-slate`
**Folded-into:** `M229_001`

This request log belongs to the existing schema clean-slate milestone.

### Private setup-chat search and titles — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Field: creator-scoped `q` query matching chat title and message text, with the query included in cursor binding.
- Why: the current endpoint supports only state and linked-fleet filters. The sidebar can search loaded titles, but cannot truthfully offer search across all conversations.
- Screen: Fleet setup chats in the sidebar.
- Endpoint: `POST /v1/workspaces/{workspace_id}/drafts` and its list/read responses.
- Field: initialize `title` from the first user request or the selected fleet/Library description instead of the constant `New fleet`. Preserve explicit title edits.
- Why: old and new conversations need meaningful persisted titles without fetching each transcript or trying a title mutation while authoring is running.
- Screen: Fleet setup chats and the private chat heading.

### Setup-chat revision and publication summaries — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Fields: current candidate revision, saved fleet setup revision, and publication revision when present.
- Why: state and linked fleet identity already support In progress / Saving / Saved labels. Revision and publication indicators for other chats require summary fields; only the open draft currently supplies these facts.
- Screen: Fleet setup chats in the sidebar.

### Historical runner leases and valid fleet destinations — open

- Endpoint: `GET /v1/fleet-runners/{runner_id}/leases`.
- Fields: explicit fleet existence/navigation availability and the fleet kind when the lease belongs to a private setup conversation rather than a user fleet.
- Why: the current projection inner-joins `fleet.fleets` and `fleet.events`, so deleting a probe fleet can remove its lease from the list. A non-null name does not establish that the signed-in operator can open the tenant fleet screen. The reported setup/probe lease opens a missing fleet page.
- Requested behavior: retain lease history within the server's retention window when the related fleet/event disappears, and expose an authorized destination or an explicit unavailable state. Do not infer this from a fleet-name prefix.
- Screens: Runner → Leases and Review lease → Open fleet.

### Saved schedule summary — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}` and `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules`.
- Fields: a human-readable schedule description and nullable `next_run_at`, calculated by the server from the registered schedule and its timezone.
- Why: Setup should answer whether a schedule is set and when it will run. The browser must not calculate a misleading time for paused, unregistered, or stopped schedules.
- Behavior: distinguish no schedule, paused, activation pending, and next run unavailable. Keep execution history in Fleet Chat and Activity.
- Screen: the small Schedule summary within Fleet Setup.
- Dependency cleanup: the UI removed `cron-parser` from its package manifest. Claude owns the root `bun.lock`; refresh it with the next dependency update.

The UI continues with explicit loaded-title search, available setup state, and safe destination checking while these requests are open.

### Saved fleet purpose on lists — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/fleets`.
- Field: `description` containing the saved fleet purpose; nullability to be published by the server.
- Why: the detail already describes what the fleet does, but list summaries do not. Cards and recents should show the stable agent callsign with useful purpose text.
- Screens: Your fleets and sidebar recents.

