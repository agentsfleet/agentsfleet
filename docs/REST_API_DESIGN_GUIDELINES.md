# REST API Design Guidelines — `agentsfleet/agentsfleetd`

**Status:** Canonical instruction set. Read this before adding, modifying, or removing any HTTP endpoint.
**Trigger:** the global instruction `HTTP handler or OpenAPI changes → read docs/REST_API_DESIGN_GUIDELINES.md first` fires when the diff touches `src/http/handlers/**`, `public/openapi/**`, or any `route_*` file. If you're an agent reading this — you got here because that trigger fired. Follow this doc as a checklist, not as background reading.

This is a goal-oriented instruction set. Each rule states the goal it serves so you can apply judgment at the edge cases instead of memorizing exceptions.

---

## Audience — who this binds

This doc binds **whoever ships the change** — Kishore directly, OR an agent acting on Kishore's behalf in auto mode under a start-instruction (per `~/.claude/CLAUDE.md` autonomy rules). Concretely:

- **The agent** runs the Quick checklist below as part of `CHORE(close)` and the §10 pre-PR gate, opens the PR via `gh pr create`, and answers `kishore-babysit-prs` review feedback. The agent is the primary enforcer.
- **Kishore** opens the PR directly when working without an agent. Same checklist applies.
- **A reviewer** (human or `/review`/`/review-pr`) checks the PR against this doc adversarially. A red box on the checklist that's not justified in the PR description is grounds to block merge.

When this doc says "you" it means the agent or the human author — same rules either way. When it says "MUST" it means a missing or wrong implementation blocks merge; "SHOULD" means deviate only with a one-line rationale in the PR description.

---

## Quick checklist — adding an endpoint

Run this checklist as part of `CHORE(close)` (per `~/.claude/CLAUDE.md` lifecycle), before `gh pr create`. Every box must be checked, OR the PR description must call out the deviation with a reason. An unchecked box that the description ignores blocks merge.

- [ ] **URL design** — plural noun resource, hierarchical path, no verbs (§1); operation-style `:verb` declared as one of the three allowed categories (§1)
- [ ] **Path params + trailing slash** — `{resource_id}` matches body field name; no trailing slash (§1)
- [ ] **HTTP method** chosen by semantics; PATCH idempotency contract stated; `Idempotency-Key` honored if applicable (§2)
- [ ] **Long-running ops** use the canonical `202 + /v1/operations/{id}` shape (§2)
- [ ] **Request body shape** matches the path (no path-param IDs in body) (§3)
- [ ] **Pagination** uses Stripe-style `?starting_after=&limit=` with `next_cursor` response field; default 50, max 100 (§3)
- [ ] **List envelope** is exactly `{items, total: int|null, next_cursor: string|null}` — no synonyms (§3)
- [ ] **Bulk endpoints** use `207` with the canonical per-item shape (§3)
- [ ] **Null vs omit** — absent optionals omitted, `null` reserved for "explicitly cleared" (§3)
- [ ] **Datetime fields** are int64 epoch ms with `_at` suffix (NOT ISO 8601) (§4)
- [ ] **Duration fields** are integers with `_ms` or `_seconds` suffix; no bare `timeout`/`ttl`/`duration` (§4)
- [ ] **Status codes** — 409 includes `current_state`; 412 includes `etag`; 429 includes `Retry-After` + `X-RateLimit-*` (§4)
- [ ] **ETag/`If-Match`** wired for any resource with realistic concurrent edits (§4)
- [ ] **Error responses** use the registry; `detail` follows hygiene rules (no IDs, no SQL, no paths, ≤200 chars) (§5)
- [ ] **OpenAPI YAML** edited under `public/openapi/paths/<tag>.yaml`; tag is 1:1 with resource; file ≤400 lines (§6)
- [ ] **Route registered** in all five places (§7)
- [ ] **Handler signature** matches `inner*(hx: Hx, req: *httpz.Request, ...)` (§8)
- [ ] **Middleware policy** picked from the table; raw handlers carry first-10-lines comment (§7)
- [ ] **Versioning** — added/renamed/removed surface listed in PR description; deprecation uses `Deprecation` + `Sunset` headers; new response fields declare `x-stability` (§9)
- [ ] **Tests** — happy path + one error per `hx.fail` + idempotency double-PATCH + `Idempotency-Key` replay (where applicable) + ETag mismatch (§10)
- [ ] **Logging** — sensitive ID values are DEBUG-only or carry `// log-id-allowed:` comment; secret-shaped fields are write-only or one-time-read (§11)
- [ ] **`make openapi` clean** — bundle in sync, redocly lint passes (§6)
- [ ] **`make test-auth` 200/200** — auth gate matrix unchanged (§10)
- [ ] **No file over 350 lines** (§10)
- [ ] **`gitleaks detect` clean** (§10)

If you're skimming, the load-bearing sections for handler authoring are §7 (route registration) and §8 (handler signature). Read those carefully every time.

---

## §1 — URL design

### Use plural nouns for resources

```
GET    /products              ← collection
GET    /products/{id}         ← single resource
POST   /products              ← create
PATCH  /products/{id}         ← partial update
DELETE /products/{id}         ← remove
```

NOT `/getProducts`, `/createProduct`, `/product`.

**Goal:** RESTful conventions; OpenAPI generators (Stainless, Speakeasy, OpenAPI Generator) produce clean SDKs from this shape.

### Reflect ownership in the path

```
GET /workspaces/{ws_id}/agents/{agent_id}/events
POST /workspaces/{ws_id}/agents
```

