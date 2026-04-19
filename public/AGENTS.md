# Agent-facing surfaces

UseZombie exposes a set of machine-readable files at stable public URLs. This index documents what each file is, where it is served, and how to edit it.

| File | Served at | Format | Purpose |
|---|---|---|---|
| `public/openapi.json` | `/openapi.json` | OpenAPI 3.1 JSON | API reference. Currently hand-edited; a YAML split + bundler is planned under the M28_003 workstream. |
| `public/llms.txt` | `/llms.txt` | Plain text (llmstxt.org) | LLM discovery index. Lists the other agent surfaces and the current operation set. |
| `public/agent-manifest.json` | `/agents` | JSON-LD (schema.org) | Structured descriptor of the agent pipeline (Echo / Scout / Warden) and their permissions. |
| `public/skill.md` | `/skill.md` | Markdown | Condensed capability description for LLM agents — pipeline summary, operation table, auth. |
| `public/heartbeat` | `/heartbeat` | JSON | Runtime health blob. Served by the server; do not hand-edit. |

## Editing rules

- **`openapi.json`** — currently hand-edited. Any endpoint rename, add, or remove must also update the operation tables in `llms.txt` + `skill.md` to match. The M28_003 workstream (pending) moves this to a YAML-split source + `make openapi` bundler + `make check-openapi-sync` parity gate; when that lands, a `public/openapi/AGENTS.md` recipe will supersede this bullet.
- **`llms.txt`, `skill.md`** — hand-edit in place. If you add, rename, or remove an API endpoint, update the operation tables in both files to match the new spec.
- **`agent-manifest.json`** — hand-edit in place. Update when agent roles, permissions, or the machine-readable URL map change.
- **`heartbeat`** — generated at runtime. Do not hand-edit.

## Public URL contract

These URLs are a public interface. Do not rename the files or move them into subdirectories — external consumers (Mintlify, LLM crawlers, `llmstxt.org`-aware clients) depend on the paths exactly as listed above. Disk reorganization is a breaking change.
