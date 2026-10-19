# UI requests

**Branch:** `feat/m229-postgres-schema-clean-slate`
**Folded-into:** `M229_001`

All 22 reports and their subitems are tracked in the [spec UI table](../v2/active/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md#8-ui-follow-up--user-reported-fixes).
This log records the requested server additions, client evidence and remaining release checks.

## Private setup-chat search and titles

- Server `1b084cd67` adds creator-only title and message search through `GET /v1/workspaces/{workspace_id}/drafts?q=…`.
- The trimmed query is NUL-free and contains 1..200 Unicode characters. Continue each search with the same query and its server cursor.
- New chats take their initial title from the first request or source description. Explicit title edits survive.
- App `c272f510d` reads bounded search pages, cancels replaced requests and ignores late responses. Search loads only after intent.
- Ordinary history and search share row rendering, selection and confirmed deletion. Access loss clears private state through the owning client.
- Saved entity-tag changes refresh an open search, including message-only matches. Busy-only changes preserve card references and avoid extra search requests.

## Recorded setup and publication versions

- Server `1aeb6ffb7` sends nullable `candidate_revision`, `setup_revision` and `publication_revision` on draft cards.
- App `c272f510d` requires nullable positive safe integers and labels proposed, saved and published versions independently.
- Save and Apply use actual operation receipts. Publication refreshes a bounded summary page for the recorded publication version.
- Target validation stays separate from completed creation. A refused update keeps Save changes available and never reports Fleet saved.

## Historical runner leases

- Server `bff5ec896` retains settled leases for thirty days and sends `destination`: `available`, `fleet_deleted` or `not_permitted`.
- Event type and actor can be null after event deletion. This route serves fleet leases, excluding setup conversations.
- App `38fe8ca72` validates the page and keeps retained agent callsigns readable. Only an available destination links its fleet or workspace.
- The lease dialog uses the served destination without an additional availability request.
- Focused schedule and lease checks pass 39 tests across six files. Source rereview is clean after removing the unavailable workspace link.

## Saved schedule summary

- Server `9deadc1c3` sends required `description`, `run_state` and always-sent nullable `next_run_at`. Server `54f30c538` corrects midnight wording.
- App `38fe8ca72` shows the description and next recorded time under Setup. Paused and unconfirmed schedules never promise a stale run.
- No schedule and a saved schedule awaiting registration have distinct explanations. Execution history stays in Fleet Chat and Activity.
- The standalone Trigger tab and browser cron parser are removed. Claude refreshed the root lockfile in `8a194eda3`.
- Command-line interface (CLI) commit `dc65550b6` validates every schedule success before text or intact JSON output.

## Saved fleet purpose and identity

- Server `18ac1e074` adds always-sent nullable `description` to fleet summaries. Cards and recents consume it without another read.
- App `fab63c116` leads with the stable agent callsign and shows the saved purpose beneath it. Null purposes are omitted.
- Recents preserve visit order, workspace isolation and their twelve-item bound when purposes change.
- Fleet headings, leases and billing use the same callsign. Billing displays the callsign alone. API identity fields remain unchanged.

## Exact money and CLI completeness

- Server `2c9c0300b` sends bounded, canonical non-negative decimal strings for nanos. Integer arithmetic preserves exact totals and ordering.
- App `716c0cfdd` and CLI `e0c15db29` consume this representation across billing, event spend and model rates.
- CLI `e7c5c55f9` validates fleet list pages before text or intact JSON success. Calendar-invalid instants fail the read.
- CLI `777ff460d` adds private chat list/search, rename and confirmed deletion alongside existing `chat [draft-id]`.
- List reads one bounded page. Rename uses a fresh entity tag and explains busy-chat refusal. Unattended deletion requires `--yes`.
- Focused HTTP and terminal-input checks pass 54 tests across four files. Command inventory, rejection and help checks pass 173 tests across four files.
- Complete CLI coverage passes 1937 tests, with 16 existing live-test skips and zero failures across 214 files. Functions and lines reach 100%.

## Verification and performance

Post-review follow-up replaces all eight stale catalogue-table references in CLI, playbooks and app tests. The spec's exact R2 inventory returns zero. `make lint-app lint-cli check-playbooks` passes (`/tmp/m229-client-followup-lint.txt`).

`cli/test/instant.unit.test.ts` adds three passing ordering regressions for Dimension 4.7: equal instants at different precision, fractional milliseconds and an instant whose text sorts in the opposite order. Comparisons use parsed time.

`make test-coverage-all` passes every configured package floor.

| Package | Files | Passing tests | Coverage |
|---|---|---|---|
| App | 412 | 3848 | 100% statements, branches, functions and lines |
| Website | 20 | 133 | 100% on all four measures |
| CLI | 214 | 1937 | 100% functions and lines; 16 existing live-test skips |
| Design system | 62 | 497 | 100% on all four measures |

- Evidence: `/tmp/m229-completion-coverage-all-final.txt`. No threshold, timeout, skip or budget changed.
- `make lint-app` and `make lint-cli` pass in the scoped `c272f510d` and `777ff460d` commit hooks.
- `make dry-app-smoke` passes four desktop/mobile sign-in and protected-route checks. Evidence: `/tmp/m229-completion-browser-smoke.txt`.
- Production `bun run build` and `bun run size --json` pass all 25 budgets and the server-module guard.
- Shared compressed JavaScript is 274265 / 276480 bytes. New-fleet total is 334933 bytes; fleet-detail total is 364328 bytes.
- Baseline `4e0bafb45502bad3b3b88b631a3690bb80ec2496` measured 364917 and 366692 bytes for those pages. Reductions are 29984 and 2364 bytes.
- The baseline failed the server-module guard. Current limits remain unchanged. Evidence: `/tmp/m229-sidebar-build-split.txt` and `/tmp/m229-sidebar-size-split.json`.
- Duplicate audit: one weaker case removed; two overlapping cases merged into one. Net reduction: two cases, with retained assertions and app coverage at 100%.

## Review and remaining release checks

Native reviews are clean for the schedule/lease readers and the new CLI history source.
Sidebar review stopped after three cycles without convergence; its final finding has a failing-before regression and a repair.

Claude must inspect the final saved entity-tag propagation before final source approval. Review receipts remain under `.tmp/reviews/`.

Authenticated local priming and reported-screen browser checks need the dedicated testing identity decision recorded in the mailbox.
Read-only daemon health and readiness return HTTP 200. Anonymous browser checks do not establish authenticated behavior or scheduled delivery.

Public command references and the changelog need the new chat commands and updated served fields.
The docs-review checklist is `.tmp/audits/m229-docs-review-checklist.md`; no docs-repository writes occur before Claude posts docs ready.
Claude owns the remaining shared verification and final spec move from `active/` to `done/`.
