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

- Endpoint: `GET /v1/fleets/runners/{runner_id}/leases`.
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

### Saved fleet purpose on lists — verified in server and application

- Endpoint: `GET /v1/workspaces/{workspace_id}/fleets`.
- Field: `description: string | null`, always sent by server commit `18ac1e074`; null means no saved purpose.
- Why: the detail already describes what the fleet does, but list summaries do not. Cards and recents should show the stable agent callsign with useful purpose text.
- Screens: Your fleets and sidebar recents. Both lead with the agent callsign and show the saved purpose beneath it; the technical slug stays out of the display.
- Proof: the list decoder preserves a purpose or null and rejects missing, malformed or empty values. Recents update changed purposes without changing visit order, workspace isolation or the twelve-item bound. App lint passes; canonical app coverage reports 402 files / 3791 tests and 100% on all four measures (/tmp/m229-purpose-coverage-all.txt). Native source review found no must-fix defect.
- Size: production build and all 25 budgets plus the server-module guard pass. Shared compressed JavaScript is 275318 / 276480 bytes; new-fleet total is 334590 bytes and fleet-detail total is 365323 bytes (/tmp/m229-purpose-size.json).

### Exact monetary wire values — verified in server and clients

- Change: `2c9c0300b` standardizes nanos fields as non-negative decimal strings matching `^(0|[1-9][0-9]*)$`.
- Readers: billing balance and charges, event costs, catalogue rates and model-entry rates in the application and CLI.
- Why: integer JSON loses precision beyond JavaScript's safe integer range. Catalogue readers and edits must use the same published representation as the server.
- Handoff: Claude published the verified field list for `2c9c0300b` and `763416e7c` in the mailbox. Application commit `716c0cfdd` and command-line interface (CLI) commit `e0c15db29` require bounded decimal strings; writes use the same representation. Live counters, event costs, balance and charge totals use integer arithmetic.
- Proof: `make lint-app` and `make lint-cli` pass. `make test-coverage-all` passes: app 402 files / 3783 tests, website 20 / 133, CLI 210 / 1841 passing tests / 16 existing skips, design system 62 / 497. All configured coverage measures are 100%; no floor, skip or timeout changed. Output: `/tmp/m229-coverage-all-complete.txt`.
- Size: production build and all 25 budgets plus the server-module guard pass. Shared compressed JavaScript: 275298 / 276480 bytes; new-fleet total: 334570 bytes; fleet-detail total: 365303 bytes. Compared with the recorded baseline, totals fall by 30347 and 1389 bytes respectively. Output: `/tmp/m229-bundle-nanos-build.txt`, `/tmp/m229-bundle-nanos-size.json`.

