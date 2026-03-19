# Skills Catalog

Each skill lives in `skills/<skill-name>/SKILL.md`.

## Workflow Phases

| Phase | Skill | Purpose |
|-------|-------|---------|
| Plan | `plan-ceo-review` | Founder taste — scope challenge, 10-star product |
| Plan | `plan-eng-review` | Eng lead — architecture + code quality lock-in |
| Implement | — | Write code with standard tools |
| Review | `review` | Staff engineer — pre-landing diff review against main |
| Ship | `ship` | Release engineer — merge main, test, bump, push, PR |
| Document | `document-release` | Generate release docs from VERSION, CHANGELOG |
| Retro | `retro` | Eng manager — weekly metrics, hotspots, trends |

## Skill Reference

| Skill | Invoke | Description |
|-------|--------|-------------|
| `plan-ceo-review` | `/plan-ceo-review` | Rethink the problem, find the 10-star product. Three modes: SCOPE EXPANSION, HOLD SCOPE, SCOPE REDUCTION. |
| `plan-eng-review` | `/plan-eng-review` | Lock in execution plan — architecture, data flow, diagrams, edge cases, test coverage. |
| `review` | `/review` | Pre-landing PR review. Analyzes diff for safety, trust boundary violations, side effects. |
| `ship` | `/ship` | Fully automated ship workflow. Merge main → test → bump VERSION → update CHANGELOG → commit → push → create PR. |
| `retro` | `/retro [window]` | Weekly engineering retrospective. Analyzes commits, sessions, code quality. |
| `oracle` | `/oracle` | Second-model review via CLI for cross-validation. |
| `write-unit-test` | `/write-unit-test` | Generate test coverage for Zig/React/Next.js. |
| `document-release` | `/document-release` | Generate release docs from VERSION, CHANGELOG, git history. |
| `frontend-design` | `/frontend-design` | Design and implement production-grade web UI. |
| `handoff` | `/handoff` | Package current work state for the next agent. |
| `create-cli` | `/create-cli` | Design command-line interface parameters and UX. |

## Core Skills

- `write-unit-test/` — multi-stack including Zig and React/TypeScript
- `frontend-design/` — production-grade web UI with accessibility

## Stack

This project uses Zig, React, and Next.js. Skills are adapted for:
- Zig tests via `zig build test`
- React tests via Jest/Vitest + React Testing Library
- Next.js caching, SSR, and bundle optimization