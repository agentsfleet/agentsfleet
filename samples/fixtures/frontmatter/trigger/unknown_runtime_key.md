---
# Typo: `contxt` instead of a known runtime key. Rigid validation under
# x-usezombie: rejects with UnknownRuntimeKey.
name: typo-skill

x-usezombie:
  trigger:
    type: api
  tools:
    - agentmail
  budget:
    daily_dollars: 1.0
  contxt:
    foo: bar
---
