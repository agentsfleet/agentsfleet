# REST API Design Guidelines — `agentsfleet/agentsfleetd`

**Status:** Canonical instruction set. Read this before adding, modifying, or removing any HTTP endpoint.
**Trigger:** the global instruction `HTTP handler or OpenAPI changes → read docs/REST_API_DESIGN_GUIDELINES.md first` fires when the diff touches a handler, `public/openapi.json`, or any `route_*` file. If you're an agent reading this — you got here because that trigger fired. Follow this doc as a checklist, not as background reading.

This is a goal-oriented instruction set. Each rule states the goal it serves so you can apply judgment at the edge cases instead of memorizing exceptions.

---

## Audience — who this binds

This doc binds **whoever ships the change** — Kishore directly, OR an agent acting on Kishore's behalf in auto mode under a start-instruction (per `~/.claude/CLAUDE.md` autonomy rules). Concretely:

- **The agent** runs the Quick checklist below as part of `CHORE(close)` and the §10 pre-PR gate, opens the PR via `gh pr create`, and answers `orly-babysit-prs` review feedback. The agent is the primary enforcer.
- **Kishore** opens the PR directly when working without an agent. Same checklist applies.
- **A reviewer** (human or `/review`) checks the PR against this doc adversarially. A red box on the checklist that's not justified in the PR description is grounds to block merge.

When this doc says "you" it means the agent or the human author — same rules either way. When it says "MUST" it means a missing or wrong implementation blocks merge; "SHOULD" means deviate only with a one-line rationale in the PR description.

---

## Quick checklist — adding an endpoint

Run this checklist as part of `CHORE(close)` (per `~/.claude/CLAUDE.md` lifecycle), before `gh pr create`. Every box must be checked, OR the PR description must call out the deviation with a reason. An unchecked box that the description ignores blocks merge.

- [ ] **URL design** — plural noun resource, hierarchical path, no verbs (§1); operation-style `:verb` declared as one of the three allowed categories (§1)
- [ ] **Path params + trailing slash** — `{resource_id}` matches body field name; no trailing slash (§1)
- [ ] **HTTP method** chosen by semantics; PATCH idempotency guarantee stated; `Idempotency-Key` honored if applicable (§2)
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
- [ ] **OpenAPI document** regenerated from the build, not hand-edited; the coverage gate is green (§6)
- [ ] **Route registered** in all six places (§7)
- [ ] **Handler signature** takes only the extractors it reads and returns `Result<Response, Refusal>` (§8)
- [ ] **Middleware policy** picked from the table; raw handlers carry first-10-lines comment (§7)
- [ ] **Versioning** — added/renamed/removed surface listed in PR description; deprecation uses `Deprecation` + `Sunset` headers; new response fields declare `x-stability` (§9)
- [ ] **Tests** — happy path + one error per refusal + idempotency double-PATCH + `Idempotency-Key` replay (where applicable) + ETag mismatch (§10)
- [ ] **Logging** — sensitive ID values are DEBUG-only or carry `// log-id-allowed:` comment; secret-shaped fields are write-only or one-time-read (§11)
- [ ] **The repository's declared `verify.unit` command is clean** — the route-scope table's exhaustive match and its tests cover the auth gate matrix (§10)
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

**Collision check** — before adding `POST /v1/.../{id}:verb`, grep the same resource's schema in `public/openapi.json` for an existing lifecycle field (`status`, `state`, `stage`, `lifecycle_state`). If one exists AND the new `:verb` would set it to a value that field can already hold, the operation is forbidden — use `PATCH /resource/{id}` body `{status: "<verb>"}` instead. Adding `:approve` when `status` already has an `approved` value is the canonical anti-pattern. The PR description MUST state the result of this grep ("no `status` field on `Approval`" or "`Approval.status` exists but `approved` is not a settable value via PATCH because <reason>").

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
| `PATCH` | Partially update a resource | Default: yes. State the guarantee in the spec. |
| `DELETE` | Remove a resource | Yes (idempotent: 204 on already-deleted) |

**PATCH idempotency guarantee** — PATCH MUST be idempotent unless the spec's "Failure Modes" section explicitly declares otherwise with a reason. Idempotent PATCH means: issuing the same body twice in succession produces identical row state and identical 200 responses. A negative test that issues the same PATCH twice and asserts equality is required (§10).

