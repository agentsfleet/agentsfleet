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
- Same-query refresh retains matches and the current cursor until the replacement page arrives. Temporary failures keep readable results and offer Retry search; current access refusal clears them. Old continuation responses cannot append to refreshed results.
- Claude's 09:20 source review accepted the sidebar subject to this refresh repair. Its interaction regression first failed, then all four focused files / 28 tests passed (`/tmp/m229-search-refresh-red.txt`, `/tmp/m229-search-refresh-green.txt`).

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

- Server `2c9c0300b` sends bounded, canonical decimal strings for nanos. Integer arithmetic preserves exact totals and ordering. The later debt change permits a minus sign only on wallet `balance_nanos`; other amounts remain non-negative.
- App `716c0cfdd` and CLI `e0c15db29` consume this representation across billing, event spend and model rates.
- CLI `e7c5c55f9` validates fleet list pages before text or intact JSON success. Calendar-invalid instants fail the read.
- CLI `777ff460d` adds private chat list/search, rename and confirmed deletion alongside existing `chat [draft-id]`.
- List reads one bounded page. Rename uses a fresh entity tag and explains busy-chat refusal. Unattended deletion requires `--yes`.
- Focused HTTP and terminal-input checks pass 54 tests across four files. Command inventory, rejection and help checks pass 173 tests across four files.
- Complete CLI coverage passes 1952 tests, with 16 existing live-test skips and zero failures across 215 files. Functions and lines reach 100%.
- Commit `aa112ddd1` makes both clients accept exact signed wallet debt, including `-50` and the signed minimum; they refuse `-0`, leading zeros and either overflow. Header and billing page share formatting, and the CLI provider-reset warning accepts debt. Tiny debt remains distinct from zero.
- Debt regressions first failed in both clients, then 36 CLI and 56 app focused tests passed. Native source review found no concrete defects across the ten production files (`.tmp/reviews/m229-signed-balance.md`).

## Verification and performance

Post-review follow-up replaces all eight stale catalogue-table references in CLI, playbooks and app tests. The spec's exact R2 inventory returns zero. `make lint-app lint-cli check-playbooks` passes (`/tmp/m229-client-followup-lint.txt`).

`cli/test/instant.unit.test.ts` adds three passing ordering regressions for Dimension 4.7: equal instants at different precision, fractional milliseconds and an instant whose text sorts in the opposite order. Comparisons use parsed time.

`make test-coverage-all` passes every configured package floor.

| Package | Files | Passing tests | Coverage |
|---|---|---|---|
| App | 412 | 3861 | 100% statements, branches, functions and lines |
| Website | 20 | 133 | 100% on all four measures |
| CLI | 215 | 1952 | 100% functions and lines; 16 existing live-test skips |
| Design system | 62 | 497 | 100% on all four measures |

- Evidence: `/tmp/m229-signed-coverage-all-final.txt`. No threshold, timeout, skip or budget changed.
- `make lint-app lint-cli` passes after signed balances and the search refresh repair (`/tmp/m229-signed-lint.txt` and the scoped `a1dd087d2` / `aa112ddd1` commit hooks).
- `make dry-app-smoke` passes four desktop/mobile sign-in and protected-route checks. Evidence: `/tmp/m229-completion-browser-smoke.txt`.
- Production `bun run build` and `bun run size --json` pass all 25 budgets and the server-module guard.
- Shared compressed JavaScript is 274266 / 276480 bytes. New-fleet total is 334981 bytes; fleet-detail total is 364428 bytes.
- Baseline `4e0bafb45502bad3b3b88b631a3690bb80ec2496` measured 364917 and 366692 bytes for those pages. Reductions are 29936 bytes (8.20%) and 2264 bytes (0.62%).
- The baseline failed the server-module guard. Current limits remain unchanged. Evidence: `/tmp/m229-signed-build-final.txt` and `/tmp/m229-signed-size-final.json`.
- Duplicate audit: one weaker case removed; two overlapping cases merged into one. Net reduction: two cases, with retained assertions and app coverage at 100%.

## Review and remaining release checks

Native reviews are clean for the schedule/lease readers and the new CLI history source.
After the three native sidebar review cycles, Claude reviewed the final tag propagation and accepted it with one required refresh repair. That repair now has failing-before and passing-after interaction evidence. Review receipts remain under `.tmp/reviews/`.

Authenticated local priming and reported-screen browser checks need the dedicated testing identity decision recorded in the mailbox.
Read-only daemon health and readiness return HTTP 200. Anonymous browser checks do not establish authenticated behavior or scheduled delivery.

Public command references and the changelog are merged in [docs PR 222](https://github.com/agentsfleet/docs/pull/222), merge commit `b17f90500b3cae8ef291b3113d6b89f98c91906a`.
Both Greptile findings were repaired in `3234f12`, answered and resolved. The final head passed Continuous Integration and Greptile reviewed 15 files with zero new comments; two quiet polls completed.
Final docs `make lint` passed 22 tests, page rules, route drift, Mintlify validation and 152 links across 34 pages. Gitleaks scanned all 12 branch commits with no leaks. Docs `main` is pulled locally and the merged worktree and local branch are removed.
The docs-review checklist is `.tmp/audits/m229-docs-review-checklist.md`.
Claude owns the remaining shared verification and final spec move from `active/` to `done/`.
