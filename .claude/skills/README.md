# Skills Catalog

Skills live in `.claude/skills/<skill-name>/SKILL.md`.
`.agents/skills/` is a symlink to the same directory — one source of truth for
Claude Code, Codex, OpenCode, and Amp.

## Workflow Phases

| Phase | Skill | Purpose |
|-------|-------|---------|
| Ideate | `office-hours` | YC forcing questions before writing a spec |
| Plan | `plan-ceo-review` | Founder taste — scope challenge, 10-star product |
| Plan | `plan-eng-review` | Eng lead — architecture + code quality lock-in |
| Implement | — | Write code with standard tools |
| Debug | `investigate` | Root cause first, fix second. Scope-locks edits. |
| Review | `review` | Staff engineer — pre-landing diff review against main |
| Ship | `ship` | Release engineer — merge main, test, bump, push, PR |
| Document | `document-release` | Generate release docs from VERSION, CHANGELOG |
| Retro | `retro` | Eng manager — weekly metrics, hotspots, trends |

## Safety Skills

| Skill | Invoke | Purpose |
|-------|--------|---------|
| `careful` | `/careful` | Warn before rm -rf, DROP TABLE, force-push, etc. |
| `freeze` | `/freeze <dir>` | Lock edits to a specific directory |
| `unfreeze` | `/unfreeze` | Remove the freeze boundary |
| `guard` | `/guard <dir>` | careful + freeze combined (max safety for prod) |

## Full Skill Reference

| Skill | Invoke | Description |
|-------|--------|-------------|
| `office-hours` | `/office-hours` | YC-mode validation: demand reality, narrowest wedge, future-fit. Saves a design doc. |
| `plan-ceo-review` | `/plan-ceo-review` | Rethink the problem, find the 10-star product. SCOPE EXPANSION / HOLD / REDUCTION. |
| `plan-eng-review` | `/plan-eng-review` | Lock in execution plan — architecture, data flow, diagrams, edge cases, test coverage. |
| `investigate` | `/investigate` | 4-phase debug: investigate → analyze → hypothesize → implement. Iron law: root cause first. |
| `review` | `/review` | Pre-landing PR review. Analyzes diff for safety, trust boundary violations, side effects. |
| `ship` | `/ship` | Merge main → test → bump VERSION → update CHANGELOG → commit → push → create PR. |
| `retro` | `/retro [window]` | Weekly engineering retrospective. Analyzes commits, sessions, code quality. |
| `oracle` | `/oracle` | Second-model review via CLI for cross-validation. |
| `write-unit-test` | `/write-unit-test` | Multi-stack test coverage including Zig and React/TypeScript. |
| `document-release` | `/document-release` | Generate release docs from VERSION, CHANGELOG, git history. |
| `frontend-design` | `/frontend-design` | Design and implement production-grade web UI. |
| `handoff` | `/handoff` | Package current work state for the next agent or session. |
| `create-cli` | `/create-cli` | Design command-line interface parameters and UX. |
| `careful` | `/careful` | Destructive command guardrails — warns before irreversible bash commands. |
| `freeze` | `/freeze` | Restrict edits to a directory. Used automatically by /investigate. |
| `unfreeze` | `/unfreeze` | Remove the freeze boundary. |
| `guard` | `/guard` | Full safety mode: careful + freeze. Use on prod systems. |

## Stack

Zig, React, Next.js. Skills are adapted for:
- Zig tests via `zig build test`
- React tests via Jest/Vitest + React Testing Library
- Next.js caching, SSR, and bundle optimization