**Idempotency keys for non-idempotent POSTs.** Any POST that creates a billable, externally-visible, or side-effecting resource MUST accept an `Idempotency-Key` request header (UUIDv7 from the client). The server stores `(workspace_id, key) → response` for at least 24 hours and replays the prior response on duplicate. Endpoints exempt: pure RPC reads, internal-only POSTs. List which endpoints honor the key in the OpenAPI description.

**Owner-approved workspace exception.** `POST /v1/workspaces` accepts no replay
key. It requires a tenant-unique name and performs one tenant-scoped insert with
a server-assigned ID. After an uncertain browser response, the dashboard
refreshes every cursor page from `GET /v1/tenants/me/workspaces`; the
command-line interface performs one exact-name GET and does not retry the POST.
A deliberate same-name retry returns 409 and cannot create a second row. Do not
generalize this exception to other POST endpoints.

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

Security-bound exception: `GET /v1/tenants/me/workspaces` also returns the
authoritative `tenant_id`. Browser and command-line clients persist that value
with the workspace list so a refreshed identity cannot mix local state from
two tenants.

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
- **Pagination — Stripe-style keyset only.** Request: `?starting_after=<resource_id>&limit=<int>`. Response: `next_cursor: <resource_id> | null` (the field is named `next_cursor` even though the request param is `starting_after`). Cursor encode/decode goes through `afd_core::paging::cursor`, and both the parameter name and the bounds are constants there: `QUERY_STARTING_AFTER`, `DEFAULT_LIMIT` (50) and `MAX_LIMIT` (100). Read the limit through that module rather than parsing the query string in a handler. To page forward, send the response's `next_cursor` value back as the next request's `starting_after`. **Forbidden:** page-based `?page=&page_size=`, and custom request-side `?cursor=` names. Both spellings predate this rule where they survive; do not copy either into a new endpoint.
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

### Success — return the wire type

```rust
Ok(Json(FleetDetailResponse::from(&detail)).into_response())
Ok((StatusCode::CREATED, Json(CreatedResponse { id, key })).into_response())
Ok(StatusCode::NO_CONTENT.into_response())
```

The wire type owns the field names and `IntoResponse` writes the status and
content-type. A response shape belongs in `afd_wire`, not spelled inline in a
handler, so the same struct is what the OpenAPI document is generated from.
Build a response by hand only for a Server-Sent Events stream.

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

The error-code registry (`rustd/crates/afd_core/src/error_code.rs` and the family modules beside it) owns the HTTP status, RFC 7807 `title`, and `docs_uri`. Your handler supplies only the code and a human-readable `detail`:

```rust
Refusal::malformed(DETAIL_WORKSPACE_ID)          // the sentence is a constant
Refusal::coded(FLEET_NOT_FOUND, DETAIL_NOT_FOUND)
Refusal::preconditioned(error.code(), error.detail(), current)
```

Never assemble a `Problem` by hand and never write a status and body directly:
the status, title and documentation link come from the registry entry, and a
hand-built body is how two spellings of one error reach clients.

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
   - Code internals: `panic`, `unwrap`, `error.`, a source path, a file extension, a line number, file paths starting with `/`, allocator names, struct names from non-public modules.
   - Stack traces, addresses (`0x...`), thread IDs, request-internal pointer values.
   - Secret-shaped strings: anything matching `sk_`, `pk_`, `Bearer `, `eyJ` (JWT), `op://`, hex blobs >16 chars.
   - Entity values: literal UUIDs, emails, raw bearer tokens, vault key names, file paths from the workspace.
6. **Acceptable shapes — match these patterns:**
   - Validation: `"<field> must <constraint>"` — `key_name must be 1-64 chars, alphanumeric + hyphen + underscore`.
   - Capability: `"<resource/action> <verb-form>"` — `Workspace access denied`, `Tenant context required`.
   - State: `"<noun> already exists"` / `"<noun> not found"` / `"<noun> expired"` — `Agent name already exists`, `token expired`.
   - Format help: `"<param>: use <format>"` — `invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)`.

When you write a new refusal, find the closest existing call site under `rustd/crates/afd_api_*/src/handler/**` and copy its shape. Don't freelance.

### Internal 500s — direct calls

A datastore or service failure is not a sentence a handler writes. Lift it with
`Refusal::at`, naming the operation that failed:

```rust
services.fleet_detail(&context, fleet_id).await.map_err(Refusal::at(FLEET_READ))?
```