Nested paths express the containment relationship. Don't flatten — `/agents?workspace_id=...` loses the rule that the agent belongs to that workspace.

### No verbs in URLs

- Bad: `/users/{id}/activate`, `/agents/{id}/start`
- Good: `PATCH /users/{id}` body `{status: "active"}`, `POST /agents/{id}/events` (event resource creation)

**Operation-style endpoints** — the `POST /v1/.../approvals/{gate_id}:approve` colon form is allowed ONLY when the action falls into one of these three categories, named explicitly in the OpenAPI `description`:

1. **Idempotent retry** — re-running converges to the same end state (e.g. `:retry`, `:resync`).
2. **Side-effecting RPC** — the action sends/produces something the caller can't undo by editing a state field (e.g. `:send`, `:rotate`, `:revoke`).
3. **Multi-resource transaction** — the action atomically touches resources beyond the path's primary resource.

Anything else MUST be modeled as `PATCH /resource/{id}` with a state field. "Convenience" is not a category. If you can't pick one of the three in one sentence, you don't have an operation.

**Collision check** — before adding `POST /v1/.../{id}:verb`, grep the same resource's schema in `public/openapi/components/schemas.yaml` for an existing state field (`status`, `state`, `phase`, `lifecycle_state`). If one exists AND the new `:verb` would set it to a value that field can already hold, the operation is forbidden — use `PATCH /resource/{id}` body `{status: "<verb>"}` instead. Adding `:approve` when `status` already has an `approved` value is the canonical anti-pattern. The PR description MUST state the result of this grep ("no `status` field on `Approval`" or "`Approval.status` exists but `approved` is not a settable value via PATCH because <reason>").

### Path-param naming consistency

`{agent_id}` not `{id}`, not `{agentId}`, not `{aid}`. The path-param name matches the field name in the resource body. One spelling per concept across the whole API.

### Trailing slash

URLs are canonical without a trailing slash: `/v1/agents`, not `/v1/agents/`. Requests with a trailing slash MUST `308 Permanent Redirect` to the canonical form (preserves method + body). No silent 404, no silent rewrite.

### Tag mapping is 1:1 with resource

Every top-level resource maps to exactly one OpenAPI tag, and each tag maps to one `paths/<tag>.yaml` file. Don't split a resource across multiple tags ("Agents" + "AgentEvents"); don't merge resources under one tag ("Workspaces and Agents"). Sub-resources live under the parent tag's file unless that file hits the §6 400-line cap, at which point you split by sub-resource and document the split in `root.yaml`.

### Resource ID lives in path, not body

```http
PATCH /products/123
body: { "name": "new name" }    ← do NOT include "id": 123 in body
```

If the parent ID is in the path, never repeat it in the body.

### Field naming (Microsoft-aligned)

- **Plural for collections, singular for items**
- **`_id` suffix** for identifiers (`product_id`, `agent_id`) — never bare `id` except as the resource's own primary key field
- **`_at` suffix** for datetimes; values are int64 epoch milliseconds (§4)
- **Unit suffix** for durations: `_ms` or `_seconds` (§4) — never bare `timeout`/`ttl`/`interval`/`duration`/`expiration`
- **Adjectives before nouns** (`completed_items`, not `items_completed`)
- **No `is_` prefix** on booleans (`enabled`, not `is_enabled`)
- **Include units** in any quantitative field (`size_in_bytes`, `wall_ms`, `expiration_days`)
- **Banned field names** (not "avoid" — banned): `data`, `payload`, `object`, `response`, `result`, `value`, `info`, `metadata` as standalone field names. They convey nothing and break SDK ergonomics. Use a domain-specific noun.
- **Avoid** brand names and reserved words

---

## §2 — HTTP method semantics

| Method | Use for | Idempotent |
|--------|---------|------------|
| `GET` | Retrieve resource(s) | Yes |
| `POST` | Create new resource (server assigns ID) OR operation-style endpoint | No |
| `PUT` | Replace a resource fully (or upsert when client supplies the ID) | Yes |
| `PATCH` | Partially update a resource | Default: yes. State the contract in the spec. |
| `DELETE` | Remove a resource | Yes (idempotent: 204 on already-deleted) |

**PATCH idempotency contract** — PATCH MUST be idempotent unless the spec's "Failure Modes" section explicitly declares otherwise with a reason. Idempotent PATCH means: issuing the same body twice in succession produces identical row state and identical 200 responses. A negative test that issues the same PATCH twice and asserts equality is required (§10).

**Idempotency keys for non-idempotent POSTs.** Any POST that creates a billable, externally-visible, or side-effecting resource MUST accept an `Idempotency-Key` request header (UUIDv7 from the client). The server stores `(workspace_id, key) → response` for at least 24 hours and replays the prior response on duplicate. Endpoints exempt: pure RPC reads, internal-only POSTs. List which endpoints honor the key in the OpenAPI description.

**When in doubt:** if the client supplies the ID and the body fully describes the resource, use PUT. If the server assigns the ID, use POST. If only some fields change, use PATCH.

### Long-running operations

Endpoints that can't complete inside the request window MUST follow this convention — no per-endpoint reinvention:

1. The kicking POST returns `202 Accepted` with `Location: /v1/operations/{operation_id}` and a body containing `operation_id` and `status: "pending"`.
2. `GET /v1/operations/{operation_id}` returns:
   ```json
   { "operation_id": "...", "done": false, "status": "running", "started_at": 1735689600000 }
   ```
   When `done: true`, the body carries either `result` (the final resource, inline) or `error` (an RFC 7807 envelope per §5).
