# Sol review round 2

Model: gpt-6.1-sol; reasoning: max; fresh context; native reviewer `/root/autonomy_sol_round2`.
Reviewed target SHA-256: `7ae306060b13ddad2f8459eb3bebdbd8ef452cff4dd0ea50c499231b31514aed`.
The reviewer did not read round 1's report before independent findings.

## Findings and disposition

No Blocker; unified scope judged sound as design intake.

- Major: ordinary progress/tool summaries absent. Accepted: short action/purpose/result and waiting/blocked states, expandable evidence and recovery progress added.
- Major: Slack scenario denied ref updates; production-repair scenario ended at an existing draft. Accepted: document current exact event-owned non-force corrections during one admitted request, retaining the absence of arbitrary existing-PR/cross-request authority.
- Major: connector and Slack pages said job-log redirects cannot be followed. Accepted: describe guarded bodyless GET redirects, fresh destination admission, stripped cross-origin headers and policy-dependent live access.
- Major: lease walkthrough described pre-compaction tool removal. Accepted: shared compaction/retained context now linked, without removing actual run bounds.
- Nit: greeting acceptance allowed an incident recap and lacked an explicit complete empty trace; failure recovery lacked its own observation. Accepted: strengthen both rows.
- Nit: sandbox evaluation described a historical optimization failure as current. Accepted: label history and link the implemented preparation record without claiming full acceptance.

No new owner decision blocks scenario intake. The detailed spec still must resolve admission/standing authority, revision adoption, publication preconditions, Jev integration and retained-work limits.

The Codex comparison assumes no hidden intent classifier. The current child inbox does not implement mid-run human steering.

## Completed reads

All 29 top-level architecture pages and seven scenario pages were read completely, 10,800 lines in the reviewed snapshot:

```text
README.md
api_resources.md
billing_and_provider_keys.md
billing_reporting.md
capabilities.md
concurrency.md
connectors.md
context_lifecycle.md
data_flow.md
datastore_scaling.md
direction.md
fleet_definitions.md
high_level.md
lease_flow.md
memory.md
observability.md
postgres_schemas.md
product_analytics.md
release_candidates.md
runner_execution.md
runner_fleet.md
rust-ownership.md
sandbox_evaluation.md
scaling.md
shared_chat_verification.md
testing.md
user_flow.md
web_app.md
website-job-entrance.md
scenarios/README.md
scenarios/durable-repair.md
scenarios/github-pr-reviewer.md
scenarios/production-deploy-repair.md
scenarios/slack-channel-resident.md
scenarios/slack-incident-responder.md
scenarios/two-fleets-github-slack.md
```

Instructions: repository AGENTS.md, .orly/LOCAL.md, .orly/AGENTS.md, architecture standard and the EGM design-critic persona in REVIEW mode.

Complete source reads: afr_agent prompt/mod.rs, loop/mod.rs, loop/conversation.rs, loop/context/mod.rs, context/mod.rs; afd_gate policy/egress/write.rs; afr_providers retry/mod.rs.

Selected source reads: afr_agent trace/mod.rs; ingress app_route.rs; afr_tools egress/mod.rs; afr_egress redirect/mod.rs; afr_providers turn.rs.
Codex reads at `181b4a853d396f009a3a02b9c36eb393e6acbe75`: core/src/session/turn.rs lines 360–570 and 1750–1875; client.rs lines 1030–1115, 1230–1320 and 1940–2010.

The review made no edits, test runs, commits or external posts. It is document/source review, not runtime proof or the full EGM pipeline.