The operation constant is what identifies the failure in logs and telemetry;
the caller gets the registry's 500 and no internal detail.

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

**The document is generated. There is nothing to hand-edit.**

`public/openapi.json` is emitted from the daemon's own handlers — the route
table decides the paths and methods, `#[utoipa::path]` beside each handler
carries the prose and the status codes, and the `afd_wire` types carry the
schemas. The `public/openapi/` YAML tree this section used to describe was
retired with the Zig daemon; a hand-edit to the JSON is reverted by the next
regeneration and fails a test before it gets that far.

### Adding, renaming, or removing an endpoint

1. Add, rename or remove the [`Route`] variant and its template, and say which
   methods it answers in that family's `verbs()`.
2. Mount it in `rustd/crates/afd_api/src/router/mount.rs`.
3. Put a `#[cfg_attr(feature = "openapi", utoipa::path(…))]` on the handler and
   name it in that plane's `src/openapi.rs` collector.
4. Regenerate and commit the artifact with the code:

   ```bash
   cd rustd && cargo run -q -p agentsfleetd --features openapi \
     --bin agentsfleetd -- --no-banner openapi > ../public/openapi.json
   ```

**Parity is mechanical, not reviewer-enforced.** Three tests grade it, and the
first is the one that used to be a review obligation:

| Test | What it refuses |
|---|---|
| `test_coverage_gate_rust_source` | a served route with no annotation, or an annotation for a route nobody mounts — named with its method and direction |
| `test_openapi_build_is_the_source` | a committed artifact that is not what the build emits |
| `test_documented_codes_match_refusals` | an operation that omits a refusal its guard or scope rung guarantees |

The prose is graded too: `scripts/check_documentation_rules.py` reads the
generated document, so a description that breaks the wording rules fails
`make lint-all`. Fix it at the annotation — or, for a schema description, at the
`afd_wire` type's doc comment, which is what utoipa publishes. Rationale that is
for maintainers rather than for API consumers belongs in a `//` comment beside
it, which the document does not carry.

## §7 — Registering a route

**The router is built FROM the route table, not beside it.** `Route::all()` is
walked once at startup and every template it yields is mounted, so a path
cannot be served under a spelling the table does not carry, and it cannot be
served under a guard its own row does not declare. Adding an endpoint is
therefore a table edit plus a handler, in three places.

1. **`rustd/crates/afd_http/src/route/<family>.rs`** — add a variant to the
   family enum (`FleetRoute`, `TenantRoute`, `AdminRoute`, `WebhookRoute`,
   `WorkspaceRoute`, `ConnectorRoute`, `RunnerRoute`, `RunnerOpsRoute`,
   `AuthRoute`, `OpsRoute`) **and to that family's `ALL` array**. `ALL` is what
   `Route::all()` iterates; a variant missing from it compiles and is never
   mounted.

2. **The same file's `meta()`** — return a `RouteMeta` for the variant:

   ```rust
   Self::MyEndpoint => (
       RouteClass::Api,
       fleet_path!("/my-endpoint"),
       Scopes::rw(FLEET_READ, FLEET_WRITE),
   ),
   ```

   `RouteMeta::new(guard, class, template, scopes)` closes over four of the five
   facts; the fifth, `ownership`, is derived from the template by
   `Ownership::of` and is not yours to pass.

3. **The plane crate** — write the handler under
   `rustd/crates/afd_api_{tenant,operator,ingress,runner}/src/handler/**`, then
   return it from that plane's `*_handler_for` arm (`fleet_handler_for`,
   `admin_handler_for`, and their siblings in each plane's `lib.rs`).
   `afd_api/src/router/mount.rs` dispatches families to planes with a total
   match, so a new family cannot silently inherit another plane's
   authentication.

4. **`public/openapi.json`** — add `#[cfg_attr(feature = "openapi", utoipa::path(…))]`
   beside the handler and regenerate (§6).

| Skipped step | Failure mode |
|---|---|
| 1, the enum variant | Compile error — `meta()` is an exhaustive match over the family. |
| 1, the `ALL` entry | **Silent.** The route compiles, is never walked, and answers 404. Nothing fails. Test the URL after wiring. |
| 2 (`meta()` arm) | Compile error — the match is not total without it. |
| 3 (`*_handler_for` arm) | The route is tabled and unmounted, so it answers 404 by design. `handler_for` returning `None` is the honest answer for a verb this binary does not serve; a 501 would claim the endpoint exists here. |
| 4 (the document) | `test_coverage_gate_rust_source` fails, naming the route, the method and which side is missing it (§6). |

