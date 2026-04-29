---
# Common authoring mistake: runtime keys at top level instead of under x-usezombie:.
# Parser should reject with RuntimeKeysOutsideBlock.
name: misplaced-runtime
trigger:
  type: api
tools:
  - agentmail
budget:
  daily_dollars: 1.0
---