3. Terminal states are `succeeded`, `failed`, `cancelled`. No others.

If you need a different shape, amend this doc first. Do not invent a parallel polling URL.

---

## §3 — Request body shape

### List response envelope

```json
{
  "items": [ {...}, {...} ],
  "total": 42,
  "next_cursor": "eyJ0Ijoi..."
}
```

The exact key set is `items`, `total`, `next_cursor` — no synonyms. `results`, `data`, `entries`, `nodes`, `records` are forbidden.

- `items` — always present, always an array. Empty result is `200 {"items": []}` — never `204`.
- `total` — always present as an `integer | null` field. `null` means "not computed" (page-bounded count would have been too expensive). The OpenAPI schema MUST declare `nullable: true`. Removing the field entirely is forbidden — it forces SDK consumers into branches.
- `next_cursor` — opaque string when more pages exist, `null` on the last page. Always present.

### Single resource response

The resource itself, no envelope:

```json
{
  "id": "01HZQ...",
  "name": "platform-ops",
  "created_at": 1735689600000
}
```

NOT `{ "data": { ... } }`. NOT `{ "resource": { ... } }`. The top-level response IS the resource.

### Filtering, sorting, pagination

```
GET /products?status=active&sort=-created_at&starting_after=01HZQ...&limit=50
```

- **Filtering grammar — exactly one form per case:**
  - **Equality:** `?status=active` (single value), `?status=active,paused` (multi-value, comma-separated, single key).
  - **Time ranges:** `?created_after=<ts_ms>&created_before=<ts_ms>`. Bracket grammar (`?created_at[gte]=...`) is forbidden.
  - **No boolean explosions.** Don't add `?include_x=true&include_y=true` — use `?include=x,y` with a documented enum of legal values, OR don't expose a knob.
- **Sorting:** `sort=field` ascending; `sort=-field` descending. Single sort key per request — no multi-key.
- **Pagination — Stripe-style keyset only.** Request: `?starting_after=<resource_id>&limit=<int>`. Response: `next_cursor: <resource_id> | null` (the field is named `next_cursor` even though the request param is `starting_after` — this matches the `parsePaginationParams` / `derivePaginationResult` helpers in `src/http/handlers/common.zig`). Default `limit=50`, max `limit=100`. To page forward, send the response's `next_cursor` value back as the next request's `starting_after`. **Forbidden:** page-based `?page=&page_size=` (the `api_keys/list.zig` shape is legacy — do not copy it); custom request-side `?cursor=` names (the `approvals/list.zig` keyset shape predates this rule — do not copy it). New endpoints MUST use the shared helper.
- **Sparse fieldsets / `?include=` / `?fields=`:** not supported in v1. If you need to slim a payload, design a smaller endpoint. Don't invent.

### Bulk operations

If an endpoint accepts a batch (e.g. `POST /agents/batch`), it MUST return `207 Multi-Status` with this exact shape — no per-endpoint reinvention:

```json
{
  "items": [
    { "id": "01HZQ...", "status": "succeeded" },
    { "id": "01HZR...", "status": "failed", "error": { "type": "...", "title": "...", "status": 409, "detail": "..." } }
  ]
}
```

