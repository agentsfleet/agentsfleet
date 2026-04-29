# Agent-facing surfaces

UseZombie exposes a set of machine-readable files at stable public URLs. This index documents what each file is, where it is served, and how to edit it.

| File | Served at | Format | Purpose |
|---|---|---|---|
| `public/openapi.json` | `/openapi.json` | OpenAPI 3.1 JSON | API reference. **Build artifact** — edit YAML under `public/openapi/` and run `make openapi`. Never hand-edit. |
| `public/llms.txt` | `/llms.txt` | Plain text (llmstxt.org) | LLM discovery index. Lists the other agent surfaces and the current operation set. |
| `public/agent-manifest.json` | `/agents` | JSON-LD (schema.org) | Structured descriptor of the API operation set, policy classes, and machine-readable URL map. |
| `public/skill.md` | `/skill.md` | Markdown | Condensed capability description for LLM agents — execution model, operation table, auth. |
| `public/heartbeat` | `/heartbeat` | JSON | Runtime health blob. Served by the server; do not hand-edit. |

## Editing rules

- **`openapi.json`** — edit YAML under `public/openapi/`, run `make openapi`. The sync gate (`make check-openapi-sync`) asserts path parity with `src/http/router.zig`. See `public/openapi/AGENTS.md` for the editing recipe.
- **`llms.txt`, `skill.md`** — hand-edit in place. If you add, rename, or remove an API endpoint, update the operation tables in both files to match the new spec.
- **`agent-manifest.json`** — hand-edit in place. Update when the operation set, policy classes, or the machine-readable URL map change.
- **`heartbeat`** — generated at runtime. Do not hand-edit.

## Public URL contract

These URLs are a public interface. Do not rename the files or move them into subdirectories — external consumers (Mintlify, LLM crawlers, `llmstxt.org`-aware clients) depend on the paths exactly as listed above. Disk reorganization is a breaking change.
