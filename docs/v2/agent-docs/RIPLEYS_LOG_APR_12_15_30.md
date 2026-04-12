M8_001 Architecture Decisions — Slack Plugin Acquisition

  Date: Apr 12, 2026: 15:30 PM
  Branch: feat/m8-slack-plugin
  Author: Oracle (design log)

  ---
  The 4 Actors (M8_001 edition)

  ┌──────────────────┬──────────────────────────────────────┬──────────────────────────────────────────────────────────────────┐
  │       ACP        │              UseZombie               │                          What they do                            │
  ├──────────────────┼──────────────────────────────────────┼──────────────────────────────────────────────────────────────────┤
  │ Buyer            │ Workspace owner (human)              │ Clicks "Add to Slack", approves OAuth, owns the bot installation │
  ├──────────────────┼──────────────────────────────────────┼──────────────────────────────────────────────────────────────────┤
  │ Agent            │ UseZombie OAuth callback handler     │ Exchanges code, stores token, creates routing record, posts msg  │
  ├──────────────────┼──────────────────────────────────────┼──────────────────────────────────────────────────────────────────┤
  │ Seller           │ Slack (OAuth provider + API)         │ Issues bot token, receives events, surfaces approval buttons     │
  ├──────────────────┼──────────────────────────────────────┼──────────────────────────────────────────────────────────────────┤
  │ Payment Provider │ UseZombie zombie vault               │ Holds the bot token, injects it at runtime, never leaks it      │
  └──────────────────┴──────────────────────────────────────┴──────────────────────────────────────────────────────────────────┘

  ---
  Key Design Decisions Made During M8_001 Spec

  1. No Slack-specific columns in core.workspaces

  Original spec had `slack_team_id TEXT` in core.workspaces. Rejected:
  workspace is channel-agnostic. Slack is one acquisition path. Adding Teams,
  Discord, or any future connector would require new columns — no.

  Decision: core.workspace_integrations(provider, external_id, workspace_id)
  is the extensibility point. One table for all providers.

  2. workspace_integrations = routing metadata only, no credential storage

  Early draft included credential_ref in workspace_integrations. Rejected:
  the zombie vault (vault.secrets) is the single source of truth for credentials.
  Having a credential pointer in a separate table creates two competing sources.
  Any code that reads the bot token must go through crypto_store, period.

  workspace_integrations only answers one question:
    "Given a Slack team_id, which UseZombie workspace owns it?"

  3. OAuth path and CLI path converge at the same vault slot

  Two ways to get the Slack bot token into the zombie vault:
    A. OAuth: user clicks "Add to Slack" → token stored automatically
    B. CLI: user runs `zombiectl credential add slack` → token stored manually

  Both write to: vault.secrets(workspace_id, key_name="slack")
  Both create: workspace_integrations row with source='oauth' or source='cli'

  The runtime (M9 execute pipeline) never knows or cares which path was used.

  4. No Block Kit template builders in Zig

  Original spec had src/zombie/slack_onboarding.zig with hardcoded Block Kit
  JSON builders for template selection (Fix Bugs, Review PRs, etc). Rejected:

  - LLMs reason about output format dynamically — static templates are the wrong
    layer of abstraction.
  - Block Kit evolves; Zig code that encodes it becomes a maintenance tax.
  - The zombie itself is the right place for message formatting decisions.

  M8 posts exactly one constant message after OAuth:
    "UseZombie is connected. Configure your Zombie at the dashboard."
  Everything after that is M9 + the Zombie's own reasoning.

  5. The confirmation message is a bootstrap exception

  After OAuth, the handler posts directly to Slack using the just-acquired token.
  This is the only place in M8 where we use the token directly — before any zombie
  exists, before any M9 grant is in place. It's a one-time bootstrap message.

  From that point forward, ALL Slack usage goes through:
    M9 /v1/execute → grant check → vault injection → proxied call

  6. Schema slot 028 (not 026)

  M9_001 takes slots 026 (integration_grants) and 027 (external_agents).
  M8's workspace_integrations must be 028 to avoid conflicts when branches merge.
  Slot gaps are fine pre-v2.0.0 — DB gets wiped on rebuild.

  7. "Zombie vault" terminology

  During spec design there was confusion between two vault systems:
    - op/1Password: operator secrets — infra keys, deploy keys, DB passwords.
      Used by the team running UseZombie. Never used at zombie runtime.
    - vault.secrets (zombie vault): runtime credentials for zombie execute calls.
      Uses crypto_store.store/load. Entirely separate from op.

  All M8 credential storage references "zombie vault" / "vault.secrets".
  Any mention of op:// in M8 code is a bug.

  ---
  What M8 does NOT own (and why)

  ┌──────────────────────────────────────┬──────────────────────────────────────────────────────┐
  │             NOT in M8                │                       Why                            │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ Integration grant authorization      │ M9_001 — zombie-level, service-level access control  │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ /v1/execute proxy pipeline           │ M9_001 — credential injection at call time           │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ Per-zombie Slack tokens              │ v3 — each zombie with its own Slack app               │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ Block Kit template builders          │ LLM-driven zombie handles output format               │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ Slack App Directory submission       │ Manual step, post-launch                             │
  ├──────────────────────────────────────┼──────────────────────────────────────────────────────┤
  │ Slack Enterprise Grid                │ Team-level only for v2                               │
  └──────────────────────────────────────┴──────────────────────────────────────────────────────┘

  ---
  How M8 connects to M9

  M8 creates the credential.
  M9 authorizes and proxies its use.

    M8: OAuth → vault.secrets(workspace_id, "slack") + workspace_integrations row
    M9: zombie requests grant → human approves → grant recorded
    M9: zombie calls /v1/execute { credential_ref: "slack" }
        → grant checked → crypto_store.load(workspace_id, "slack") → injected
        → zombie never sees the token

  For incoming events (Slack → zombie):
    workspace_integrations: provider="slack", external_id="T01ABC" → workspace_id
    zombie config: which zombie in that workspace handles Slack events
    M4: approval gate fires for high-risk actions
