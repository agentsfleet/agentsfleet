# UI requests

**Branch:** `feat/m229-postgres-schema-clean-slate`
**Folded-into:** `M229_001`

This request log belongs to the existing schema clean-slate milestone.

### Private setup-chat search and titles — server landed; application reader in progress

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Field: creator-scoped `q` query matching chat title and message text, with the query included in cursor binding.
- Server: `1b084cd67` adds creator-only title and turn-text search. The trimmed query is 1..200 characters and is bound into its cursor. Application consumption remains required.
- Screen: Fleet setup chats in the sidebar.
- Endpoint: `POST /v1/workspaces/{workspace_id}/drafts` and its list/read responses.
- Field: initialize `title` from the first user request or the selected fleet/Library description instead of the constant `New fleet`. Preserve explicit title edits.
- Server: `1b084cd67` initializes the title from the first request or source description and preserves explicit edits. No transcript fetch or title mutation is needed in the sidebar.
- Screen: Fleet setup chats and the private chat heading.

### Setup-chat revision and publication summaries — server landed; application reader in progress

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Fields: current candidate revision, saved fleet setup revision, and publication revision when present.
- Server: `1aeb6ffb7` sends nullable integer `candidate_revision`, `setup_revision`, and `publication_revision`. The sidebar reader and state labels remain to be verified.
- Screen: Fleet setup chats in the sidebar.

### Historical runner leases and valid destinations — application unit proof complete

- Server: `bff5ec896` retains settled leases for thirty days and sends `destination`: `available`, `fleet_deleted`, or `not_permitted`.
- Event type and actor are explicitly nullable after event deletion. These are fleet leases; setup conversations are not served by this route.
- Application: validate the page before rendering. Keep retained callsigns and unavailable work readable. Only `available` links the fleet or workspace; other states explain why the fleet cannot open.
- Performance: the lease dialog uses the authoritative destination and makes no additional availability request.
- Proof: `make test-unit-app` passes 404 files / 3802 tests, and `make lint-app` passes. Focused schedule and lease checks pass 39 tests across six files. Native source rereview is clean after removing the unavailable workspace link.

### Saved schedule summary — application unit proof complete

- Server: `9deadc1c3` supplies required `description`, `run_state`, and always-sent nullable `next_run_at` in every schedule view.
- Application: Setup shows the server description and the recorded next time. Paused, activation pending, unavailable and unknown states never promise a stale run.
- The existing no-schedule and requested-but-unregistered explanations remain distinct. Execution history stays in Fleet Chat and Activity.
- Proof: `make test-unit-app` and `make lint-app` pass as recorded above. Malformed or missing summary fields fail the typed read. No browser cron parser or polling loop remains.
- Follow-up: command-line interface (CLI) schedule output is being aligned with the same fields. Claude is fixing the server's midnight wording quirk.
- Dependency cleanup: `cron-parser` is removed from the app manifest. Claude owns the root `bun.lock` refresh.

All five requested server additions have landed. Sidebar search and revision consumption, final coverage, and size verification remain in progress.

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

