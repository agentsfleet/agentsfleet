# Governance-edit dispatch

Read this file before editing Oracle's operating model, rule registry, profiles,
generated instructions, dispatch pages, audits, or governance hooks.

The canonical propagation design lives in
[`docs/ORLY_ARCHITECTURE.md`](../docs/ORLY_ARCHITECTURE.md).
The deterministic checker is `make audit`. The comprehension questionnaire is
[`audits/agents-md.md`](../audits/agents-md.md).

## Trigger

This dispatch fires for edits to:

- `orly/**`
- generated `AGENTS.md`
- `dispatch/**`
- `audits/agents-md.md`, `audits/agents-md.sh`, or `audits/data.sh`
- `evals/**` governance fixtures or runners
- `.githooks/pre-commit`, `.githooks/pre-push`, or governance `Makefile` targets
- `README.md` setup or propagation instructions

The agent has no override. Indy may use `SKIP_INVARIANCE_PUSH=1` for one push
when the reason is recorded in the newest commit message.

## Required action

1. Run `orly validate`.
2. Render the dotfiles profile into the repository root.
3. Run `make audit`; fix every failure.
4. Answer every question in `audits/agents-md.md` against the generated rules.
5. Run `make llmevals CHECK=1` before commit. Pre-push owns the live smoke run
   for semantic changes. Run the full matrix manually with `make llmevals`.
6. Run `orly verify --all --write-evidence --llm-result pass`.
7. Emit the invariance report before declaring the work complete.

Render command:

```bash
orly render --profile dotfiles --output ~/Projects/dotfiles
```

Any failure returns to the edit. Do not patch the checker to silence its result.

## Push enforcement

Pre-commit runs `make audit` when governance files are staged. Pre-push runs the
same deterministic chain, runs one live fixture per installed agent for semantic
changes, and regenerates `.oracle/evidence.json` against the pushed commit.

Repository synchronization remains explicit. Governance verification never
mutates a consumer repository or sibling worktree.

## Required output

```text
🚧 EDIT_RULES DISPATCH: <branch> @ <head>
  Registry:      ✅ valid
  Generated:     ✅ byte-stable
  Audit:         ✅ all checks passed
  Questionnaire: ✅ <N>/<N> YES
  Comprehension: ✅ pass
  Evidence:      ✅ current commit + registry digest
```