### Templates are low-cardinality, always

`RouteMeta.template` is the `http.route` span attribute as well as the mount
path. It carries `{workspace_id}`, `{fleet_id}` and their siblings as
parameters and never a concrete value — a real path would put tenant identity
into span attributes and give the tracing backend one route value per request.

`matchit`, the router underneath axum, **refuses a literal after a parameter
inside one segment**, so a custom verb cannot be spelled `{id}:sync`. Where the
Zig daemon and the shipped clients used that form, the Rust table spells it as
its own segment (`/schedules/{schedule_id}/sync`) and the change travels with
its clients in the same diff.

### The five facts a route row carries

| Fact | Type | What it decides |
|---|---|---|
| `guard` | `Guard` | What must be presented before a handler runs. |
| `class` | `RouteClass` | `Ops` never sheds, `Stream` is capped separately because one Server-Sent Events connection is held for minutes, `Api` is subject to the in-flight ceiling. |
| `template` | `&'static str` | The mount path and the `http.route` attribute. |
| `scopes` | `Scopes` | `Always(&[…])` for one requirement, or `ByMethod { get, otherwise }` where reads are cheaper than writes. `otherwise` is always the more privileged of the pair, so a method nobody considered can only be refused too often. |
| `ownership` | `Ownership` | Derived from the template. Which object named in the path must belong to the caller. |

**Ownership is the half of authorization a handler used to have to remember.**
Capability and ownership are independent questions — may you do this kind of
thing, and is this particular thing yours. The Zig daemon answered the first in
its route table and the second by calling `authorizeWorkspace` by hand at the
top of every workspace handler, where forgetting it was a cross-tenant read
with nothing failing. Here it is mounted from the route's own template, so
forgetting is not a thing a handler can do.

### Guard — pick one at step 2

| Guard | Use for |
|--------|---------|
| `Guard::Open` | A probe, or a route authenticated by its own payload: the device-flow session endpoints, the connector callback, every webhook. |
| `Guard::Bearer` | The standard tenant-plane credential — session bearer, `agt_t`, or `afc_`. The capability the principal must hold is the row's `scopes`, not a second guard. |
| `Guard::RunnerBearer` | A runner-plane credential (`agt_r`). Refused for tenant callers by `afd_auth::Plane`, which is data rather than which router mounted it. |
| `Guard::WebhookHmac` | An HMAC over the request body, keyed per fleet. |
| `Guard::WebhookSignature` | The per-fleet webhook signature header. |
| `Guard::Svix` | A Svix-signed delivery. |

The layers run outside your handler and in a fixed order: **admission**
outermost, because a shed must stay cheaper than the work it refuses;
**authentication and the capability gate** next, so a handler never runs for a
caller who should not reach it; **ownership** innermost, because it is the only
one of the three that runs a statement. A caller who is over the ceiling or
short a capability is refused before this daemon reaches Postgres for them.

Each layer is decided while the router is BUILT, not per request: a route that
is not metered has no admission layer in its stack to consult, and a route with
no guard has no authenticator in its stack to reach.

### `Guard::Open` routes verify their own callers

An open route is not an unauthenticated one, except for the probes. The proof
is the body or a signed parameter, which is why it cannot be checked before the
body is read — and why those routes carry a buffer cap the bearer-proven ones
do not need.

| Route family | What verifies the caller instead |
|------|------|
| `OpsRoute` (`/healthz`, `/readyz`) | Nothing. Unauthenticated by design — an orchestrator that cannot reach these has no other way to ask. |
| `AuthRoute::{CreateSession, PollSession, VerifySession}` | The device-flow code itself; approve and delete are `Guard::Bearer`. |
| `WebhookRoute`, every variant | A signature over the body — per-fleet HMAC, the fleet signature header, or Svix. |
| `ConnectorRoute` callback and events | A signed `state` parameter verified inline before the provider hook runs, or the provider's own request signature. |

A new `Guard::Open` route states in its module documentation what verifies the
caller instead. One that verifies nothing and says nothing is a bug, not an
oversight.

### Reference implementations

When in doubt, mirror an existing handler:

