# Machine-readable surfaces

agentsfleet exposes a set of machine-readable files at stable public URLs. This index documents what each file is, where it is served, and how to edit it.

| File | Served at | Format | Purpose |
|---|---|---|---|
| `public/openapi.json` | `/openapi.json` | OpenAPI 3.1 JSON | API reference. **Committed static artifact** — no generator and no gate; edit directly until the Rust daemon emits its own. |
| `public/llms.txt` | `/llms.txt` | Plain text (llmstxt.org) | Large Language Model (LLM) discovery index. Lists the other machine-readable surfaces and the current operation set. |
| `public/agentsfleet-manifest.json` | `/agentsfleet-manifest.json` | JSON Linked Data (schema.org) | Structured descriptor of the API operation set, policy classes, and machine-readable URL map. |
| `public/skill.md` | `/skill.md` | Markdown | Condensed capability description for LLM/tool consumers — execution model, operation table, auth. |
| `public/heartbeat` | `/heartbeat` | JSON | Runtime health blob. Served by the server; do not hand-edit. |

## Editing rules

- **`openapi.json`** — a committed static artifact. The split YAML, the Redocly
  bundler and the `check-openapi` gate family were removed: the Rust daemon will
  emit this document itself, and until it does the file is edited directly.
- **`llms.txt`, `skill.md`** — hand-edit in place. If you add, rename, or remove an API endpoint, update the operation tables in both files to match the new spec.
- **`agentsfleet-manifest.json`** — hand-edit in place. Update when the operation set, policy classes, or the machine-readable URL map change.
- **`heartbeat`** — generated at runtime. Do not hand-edit.

## Public URL Rules

These URLs are a public interface. Do not rename the files or move them into subdirectories — external consumers (Mintlify, LLM crawlers, `llmstxt.org`-aware clients) depend on the paths exactly as listed above. Disk reorganization is a breaking change.
