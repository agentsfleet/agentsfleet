# Editing the OpenAPI spec (agents, read this first)

## Hard rule

**`public/openapi.json` is a build artifact.** Never hand-edit it.

- Direct edits to `public/openapi.json` will be wiped on the next `make openapi`.
- CI's bundle-in-sync gate will fail any PR that commits a hand-edited `public/openapi.json`.
- The real source lives under `public/openapi/` — YAML files grouped by tag.

## What lives where

```
public/openapi/
├── root.yaml                        # info, servers, tags, security, paths map with $refs
├── paths/<tag>.yaml                 # one file per tag (≤ ~400 lines each, advisory)
├── components/schemas.yaml
├── components/responses.yaml        # shared ErrorBody etc.
└── components/security.yaml
```

Source layout decisions:
- Each `components/*.yaml` wraps under `{components: {<section>: {...}}}` so internal `#/components/schemas/X` refs resolve within the file. root.yaml refs each section via `./components/<section>.yaml#/components/<section>`.
- Paths files keep operations flat at top level. Cross-file refs to components use `../components/<section>.yaml#/components/<section>/...`. Redocly rewrites them back to bundled-doc form on bundle.

## Four recipes

Copy-paste-ready. After each, run `make openapi` before committing — it chains bundle, Redocly lint, `check_openapi_errors.py`, and `check_openapi_url_shape.py` (REST §1).

**Router ↔ openapi.json parity is reviewer-enforced** — there is no mechanical gate. When you add, rename, or remove a route, both surfaces must move in the same diff and the reviewer verifies it. See `docs/REST_API_DESIGN_GUIDELINES.md` §6.

### Rename a path (e.g. `/v1/external-agents` → `/v1/agent-keys`)

```bash
# 1. Find the owning path file.
grep -rl "external-agents:" public/openapi/paths/

# 2. Rename the path key inside that YAML file.
#    Edit public/openapi/paths/<tag>.yaml and change
#      /v1/external-agents:         →  /v1/agent-keys:

# 3. Update the $ref in root.yaml (the JSON-pointer fragment uses ~1 for /).
#    Edit public/openapi/root.yaml and change the corresponding line.

# 4. Rename the match() arm in src/http/router.zig (literal string + enum variant).

# 5. Verify.
make openapi
```

### Append a new path

```bash
# 1. Pick (or create) public/openapi/paths/<tag>.yaml. Aim ≤ 400 lines per file.
#    If <tag> is new, create the file AND add a top-level tag entry in root.yaml tags:[].

# 2. Add the operation block. Required fields per operation:
#       operationId, summary, tags, parameters (if any), responses.
#    For every 4xx/5xx response use:
#       $ref: ../components/responses.yaml#/components/responses/Error
#    This is enforced by scripts/check_openapi_errors.py — deviating will fail CI.

# 3. If you created a new paths/<tag>.yaml, wire a $ref in root.yaml:
#       /v1/new/path:
#         $ref: ./paths/<tag>.yaml#/~1v1~1new~1path

# 4. Add the dispatch in src/http/router.zig match().

# 5. Verify.
make openapi
```

### Remove a path

```bash
# 1. Delete the operation block in public/openapi/paths/<tag>.yaml.
#    If the file becomes empty, delete the file AND remove its $ref
#    from root.yaml (paths map and — if applicable — the top-level tag).

# 2. Remove the match() arm in src/http/router.zig. Delete the handler if
#    nothing else uses it.

# 3. Verify.
make openapi
```

### Update a description, summary, or parameter

```bash
# 1. Edit the field in public/openapi/paths/<tag>.yaml.

# 2. Re-bundle. Router changes are not required for content-only edits.
make openapi
```

## Cross-reference: agent-facing public surfaces

See [`../AGENTS.md`](../AGENTS.md) for the full index of machine-readable files usezombie publishes — `openapi.json`, `llms.txt`, `skill.md`, `agent-manifest.json`, `heartbeat`.

If a rename/add/remove changes the **operation surface**, update `public/llms.txt` and `public/skill.md` operation tables in the same commit so the agent-facing discovery docs stay in sync with the bundled OpenAPI.

## Related specs

- `docs/v2/done/P2_INFRA_M28_003_OPENAPI_TOOLING.md` — the spec that set up this tree and the CI gates.
- `docs/REST_API_DESIGN_GUIDELINES.md` §10a — the human-facing short version of this document.