| Pattern | Look at |
|---------|---------|
| Read, edit and purge one resource | `rustd/crates/afd_api_tenant/src/handler/fleet/detail.rs` |
| Request parsing and its refusal sentences | `rustd/crates/afd_api_tenant/src/handler/fleet/detail_request.rs` |
| Keyset pagination | `rustd/crates/afd_api_tenant/src/handler/tenant/models.rs` |
| Platform plane, operator-held | `rustd/crates/afd_api_operator/src/handler/admin/platform_keys.rs` |
| Streaming (Server-Sent Events) | `rustd/crates/afd_api_tenant/src/handler/stream.rs` |
| Signature-proven ingress | `rustd/crates/afd_api_ingress/src/handler/webhook/receive_route.rs` |
| A runner speaking for itself | `rustd/crates/afd_api_runner/src/handler/runner/lease.rs` |

## §8 — Handler signature rule

A handler is an axum handler. The router built it into a stack that has already
admitted the request, proven the caller, checked the capability and confirmed
ownership, so a handler receives what it needs as extractors and constructs
none of it.

```rust
pub async fn my_endpoint<D: Services>(
    State(services): State<Arc<D>>,
    context: WorkspaceContext,
    Path(path): Path<FleetPath>,
    Json(body): Json<MyRequest>,
) -> Result<Response, Refusal> {
    // 1. parse and bound the inputs
    // 2. call the service
    // 3. return the wire type
}
```

### Rules

- **Take what you read, and nothing else.** `State` for services, the auth
  context for the caller, `Path` for identifiers, `Json` or `Bytes` for a body.
  A handler that does not read the body does not extract one.
- **Return `Result<Response, Refusal>`**, or a wire type that implements
  `IntoResponse`. `Refusal` carries the error code and the sentence; the
  envelope is built for you.
- **Bounds are declared on the request type**, with `garde`, not re-spelled per
  handler. The refusal sentence a field earns is mapped from the path `garde`
  reports.
- **Never authenticate inside a handler.** The guard on the route's row did it.
  Never call an ownership check by hand either; the template mounted it.
- **Refusal sentences live beside the parser that produces them**, as
  constants, so the test that asserts one reads it from the same place.

### What NOT to do

- ❌ Reading a query string directly for `limit` or `starting_after` — use
  `afd_core::paging`.
- ❌ Building a `Problem` by hand — construct it from the declared error code so
  the status, title and documentation link come from the registry (§5).
- ❌ A second unique spelling of a capability check inside the handler body.
- ❌ Re-parsing a path parameter the extractor already typed.

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

No silent removals. No removals that skip the `Deprecation` header period.

### Response field stability classes

Every response field has one of three classes, declared via OpenAPI extension `x-stability`:

- `stable` — covered by the deprecation policy above. Default.
- `beta` — may change shape or be removed without the 90-day clock; flagged as `beta` in the OpenAPI description; SDK generators may strip or wrap as optional.
- `internal` — present in responses for tooling/debugging; SDK generators MUST strip; no compatibility guarantee. Avoid leaking these — internal fields tend to leak production state.

Every new response field added in a PR MUST declare a class. Default is `stable` if you forget, which is binding — the deprecation clock starts the moment it ships.

### PR-level surface diff

Every PR that changes the HTTP surface MUST open its description with a "Surface diff" section listing added / renamed / removed `(method, path)` pairs and added / renamed / removed schema fields. This is how reviewers and the orly-babysit-prs loop detect breaking changes that the diff itself buries.

---

## §10 — Pre-PR checklist (testing + ship)

Before opening a PR touching any handler:

- [ ] `zig build` clean
- [ ] `zig build test` passes
- [ ] the declared `verify.unit` command passes — the route-scope match and its tests cover the auth gate matrix
- [ ] the repository's integration suite passes end to end against real datastores, where it declares one
- [ ] Cross-compile: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] the declared `verify.lint` command passes — every language gate the repository declares
- [ ] Handler file ≤ 350 lines; split if it grows
- [ ] Integration test covers the happy path AND at least one error path per refusal the handler can return
- [ ] PATCH endpoints have an idempotency test: same body issued twice → identical 200 + identical row state. Skip only if the spec's "Failure Modes" explicitly declares non-idempotent PATCH with a reason (§2)
- [ ] POST endpoints accepting `Idempotency-Key` have a replay test: same key + same body → cached response; same key + different body → 4xx (§2)
- [ ] Mutable resources have an ETag/`If-Match` test: stale `If-Match` → 412 with current `etag` returned (§4)
- [ ] OpenAPI updated — endpoint definition, request/response schemas, error responses
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
