---
name: create-scaffold
description: Create or normalize repositories to the standard Makefile, CI, docs, and verification contract for any supported stack.
---

# Create Scaffold

Create or normalize a repository to the standard project layout for any supported stack.

## Usage

```
/create-scaffold <stack>
```

Supported stacks: `python`, `rust`, `go`, `typescript`, `javascript-cli`, `tauri`

If `<stack>` is omitted, prompt the user:

```
Which stack?
1. python        — Python backend (Django/FastAPI)
2. rust           — Rust library or API
3. go             — Go project
4. typescript     — TypeScript + Bun
5. javascript-cli — JavaScript/TypeScript CLI tool
6. tauri          — Tauri 2 desktop app (Rust + TypeScript)
```

## Common Inputs

- Repo path
- Forge (`github` or `gitlab`)

Stack-specific inputs are defined in `stacks/<stack>.md`.

## Shared Required Files

Every scaffold must produce:

- `Makefile`
- `make/quality.mk`, `make/test.mk`, `make/build.mk`, `make/push.mk`, `make/sonar.mk`
- CI config (`.gitlab-ci.yml` or `.github/workflows/ci.yml`)
- `VERSION`
- `CHANGELOG.md`
- `docs/diagrams/*.mmd` (Mermaid diagram source)

## Shared Command Workflow

```bash
mkdir -p make docs/diagrams tests
```

Ensure `Makefile` exposes the standard target taxonomy:

| Target        | Purpose                                              |
|---------------|------------------------------------------------------|
| `make dev`    | Start local dev server or run binary in dev mode     |
| `make up`     | Start background services (Docker Compose)           |
| `make down`   | Stop background services                             |
| `make lint`   | Run all linters and type checks                      |
| `make test`   | Run all unit tests                                   |
| `make build`  | Compile / bundle for production                      |
| `make _clean` | Remove generated artefacts (dist, coverage, .tmp)    |
| `make push`   | Push image/package to registry                       |
| `make sonar`  | SonarQube scan (optional)                            |

Web stacks additionally expose:

| Target           | Purpose                                     |
|------------------|---------------------------------------------|
| `make qa`        | Run full Playwright e2e suite (headless)    |
| `make qa-smoke`  | Run Playwright smoke tests only (CI gate)   |

Rules:
- Never use `make quality` — the standard target is `make lint`.
- `make up` / `make down` apply when the stack has Docker services. Omit for pure frontend.
- `make _clean` uses a leading underscore to signal it is destructive (removes build artefacts).
- Agents run headless. Do not add `make qa-headed` to shared targets; it may exist locally in personal Makefile overrides only.

Then follow the stack-specific workflow in `stacks/<stack>.md`.

## Verify

```bash
make lint
make test
make build
```

## Output Contract

Report:

1. Files created/updated.
2. Missing prerequisites.
3. Exact verify command results.
4. Any unresolved blockers.
