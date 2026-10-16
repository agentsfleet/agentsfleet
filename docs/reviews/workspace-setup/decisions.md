---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat: owner decisions

## What it is

These are Indy's decisions from the October 14 review. They govern the proposed cutover; implementation is pending. The [resource reference](../../architecture/api_resources.md) is the authority for paths and types.

## Why it exists

Later reviewers should assess enforcement and missing proof without reopening settled product choices.

## How it behaves

| Decision | Required behavior |
|---|---|
| L1 | Creator-private drafts use `POST /drafts/{draft_id}/messages` → `202 {status, sequence, replayed}`. Answers and usage appear in messages and the private stream. Unlimited drafts per person. No public turn identifier or turn-status route. Amend the REST guide for both draft and fleet conversation sends. |
| L2 | Cancel setup = Kill: `PATCH /fleets/{fleet_id} {status:"killed"}`, under existing `fleet:write` authority. |
| L3 | Cancelled setup returns the linked draft to `editing`. Another Save uses a fresh creation key and a new fleet. |
| L4 | Closed private chat remains 30 days, with expiry displayed. Active work never expires. Accounting remains content-free. |
| L5 | A Command-Line Interface (CLI) install returning `installing` prints missing checks and a retry command, then exits nonzero. |
| L6 | Save and Save and run now. API start choices `none` and `run_now`; reviewed triggers remain identical. |
| L7 | Save never publishes to Library automatically. Save to library is explicit. |
| L8 | The separate schedule fix ships stable prefixed provider IDs. A follow-up adds a distinct content revision, a revision-bearing signed destination, and an owned-prefix sweep. |
| L9 | Tokenizer-counted caps: 32,768 input per call; 8,192 output across a turn. |
| L10 | Keep the creator-private draft progress stream. |
| L11 | One Library collection: `/fleet-libraries`. |
| L12 | Remove `/library-entries`; owned entries use literal `/fleet-libraries/owned`, removal uses the Library member route. |
| L13 | Fleet PATCH writes status/name only. `POST /fleets/{id}/revisions` is the only writer of an existing fleet's setup. |
| L14 | Fleet PATCH requires `If-Match`; absence returns 428. |
| L15 | Fleet messages require a version 7 Universally Unique Identifier (UUIDv7) `operation_id`. |
| L16 | Remove stored `trigger_markdown`. The later chat-only instruction below removes the export requirement. |
| L17 | Delete the unmounted GettingStartedWidget. Keep the live onboarding-refresh module. |
| L18 | Delete Defaults and Security placeholder settings pages. |
| L19 | One shared setup validator and set of limits. The chat-only instruction removes the frontmatter input requirement. |
| L20 | Budget fields become `daily_nanos`/`monthly_nanos` across this milestone; money uses decimal strings on the wire. |
| L21 | No compatibility code, aliases, deprecation windows, or old-client paths. Acceptance-test rows may be reset at an explicitly chosen release boundary. No live reset is performed by these documents. |

Authoring admission is also settled: under one wallet-row lock, admit another running turn only when `balance >= (running + 1) * worst_case_turn_cost`. Draft count has no fixed cap. The [runner design](../../architecture/runner_fleet.md#authoring-leases) defines prices, outstanding attempts, and the assurance's limit.

### Chat-only authoring amendment

Indy's later instruction: “Okay makes sense. i think then the import/export must be removed and we support the easier way as a terminal chat client”. Browser and terminal chat share the private draft API. Explicit Library publication and pinned revision reuse remain. File/GitHub/template-bundle inputs and import/export adapters are removed. This supersedes the export and frontmatter-input portions of L16/L19; the remaining locked decisions stand.

Review reports describe the packet they inspected. The canonical architecture and active specs include this later owner amendment; no second general review is requested.

### One pull request

Indy's latest instruction: “Yes go, jpon rewrite should we write the specs, ideally i want the cutover in 1PR that you are on now.” The chat cutover and existing website, analytics, and branding batch share `feat/website-completion` and its existing worktree. This owner instruction overrides the repository's general preference for separate backend and interface pull requests; sections retain independent proof requirements.

L8 remains a follow-up. The separately owned schedule fix is a prerequisite to final schedule verification; it is not claimed merged or tested here. Published changes use the [public change handoff](public-changes.md) for a separate `~/Projects/docs` branch. That repository is not edited here.

## Limits

The previous worker, preview POST, weaker configuration schema, fixed authoring slots, and extra Library collection recommendations are superseded. [The resolution ledger](codex-response.md) records reasoning without leaving those as live options.

## Related pages

- [Source findings](source-findings.md)
- [Cutover spec](../../v2/active/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md)
