# HTTP responses and errors

Rules for what an endpoint returns: wire types, field names, lists, instants, durations, status codes and error bodies. A coding agent applies them while writing a response or a refusal, and a reviewer judges a diff against them. Paths and methods are in [http.md](http.md).

## Bodies and fields

### RAD — A new or changed endpoint meets the HTTP design rules, and its response holds no acknowledgement or two-meaning field.
- **Applies to:** new or changed endpoints, their wire types and their OpenAPI annotations
- **Checked by:** review
- **Bad:** a steer response `{"ack": true, "run_steered": true}`
- **Good:** `{"message_queued": true, "execution_active": false}`, with the status code as the acknowledgement
- **Why:** a response shape is the hardest part to change once tests and the OpenAPI document depend on it.

The design rules it names are HTTP-VERB, HTTP-METHOD, HTTP-PATHID, HTTP-FIELD, HTTP-PROBLEM and HTTP-VERSION.

### HTTP-WIRE — A handler returns a wire type through `IntoResponse`, and builds a response by hand only for a Server-Sent Events (SSE) stream.
- **Applies to:** handlers and the wire-type module
- **Checked by:** review
- **Bad:** `(StatusCode::OK, Json(json!({"id": id, "name": name})))` spelled inline in a handler
- **Good:** `Ok(Json(ProjectDetailResponse::from(&detail)).into_response())`
- **Why:** the same struct generates the OpenAPI schema, so field names, status and content type cannot drift.

### HTTP-SINGLE — A single-resource response is the resource itself, with no envelope.
- **Applies to:** response wire types
- **Checked by:** review
- **Bad:** `{"data": {"id": "…"}}`, `{"project": {…}}`
- **Good:** `{"id": "…", "name": "Support triage", "created_at": "2026-10-17T18:06:49.123Z"}`
- **Why:** an envelope adds a layer every client must unwrap and every Software Development Kit (SDK) must model.

### HTTP-FIELD — A field name is snake_case, ends `_id` for an identifier and `_at` for an instant, and carries its unit.
- **Applies to:** request and response wire types
- **Checked by:** review
- **Bad:** `isEnabled`, `items_completed`, `size`, `owner` holding an identifier
- **Good:** `enabled`, `completed_items`, `size_in_bytes`, `owner_user_id`
- **Why:** consistent names let SDK users guess a field without reading its schema.

Booleans have no `is_` prefix, adjectives come before nouns, and collections are plural.
Brand names and reserved words stay out of field names, and a bare `id` names only the resource's own key.

### HTTP-NAMES — No field is named `data`, `payload`, `object`, `response`, `result`, `value`, `info` or `metadata` on its own.
- **Applies to:** request and response wire types
- **Checked by:** review
- **Bad:** `{"data": {…}}`, `{"metadata": {…}}`
- **Good:** a domain noun, such as `usage_line` or `schedule`
- **Why:** these names say nothing and make poor SDK accessors.

The finished-operation body of HTTP-LRO keeps its `result` field.

### HTTP-NULL — An optional field with no value is omitted, and `null` means the value was explicitly cleared.
- **Applies to:** request and response wire types
- **Checked by:** review
- **Bad:** `"label": null` for a label that was never set
- **Good:** no `label` key until one is set, and `PATCH` with `{"label": null}` clears it so later reads show `null`
- **Why:** SDK users branch on undefined versus `null`, and mixing the two breaks clients silently.

A lifecycle instant not yet reached, such as an unrevoked key's `revoked_at`, is `null` and declared `nullable: true`.

## Lists

### HTTP-LIST — A list response is exactly `{items, total, next_cursor}`.
- **Applies to:** list endpoints and their wire types
- **Checked by:** review
- **Bad:** `{"results": […]}`, `{"data": […], "count": 3}`, or `204` for an empty list
- **Good:** `{"items": [], "total": null, "next_cursor": null}` with status `200`
- **Why:** one envelope lets every SDK page through every list the same way.

`items` is always an array, `total` is an integer or `null` when not computed, and `next_cursor` is always present.
Both nullable keys declare `nullable: true`. `GET /v1/tenants/me/workspaces` also returns `tenant_id`, which clients store with the list.

### HTTP-PAGE — Pagination is keyset, `?starting_after=<id>&limit=<int>` in and `next_cursor` out, parsed by the shared paging code.
- **Applies to:** list handlers
- **Checked by:** review
- **Bad:** reading `limit` from the query string by hand, or an offset parameter
- **Good:** the shared `paging::cursor`, `Page::parse` with the route's `Ceiling`, or `Limit::parse`
- **Why:** shared parsing gives every list one default of 50 and one maximum of 100, tested at the served endpoint.

An empty `limit` uses the route default, and `next_cursor` goes back as the next request's `starting_after`.