Per-item `status` ∈ `{succeeded, failed, skipped}`. Per-item `error` follows the §5 RFC 7807 envelope (without `request_id` — that's on the outer response). Whole-request 4xx still applies for malformed batches.

### Null vs omitted fields

- **Optional fields the server doesn't have a value for: omit the key.** Don't serialize `null` for "not present."
- **`null` is reserved for "explicitly cleared."** A PATCH with `{ "label": null }` clears the label; a GET response containing `"label": null` means the row has been cleared.

This rule is binding on responses AND requests. SDK consumers branch on `field === undefined` vs `field === null` — getting it wrong breaks them silently.

### IDs

- Use **UUIDv7** for all externally-exposed IDs (sortable, time-encoded). See [uuid7.com](https://uuid7.com).
- Do not expose database serial integers.
- Sensitive IDs (workspace_id, agent_id) live in the path, not the body. Never log their values at INFO level (§11).

---

## §4 — Response shape

### Success — use `hx.ok`

```zig
hx.ok(.ok, .{ .agent_id = id, .status = "active" });
hx.ok(.created, .{ .agent_id = id, .key = raw_key });
hx.ok(.accepted, .{ .status = "accepted", .event_id = event_id });
```

The `Hx` helper writes the JSON response with the right status code and content-type. Never call `hx.res.json(...)` directly except for SSE streams (and Slack's "ack-and-drop" pattern).

### Status codes

| Code | When |
|------|------|
| `200` | Successful read or update |
| `201` | Resource created |
| `202` | Accepted for async processing |
| `204` | Successful delete or no-content op |
| `400` | Client error in request shape |
| `401` | Missing or invalid authentication |
| `403` | Authenticated but unauthorized |
| `404` | Resource not found |
| `409` | State-transition conflict — the resource's current state forbids the requested transition. Body MUST include `current_state` in `detail` or as an extension field. |
| `412` | Precondition failed — `If-Match` ETag mismatch. Body MUST include the current `etag` so the client can refetch. |
| `413` | Payload too large |
| `429` | Rate limited — MUST include `Retry-After` header (seconds, integer) and `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset` (epoch seconds). |
| `500` | Internal server error |

`409` and `412` are distinct: `409` is "the state of the resource forbids this"; `412` is "your version of the resource is stale." Don't merge them.

If you find yourself wanting another status code, check RFC 7231/7807 — don't invent one.

### Datetime fields

**Integer milliseconds since Unix epoch (UTC), serialized as JSON number, OpenAPI `type: integer, format: int64`.** Field name uses the `_at` suffix.

```json
"created_at": 1735689600000
```

NOT ISO 8601 strings, NOT seconds, NOT floats, NOT a string-encoded integer. This matches the codebase (`EXTRACT(EPOCH FROM created_at) * 1000` in handlers; `i64` in response structs; `format: int64` in `components/schemas.yaml`). Anyone proposing ISO 8601 string datetimes for a new endpoint is wrong — match the existing convention or amend this rule first.

When the field has not yet been set (e.g. an unrevoked key's `revoked_at`), serialize as JSON `null` and declare `nullable: true` in the schema.

### Duration / interval / timeout fields

**Integer with explicit unit suffix in the field name.** Allowed suffixes:

- `_ms` — milliseconds (default for sub-second to multi-second internal timeouts).
- `_seconds` — seconds (TTLs, intervals where Redis or another sub-system uses seconds natively).

Banned:

- Bare names: `timeout`, `ttl`, `interval`, `duration`, `expiration`. The unit MUST be in the name.
- ISO 8601 durations (`PT5M`, `PT1H30M`). Forbidden in both requests and responses.
- Floats. Use the smaller unit if you need sub-integer precision.
- Mixing units in a single endpoint (e.g. one field in `_ms`, a peer in `_seconds`) without a stated reason in the OpenAPI description.

Pick `_ms` unless the underlying system speaks seconds (Redis `EX`, JWT `exp`). When in doubt, `_ms`.

### ETags and optimistic concurrency

For any resource where concurrent edits are realistic (anything mutable that two principals can touch), the `GET` and the response of `PATCH`/`PUT` MUST include an `ETag` response header. The client MUST send `If-Match: <etag>` on subsequent `PATCH`/`PUT`/`DELETE`. Mismatch → `412`. Resources without realistic concurrent edits (workspace-private append-only logs, single-tenant config) may opt out — note that decision in the spec's "Failure Modes" section.

---

## §5 — Error handling

### Use the error registry

The error-code registry (`src/errors/error_entries.zig`) owns the HTTP status, RFC 7807 `title`, and `docs_uri`. Your handler supplies only the code and a human-readable `detail`:

```zig
hx.fail(ec.ERR_INVALID_REQUEST, "workspace_id must be a valid UUIDv7");
hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
hx.fail(ec.ERR_AGENT_NOT_FOUND, ec.MSG_AGENT_NOT_FOUND);
```

NOT `common.errorResponse(hx.res, "...", "...", req_id)`. NOT `hx.res.status = 400; hx.res.body = "{...}"`.

If you need an error code that doesn't exist:

1. Add it to the registry with status, title, docs_uri.
2. Add a corresponding test asserting the registry entry is reachable.
3. Document in your spec's "Error Contracts" table.

### Registry `title` style

Titles are **short imperative noun phrases, 2–5 words, sentence case, no trailing punctuation**. Match the existing codebase style:

- Good: `Invalid UUID canonical format`, `Database unavailable`, `Insufficient role`, `Agent name already exists`, `Invalid webhook signature`.
- Bad: `error: database is currently unavailable.` (sentence, lowercase, period), `WORKSPACE_NOT_FOUND` (screaming snake, that's the code not the title), `Something went wrong while processing your request` (vague + long).

The title must be safe to render verbatim in a UI toast — think "what would I show a user."

### `detail` field hygiene

`detail` is user-facing. It MUST follow these rules — not "should":

1. **Length:** ≤200 characters. If you need more, you're explaining implementation; cut it.
2. **Voice:** one sentence OR one fragment. Be consistent within a code: pick one shape for `ERR_INVALID_REQUEST` and stick to it.
3. **Audience:** a developer integrating the API. Not a database admin, not a Zig engineer.
4. **Templating:** `{s}` placeholders are allowed for **enumerable safe values** — role labels, sort options, supported services. `{s}` placeholders are forbidden for **entity values** — never interpolate a `workspace_id`, `agent_id`, `user_id`, email, IP, hostname, or any UUID into `detail`.
5. **Ban list — `detail` MUST NOT contain any of these substrings, even when paraphrased:**
   - Internal table/column names: `pg_`, `pg.`, table names from `schema/*.sql`, column names not exposed in the OpenAPI response.
   - SQL fragments: `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `WHERE`, `JOIN`, `CONSTRAINT`, `relation does not exist`, `duplicate key value violates`.
   - Code internals: `panic`, `unreachable`, `error.`, `.zig`, `src/`, `*.zig` line numbers, file paths starting with `/`, allocator names, struct names from non-public modules.
   - Stack traces, addresses (`0x...`), thread IDs, request-internal pointer values.
   - Secret-shaped strings: anything matching `sk_`, `pk_`, `Bearer `, `eyJ` (JWT), `op://`, hex blobs >16 chars.
   - Entity values: literal UUIDs, emails, raw bearer tokens, vault key names, file paths from the workspace.
6. **Acceptable shapes — match these patterns:**
   - Validation: `"<field> must <constraint>"` — `key_name must be 1-64 chars, alphanumeric + hyphen + underscore`.
   - Capability: `"<resource/action> <verb-form>"` — `Workspace access denied`, `Tenant context required`.
   - State: `"<noun> already exists"` / `"<noun> not found"` / `"<noun> expired"` — `Agent name already exists`, `token expired`.
   - Format help: `"<param>: use <format>"` — `invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)`.

When you write a new `hx.fail`, find the closest existing call site in `src/http/handlers/**` and copy its shape. Don't freelance.

### Internal 500s — direct calls

For DB / pool / operation failures, use the helpers directly:

```zig
common.internalDbUnavailable(hx.res, hx.req_id);   // pool.acquire failed
common.internalDbError(hx.res, hx.req_id);         // query failed
common.internalOperationError(hx.res, "detail", hx.req_id);
```

These are NOT wrapped on `Hx`.

### Error response body (RFC 7807)

```json
{
  "type": "https://docs.agentsfleet.net/errors/invalid-request",
  "title": "Invalid request",
  "status": 400,
  "detail": "workspace_id must be a valid UUIDv7",
  "request_id": "req_01HZQ..."
}
```

`request_id` (carried as `x-e2e-request-id` header on the response) is required for traceability.

**Status-specific extensions** — the envelope is open per RFC 7807 §3.2. Two extensions are mandatory at their corresponding statuses:

- `409` MUST include `current_state: <string>` naming the state that forbade the transition (e.g. `"current_state": "approved"` when the caller tried to approve an already-approved gate).
- `412` MUST include `etag: <string>` carrying the resource's current ETag so the client can refetch and retry.

Don't invent other extensions without amending this doc.

---

## §6 — OpenAPI editing

**`public/openapi.json` is a build artifact.** Never edit it directly. Edits get wiped on `make openapi` and CI's bundle-in-sync gate fails the PR.

The source of truth lives under `public/openapi/`:

```
public/openapi/
├── root.yaml                        # info, servers, tags, security, paths map with $refs
├── paths/<tag>.yaml                 # one file per tag, hard cap 400 lines — split by sub-resource when exceeded
└── components/
    ├── schemas.yaml
    ├── responses.yaml
    └── security.yaml
```

### Adding, renaming, or removing an endpoint

1. Edit the relevant YAML under `public/openapi/paths/<tag>.yaml`.
2. Add / rename / remove the corresponding `match()` arm in `src/http/router.zig`.
3. Run `make openapi` — bundles YAML → JSON, runs Redocly lint, runs `check_openapi_errors.py`, runs `check_openapi_url_shape.py` (REST §1).
4. Commit YAML + bundled JSON + `router.zig` together. Splitting these across commits leaves CI red.

**Router ↔ openapi.json parity is reviewer-enforced.** There is no mechanical gate cross-checking that every `router.match()` arm has a documented openapi path or vice versa. When you add, rename, or remove a route, both surfaces must move in the same diff and the reviewer must verify it. The previous Python parity gate (`audits/check_openapi_sync.py`) and its data file (`src/http/route_manifest.zig`) were retired in M61_002.

**Agent-edit recipe:** see `public/openapi/AGENTS.md` for copy-paste-ready rename / append / remove / update-description workflows.

---

## §7 — Registering a route

Five places, in order. Steps 1–4 fail loudly at build/runtime; step 5 is reviewer-enforced — see §6 for the parity rule.

| Skipped step | Failure mode |
|---|---|
| 1 (Route enum) | Compile error in `route_table.zig::specFor()` — the `.my_endpoint` arm references an undefined enum variant. |
| 2 (`match()` arm) | Runtime — your URL returns 404 even though the rest is wired; no compile error. |
| 3 (`route_table.zig::specFor()`) | Compile error — exhaustive switch on `Route` enum is missing your arm. |
| 4 (`route_table_invoke.zig` shim) | Compile error — `specFor` references `invoke.invokeMyEndpoint` which doesn't exist. |
| 5 (OpenAPI YAML) | **No automated check.** Router ↔ openapi.json parity is reviewer-enforced (§6). Reviewer must confirm both surfaces moved in the same diff. |

If you only see #2's silent-404 failure mode, you've forgotten the matcher even though the route compiles. Test the URL after wiring.

1. **`src/http/router.zig`** — add a variant to the `Route` enum (with path params).
2. **`src/http/router.zig::match()`** — add the path parser that returns your variant.
3. **`src/http/route_table.zig::specFor()`** — map the variant to a `RouteSpec`:

   ```zig
   .my_endpoint => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMyEndpoint },
   ```

4. **`src/http/route_table_invoke.zig`** — add the invoke shim:

   ```zig
   pub fn invokeMyEndpoint(hx: *Hx, req: *httpz.Request, route: router.Route) void {
       if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
       my_handler.innerMyEndpoint(hx.*, req, route.my_endpoint);
   }
   ```

5. **`public/openapi/paths/<tag>.yaml`** — add the endpoint (§6).

### Matcher style — segment-based, not substring-based

All path matchers in `src/http/route_matchers.zig` operate on a canonical
`Path` view: a stack-allocated array of segments parsed once at the dispatch
boundary. Matchers compare by **segment count + segment[i] equality**.
Disambiguation is shape-driven, not order-driven.

**Pattern (mandatory for any new matcher):**

```zig
pub fn matchMyEndpoint(p: Path) ?MyRoute {
    if (p.segs.len != N) return null;        // exact segment count
    if (!p.eq(0, "literal-1")) return null;  // literal slots
    if (!p.eq(2, "literal-2")) return null;
    const id_a = p.param(1) orelse return null;  // path-param slots: param() rejects empty
    const id_b = p.param(3) orelse return null;
    return .{ .field_a = id_a, .field_b = id_b };
}
```

**Banned in new matchers:**
- `std.mem.startsWith` / `endsWith` / `indexOf` on the path string.
- Suffix-driven dispatch (`if endsWith(path, "/foo")`).
- Implicit segment boundaries via offset arithmetic.
- Reservations encoded as call-site ordering ("must run matcher A before B").

**Required properties of a matcher set:**
- **Mutual exclusivity by structure.** Any two matchers must be reject-incompatible
  by segment count or segment-equality predicates. If two matchers can both
  fire on the same path, one of them needs a tighter predicate (e.g. an explicit
  `if (p.eq(i, RESERVED)) return null` exclusion). Order in `match()` must not
  decide correctness.
- **Empty-segment safety.** `Path.parse` preserves empty segments from `//` and
  trailing slashes. Matchers extract path-parameter slots via `param(idx)`,
  which returns null on empty — empty IDs reject at the matcher, not the
  handler.
- **Semantic-named typed structs.** Each `Route` enum variant gets its own
  struct with semantic field names (`credential_name`, `agent_id`, `grant_id`,
  `memory_key`, `gate_id`, …). Parsing logic may be shared via a private
  helper, but the public surface stays type-distinct.

**Single API-version site.** The dispatcher in `router.zig::match()` calls
`Path.parse(...)` once, checks `segs[0]` against the API version, then hands
`p.tail(1)` to the version-specific matchers (`matchV1`, future `matchV2`).
The string `"v1"` lives in exactly one place — that dispatch line. No matcher
body checks the API version.

### Middleware policy table — pick one at step 3

| Policy | Use for |
|--------|---------|
| `auth_mw.MiddlewareRegistry.none` | Public endpoint; no auth, OR handler does its own (OAuth callbacks, webhook receivers) |
| `registry.bearer()` | Standard user-facing endpoint; workspace-scoped Bearer token |
| `registry.admin()` | Admin-only (internal telemetry, platform keys) |
| `registry.operator()` | Operator role required |
| `registry.webhookHmac()` | Approval webhook (HMAC-signed body) |
| `registry.slack()` | Slack events/interactions (Slack signature) |

Middleware runs BEFORE your handler: token verification, `hx.principal` population, and 401/403 short-circuits are already done when your `inner*` runs. For `none` policy routes, `hx.principal` is zero-valued — do not read it.

### Raw-handler exceptions

Some endpoints bypass `authenticated()`. Each must carry an explanatory comment at the top of the file:

| File | Reason | Comment |
|------|--------|---------|
| `webhooks.zig` | HMAC-verified, not Bearer | `// HMAC-verified — does not use hx.authenticated()` |
| `github_callback.zig` | OAuth callback | `// OAuth callback — does not use hx.authenticated()` |
| `health.zig` | Health endpoints | `// unauthenticated — health` |
| `agent_relay.zig`, `runs/stream.zig` | Streaming | `// Streaming handler — does not use hx.authenticated()` |
| `auth_sessions_http.zig` | Per-function (login/poll public, complete authed) | per-function comments |

If you write a new raw handler, add it to this table AND add a first-10-lines comment in the file explaining why it bypasses `hx.authenticated()`. A handler file lacking either an `hx.authenticated()` call OR an explanatory comment in its first 10 lines is a bug — not an oversight.

### Reference implementations

When in doubt, mirror an existing handler:

| Pattern | Look at |
|---------|---------|
| Simple list/get | `agent_api.zig::innerListAgents` |
| POST with body + validation | `agent_api.zig::innerCreateAgent` |
| DELETE with idempotency | `agent_api.zig::innerDeleteAgent` |
| Admin-only | `admin_platform_keys_http.zig::innerGetAdminPlatformKeys` |
| Multi-method on one path (GET/PUT/DELETE) | `workspace_credentials_http.zig` |
| Workspace auth + tenant context | `agent_telemetry.zig::innerAgentTelemetry` |
| Webhook with inline auth | `webhooks.zig::innerReceiveWebhook` |
| Streaming (SSE) | `agent_relay.zig::innerRelay` |

---

## §8 — Handler signature contract

Every HTTP handler in `agentsfleetd` follows this shape. Enforced by `make lint` and the comptime wrappers in `src/http/handlers/hx.zig`.

```zig
pub fn innerMyEndpoint(hx: Hx, req: *httpz.Request, ...path_params) void {
    // 1. validate inputs
    // 2. db / state work (use hx.db()/hx.releaseDb())
    // 3. respond via hx.ok(...) or hx.fail(...)
}
```

### Rules

- **Name prefix `inner`** (never `handle`). The `invokeXxx` shim in `route_table_invoke.zig` is the only public entry — your function is the inner implementation it calls after middleware has populated `hx`.
- **First parameter: `hx: Hx`** — never `ctx: *Context`, `res: *Response`, `req_id: []const u8`, or an arena allocator. All of those live inside `hx`.
- **Second parameter: `req: *httpz.Request`** — only if you actually read it (body, query, headers). Drop it if you only need path params.
- **Path params come after `req`** — order matches the `Route` enum variant in `router.zig`.
- **Return `void`.** Errors are written to the response; never return a Zig error.

### What NOT to do

- ❌ `handleMyEndpoint(ctx, req, res)` — old signature; don't add new ones.
- ❌ Building an `ArenaAllocator` inside the handler — `hx.alloc` is already request-scoped.
- ❌ Calling `common.authenticate` — middleware already did this; use `hx.principal`.
- ❌ `common.errorResponse(hx.res, ...)` — use `hx.fail(...)`.
- ❌ `hx.res.json(body, .{})` or `res.status = 200; res.body = "{}"` — use `hx.ok(.ok, body)`. Acceptable only for SSE streams and Slack ack-and-drop.
- ❌ Returning a Zig error from the handler — write to `hx.res` and return void.

### `Hx` struct reference

`Hx` is defined in `src/http/handlers/hx.zig`. Five pointer-sized fields, passed by value:

```zig
pub const Hx = struct {
    alloc:     std.mem.Allocator,       // request-scoped arena
    principal: common.AuthPrincipal,    // populated by bearer/admin middleware
    req_id:    []const u8,              // unique request ID
    ctx:       *common.Context,         // pool, queue, oidc, telemetry, app_url
    res:       *httpz.Response,         // response writer

    pub fn db(self: Hx) !*pg.Conn;             // acquire from pool; caller must releaseDb
    pub fn releaseDb(self: Hx, conn: *pg.Conn) void;
    pub fn redis(self: Hx) *queue_redis.Client; // no allocation
    pub fn ok(self: Hx, status: std.http.Status, body: anytype) void;
    pub fn fail(self: Hx, code: []const u8, detail: []const u8) void;
};
```

Comptime wrappers used to register handlers:

| Wrapper | When |
|---------|------|
| `authenticated(inner)` | Standard Bearer-authed handler with no path params |
| `authenticatedWithParam(inner)` | Handler with one path param (e.g. `agent_id`) |

Handlers with two or more path params (e.g. `webhooks.zig`'s `agent_id` + `url_secret`) do NOT use the wrappers — they stay raw and call `common.authenticate` directly with their own logic.

### Guardrails on `hx.zig`

- `hx.zig` must stay ≤ 150 lines. Methods are one-liners delegating to `common.zig`. No business logic in `hx.zig`.
- A new method on `Hx` is justified only when ≥2 in-tree call sites exist **at merge time** — both verifiable by `rg` from `main`. The PR description MUST list both call sites as `path/to/file.zig:line`. "Two handlers will need this soon" is not a justification; ship the second caller in the same PR or keep the helper local.
- Zero `ArenaAllocator.init` calls in converted handler files. All arena setup lives inside the comptime wrappers.

---

## §9 — Versioning

URI-based: `/v1/...`. All current endpoints sit under `/v1`. Bump to `/v2` only when a breaking change is unavoidable; default to additive evolution within `/v1`.

Header-based versioning (e.g. `X-API-Version: 2026-04-25`) is NOT used in this project. Don't introduce it.

### What is additive vs breaking

| Change | Class | Allowed within `/v1`? |
|---|---|---|
| New endpoint | Additive | Yes |
| New optional request field | Additive | Yes |
| New response field | Additive | Yes — but mark its stability class (below) |
| Tightening validation (narrower regex, lower max length) | Breaking | No |
| Loosening validation (wider regex, higher max length) | Additive | Yes |
| Adding a new enum value to a request param | Additive | Yes |
| Adding a new enum value to a response field | Breaking for typed SDK consumers — see "Enum extension" below |
| Renaming a field (request or response) | Breaking | No |
| Removing a field | Breaking | No — deprecate first |
| Changing a field's type or unit | Breaking | No |
| Making an optional field required | Breaking | No |
| Making a required field optional | Additive | Yes |
| Changing default pagination `limit` | Breaking (silently changes per-page billing/UI) | No |
| Adding a new error code to an existing endpoint | Additive | Yes |
| Changing a 4xx status code for an existing failure mode | Breaking | No |

### No field renames within `/v1`

Once a field name is exposed, it's immortal until `/v2`. To evolve, add the new field, populate both, mark the old one deprecated (see below), and remove only at the next major bump. Same rule for path-param names and query-param names.

### Enum extension policy

Adding a value to a response enum is a **breaking change for typed SDK consumers** (Go, Rust, TypeScript SDKs generate exhaustive switches; a new value triggers compile errors or silent fallthrough). Two options:

1. **Defer to `/v2`** — preferred when the new value changes semantics meaningfully.
2. **Add as a `additional_*` companion field** — leave the typed enum frozen, add `additional_status: string` for the new value. Document in the OpenAPI description.

If you ship a new enum value inside `/v1`, the PR description MUST acknowledge the SDK regeneration impact and name the consumers that will need a release.

### Deprecation

When deprecating an endpoint or field:

1. Set the response header `Deprecation: true` and `Sunset: <RFC 1123 date>` on every response from the deprecated endpoint (or every response that includes the deprecated field).
2. Minimum 90-day clock between announcing deprecation and removing the endpoint/field. 180 days for anything an external SDK consumer touches.
3. Add a `Link: <docs-url>; rel="deprecation"` header pointing to the migration guide.
4. Document the deprecation in the OpenAPI YAML with `deprecated: true` AND a one-line migration hint in the description.

No silent removals. No removals that skip the `Deprecation` header phase.

### Response field stability classes

Every response field has one of three classes, declared via OpenAPI extension `x-stability`:

- `stable` — covered by the deprecation policy above. Default.
- `beta` — may change shape or be removed without the 90-day clock; flagged as `beta` in the OpenAPI description; SDK generators may strip or wrap as optional.
- `internal` — present in responses for tooling/debugging; SDK generators MUST strip; no compatibility guarantee. Avoid leaking these — internal fields tend to leak production state.

Every new response field added in a PR MUST declare a class. Default is `stable` if you forget, which is binding — the deprecation clock starts the moment it ships.

### PR-level surface diff

Every PR that changes the HTTP surface MUST open its description with a "Surface diff" section listing added / renamed / removed `(method, path)` pairs and added / renamed / removed schema fields. This is how reviewers and the babysit-prs loop detect breaking changes that the diff itself buries.

---

## §10 — Pre-PR checklist (testing + ship)

Before opening a PR touching any handler:

- [ ] `zig build` clean
- [ ] `zig build test` passes
- [ ] `make test-auth` passes (200/200) — auth gate matrix unchanged
- [ ] `make test-integration` passes (HTTP + DB + Redis E2E)
- [ ] Cross-compile: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make lint` — all Zig gates pass (350-line file cap, `check-pg-drain`, zlint)
- [ ] Handler file ≤ 350 lines; split if it grows
- [ ] Integration test covers the happy path AND at least one error path per `hx.fail` call in the handler
- [ ] PATCH endpoints have an idempotency test: same body issued twice → identical 200 + identical row state. Skip only if the spec's "Failure Modes" explicitly declares non-idempotent PATCH with a reason (§2)
- [ ] POST endpoints accepting `Idempotency-Key` have a replay test: same key + same body → cached response; same key + different body → 4xx (§2)
- [ ] Mutable resources have an ETag/`If-Match` test: stale `If-Match` → 412 with current `etag` returned (§4)
- [ ] OpenAPI updated — endpoint definition, request/response schemas, error responses
- [ ] `make openapi` — bundle in sync, redocly lint, router parity
- [ ] `make check-openapi-errors` — every `hx.fail` code is in the OpenAPI's error responses
- [ ] `gitleaks detect` clean
- [ ] No new file over 350 lines

---

## §11 — Security

- Use HTTPS for all endpoints. The Cloudflare Tunnel layer (see `playbooks/ARCHITECTURE.md`) enforces this for public ingress.
- Bearer tokens for user-facing endpoints. OAuth (Clerk) for the auth-issuance flow.
- Sensitive IDs (workspace_id, agent_id, user_id) live in the path, not the body. **Never log their literal values at INFO or above.** A log call whose format string OR struct argument references one of these IDs MUST be `DEBUG` or below, OR carry a same-line comment `// log-id-allowed: <reason>` explaining why this specific call is safe (e.g. it's a hashed prefix, not the raw value).
- Use **UUIDv7** for all IDs.
- **Secret-shaped response fields are write-only or one-time-read.** Any field whose name contains `token`, `secret`, `key`, `password`, `credential` (and is not just a key-by-name like `key_name`) MUST either be: (a) write-only — never returned by GET, only echoed in the POST that creates it, or (b) one-time-read — returned in the create response and never again. Document which in the OpenAPI description with `x-secret-handling: write-only | one-time-read`. This includes raw API keys, OAuth tokens, HMAC shared secrets, and webhook signing secrets.
- Never log secret values. Substitution happens at the tool bridge inside the executor sandbox; tokens never enter the agent's context, the event log, or any handler-level log line. (See `docs/ARCHITECTURE.md` §10 for the substrate guarantees.)
- Webhook receivers verify HMAC signatures using `std.crypto.utils.timingSafeEql`. Never string-compare HMACs.
- **CORS posture is per-endpoint and explicit.** Endpoints intended for browser callers (the dashboard at `app.agentsfleet.net`) MUST declare their allowed origin in the spec; everything else MUST refuse cross-origin requests at the edge. Do not enable wildcard `Access-Control-Allow-Origin: *`. Adding browser exposure to an existing endpoint is a surface change — call it out in the §9 PR-level surface diff.

---

## §12 — Performance

Targets (measured in CI bench under `make bench`):

- p99 latency < 200 ms
- p95 latency < 150 ms
- Zero allocator leaks (`std.testing.allocator` integration tests pass)
- No unbounded query loops. Pagination caps live in §3 (default `limit=50`, max `limit=100`).

If your endpoint can't meet these in a normal load profile, the spec's "Performance Considerations" section MUST contain ALL of:

1. **Endpoint** — exact `(method, path)`.
2. **Measured numbers** — p50, p95, p99 from a representative load run; cite the bench command and config used.
3. **Load profile** — qps, payload size, concurrency, dataset size that produced the numbers.
4. **Reason** — what makes this endpoint structurally slower (e.g. unavoidable cross-region call, large aggregation, third-party API).
5. **Remediation milestone** — the spec ID where the carve-out is planned to be retired (`M{N}_{NNN}`), OR an explicit `accepted-permanent` with a justification reviewed by the user.

A one-line "this endpoint is slow because reasons" is not a carve-out. Without all five, the carve-out doesn't merge.

---

## §13 — Reference specifications

When the project's conventions don't cover a case, defer to (in priority order):

1. **This document** — project-specific overrides everything.
2. [Microsoft REST API Guidelines](https://github.com/microsoft/api-guidelines) — for naming + pagination semantics.
3. [Google API Design Guide](https://cloud.google.com/apis/design) — for resource modeling.
4. [GitHub REST API Docs](https://docs.github.com/en/rest) — for query-param + filtering patterns.
5. [OpenAPI Specification](https://swagger.io/specification/) — schema validity.
6. [RFC 7807](https://www.rfc-editor.org/rfc/rfc7807) — error response envelope.
7. [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) — newer Problem Details (informational).

If a project rule conflicts with an external guide, the project rule wins; document the conflict in this file.

---

## SDK generation

This API is designed for OpenAPI-driven SDK generation. SDK generators tested against the OpenAPI bundle:

- [Stainless](https://stainlessapi.com/)
- [OpenAPI Generator](https://openapi-generator.tech/)
- [Speakeasy](https://speakeasy.com/)
- [APIMatic](https://www.apimatic.io/)

If you add a non-standard pattern (e.g., a polymorphic response shape) without checking SDK generator output, you risk breaking client codegen on the next bundle. When in doubt, mirror an existing endpoint that's known-clean under codegen.