### HTTP-FILTER — A filter uses one grammar: comma-separated equality, exclusive `_after` and `_before` bounds, and `include` with a documented enum.
- **Applies to:** list query parameters
- **Checked by:** review
- **Bad:** `?created_at[gte]=…`, `?include_events=true&include_runs=true`
- **Good:** `?status=active,paused&created_after=2026-10-01T00:00:00Z&include=events,runs`
- **Why:** one grammar across endpoints lets SDKs model every filter the same way.

An endpoint whose time bounds are inclusive says so in the parameter description.

### HTTP-SORT — A list sorts by one key, `sort=field` ascending or `sort=-field` descending, and offers no sparse fieldsets.
- **Applies to:** list query parameters
- **Checked by:** review
- **Bad:** `?sort=status,-created_at`, `?fields=id,name`
- **Good:** `?sort=-created_at`
- **Why:** one key maps onto one keyset cursor, and a slimmer payload is a smaller endpoint, not a knob.

### QPC — Sister endpoints accept the same value set for the same filter.
- **Applies to:** query parameters shared across endpoints, their OpenAPI enums and their tests
- **Checked by:** review
- **Bad:** workspace billing accepts `period_days` of 7, 30 or 90 while project billing silently clamps 90 to 30
- **Good:** one shared constant feeds both handlers and both OpenAPI `enum` lists, and each value is tested on both
- **Why:** a generated SDK exposes one enum per parameter, so a divergent value returns wrong data with no error.

An out-of-set value gets the same treatment on both endpoints, either `400` or the same coercion.

### HTTP-BULK — A batch endpoint answers `207 Multi-Status` with one item per input.
- **Applies to:** batch endpoints
- **Checked by:** review
- **Bad:** a batch returning `200` with a bespoke `failures` array
- **Good:** `{"items": [{"id": "…", "status": "succeeded"}, {"id": "…", "status": "failed", "error": {…}}]}`
- **Why:** one shape lets clients handle partial failure the same way on every batch.

An item's `status` is `succeeded`, `failed` or `skipped`, and its `error` is the error envelope without `request_id`.
A malformed batch still fails whole with `4xx`.

## Instants and durations

### HTTP-TIME — An instant is a Request for Comments (RFC) 3339 string in Coordinated Universal Time (UTC) ending `Z`, in a field ending `_at`.
- **Applies to:** wire types, query parameters and cursors
- **Checked by:** review
- **Bad:** `"created_at": 1760724409123`, or `"created": "2026-10-17 18:06"`
- **Good:** `"created_at": "2026-10-17T18:06:49.123Z"`, held as `jiff::Timestamp` with its standard serialization
- **Why:** one textual form lets every client parse every instant, and an epoch integer hides its unit.

A request may carry any numeric offset, which the server normalizes to UTC.
An invalid or out-of-range instant answers `400` with a field-specific `detail`, and shared validation enforces the range and millisecond precision.

### HTTP-DURATION — A duration is an integer whose field name ends `_ms` or `_seconds`.
- **Applies to:** wire types and query parameters
- **Checked by:** review
- **Bad:** `timeout`, `ttl`, `"PT5M"`, `1.5`
- **Good:** `timeout_ms`, or `ttl_seconds` where the system beneath speaks seconds, such as a key-value store `EX` or a JSON Web Token `exp`
- **Why:** the unit in the name is the only unit a reader of the payload can see.

Mixing `_ms` and `_seconds` in one endpoint needs a reason in its OpenAPI description, and `_ms` is the default choice.

## Status codes

### HTTP-STATUS — A status code means what its entry below says, and `409` and `412` stay distinct.
- **Applies to:** handlers and the error registry
- **Checked by:** review
- **Bad:** `409` for a stale `If-Match`, `400` for a missing resource, or an invented code
- **Good:** `409` when the resource's state forbids the transition, and `412` when the caller's version is stale
- **Why:** clients branch on the code, and merging two meanings sends them down the wrong retry path.

`200` is a read or update, `201` a create, `202` accepted work and `204` no content.
`400` is a bad shape, `401` unauthenticated, `403` unauthorized, `404` not found, `413` too large, `429` rate limited and `500` internal.

### HTTP-STATEEXT — A `409` body names `current_state`, a `412` body carries the current `etag`, and no other extension exists unless this standard adds it.
- **Applies to:** refusals with status `409` or `412`
- **Checked by:** review
- **Bad:** a `409` whose only explanation is `"detail": "conflict"`
- **Good:** `{"status": 409, …, "current_state": "approved"}` when the gate was already approved
- **Why:** the client needs the current state or version to recover without another read.

A reviewed setup mutation also returns an opaque `review_digest` on a stale-review `409`, which grants no authority.

### HTTP-429 — A `429` carries `Retry-After` in whole seconds and the `X-RateLimit-Limit`, `X-RateLimit-Remaining` and `X-RateLimit-Reset` headers.
- **Applies to:** rate-limited endpoints
- **Checked by:** review
- **Bad:** a bare `429` with no headers
- **Good:** `Retry-After: 30`, with `X-RateLimit-Reset` in epoch seconds
- **Why:** without them a client can only guess when to retry.

### HTTP-ETAG — A resource two principals can edit returns an `ETag`, and its writes require `If-Match`.
- **Applies to:** `GET`, `PATCH`, `PUT` and `DELETE` handlers of shared mutable resources
- **Checked by:** review
- **Bad:** a last-writer-wins `PATCH` on a shared project configuration
- **Good:** `GET` and `PATCH` responses carry `ETag`, and a test shows a stale `If-Match` answered `412` with the current `etag`
- **Why:** optimistic concurrency stops one editor from silently overwriting another.

A resource without realistic concurrent edits may opt out, with the decision in the spec's Failure Modes section.

## Errors

### HTTP-REGISTRY — Every error response is built from a registry code, never assembled by hand.
- **Applies to:** handlers and the error-code registry module
- **Checked by:** review
- **Bad:** a hand-built `Problem`, or `(StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))`
- **Good:** `Refusal::coded(PROJECT_NOT_FOUND, DETAIL_NOT_FOUND)`, `Refusal::malformed(DETAIL_WORKSPACE_ID)` or `Refusal::preconditioned(error.code(), error.detail(), current)`
- **Why:** status, title and documentation link come from one entry, so one error never reaches clients with two spellings.

### HTTP-NEWCODE — A new error code enters the registry with its status, title and documentation link, and a test reaches it.
- **Applies to:** the error registry and its family modules
- **Checked by:** review
- **Bad:** a new code constant that a handler returns with no registry entry
- **Good:** the family constant, its registry entry, and a test asserting the entry is reachable, listed in the spec's error table
- **Why:** the registry is the one place a client-facing error is defined.

### HTTP-TITLE — A registry title is a noun phrase of two to five words, in sentence case, with no trailing punctuation.
- **Applies to:** error registry titles
- **Checked by:** review
- **Bad:** `error: database is currently unavailable.`, `WORKSPACE_NOT_FOUND`, `Something went wrong while processing your request`
- **Good:** `Database unavailable`, `Insufficient role`, `Invalid webhook signature`
- **Why:** a title must be safe to show verbatim in a user interface toast.

### HTTP-DETAIL — An error `detail` is one sentence or fragment of at most 200 characters, written for a developer calling the API.
- **Applies to:** refusal sentences
- **Checked by:** review
- **Bad:** a paragraph explaining the storage layer, or a `detail` that interpolates a `workspace_id`
- **Good:** `key_name must be 1-64 chars, alphanumeric + hyphen + underscore`, `Workspace access denied`, `Project name already exists`
- **Why:** `detail` is user-facing text, and each code keeps one shape of sentence.

A placeholder carries only an enumerable safe value, such as a role label or sort option, never an entity value.

### HTTP-LEAK — An error `detail` never contains internals, secrets or entity values.
- **Applies to:** refusal sentences and the refusal renderer
- **Checked by:** review
- **Bad:** `duplicate key value violates unique constraint "uq_projects_name"`, `panic at src/project.rs:42`, `Bearer eyJ…`
- **Good:** `Project name already exists`
- **Why:** a leaked table, path or token helps an attacker and teaches clients to parse internals.

Banned: `pg_` names, table and unexposed column names, SQL keywords, source paths, line numbers, stack traces and addresses.
Also banned: `sk_`, `pk_`, `Bearer `, `eyJ`, `op://`, hex runs over 16 characters, identifier values, emails and vault key names.

### HTTP-500 — An internal failure answers the registry's `500`, and only diagnostics name the failed operation.
- **Applies to:** service and datastore error paths in handlers
- **Checked by:** review
- **Bad:** `Refusal::coded(INTERNAL, &err.to_string())`, which puts the driver's message into `detail`
- **Good:** `services.project_detail(&context, project_id).await.map_err(Refusal::at(PROJECT_READ))?`
- **Why:** the operation constant identifies the failure in logs and telemetry, and the caller learns nothing internal.

### HTTP-PROBLEM — An error body is an RFC 7807 problem with `type`, `title`, `status`, `detail` and `request_id`.
- **Applies to:** the refusal renderer
- **Checked by:** review
- **Bad:** `{"error": "invalid", "message": "bad id"}`
- **Good:** `{"type": "https://docs.example.com/errors/invalid-request", "title": "Invalid request", "status": 400, "detail": "workspace_id must be a valid UUIDv7", "request_id": "req_…"}`
- **Why:** one envelope lets clients and support trace any failure by its `request_id`.
