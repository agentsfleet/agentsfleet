# HTTP paths, methods and routes

Rules for the shape of the HTTP API: resource paths, methods, idempotency, long-running work, route registration and handler signatures. A coding agent applies them before adding or changing an endpoint, and a reviewer judges a diff against them. Responses are in [http-responses.md](http-responses.md), and OpenAPI, versioning and clients in [http-surface.md](http-surface.md).

## Paths

### HTTP-NOUN — A resource path is a plural noun collection, optionally followed by one identifier.
- **Applies to:** route templates in the repository's route definition files
- **Checked by:** review
- **Bad:** `/getProjects`, `/createProject`, `/project/{project_id}`
- **Good:** `GET …/projects` for the collection and `GET …/projects/{project_id}` for one project
- **Why:** OpenAPI generators build clean Software Development Kits (SDKs) from this shape.

### HTTP-NEST — A path nests each resource under the resource that owns it.
- **Applies to:** route templates
- **Checked by:** review
- **Bad:** `GET /v1/projects?workspace_id=…`
- **Good:** `GET /v1/workspaces/{workspace_id}/projects`
- **Why:** the nesting states that the project belongs to the workspace, and the ownership layer reads it from the template.

### HTTP-VERB — A path holds no verb, and a state change is a `PATCH` of the resource's state field.
- **Applies to:** route templates
- **Checked by:** review
- **Bad:** `POST /v1/users/{user_id}/activate`, `POST …/projects/{project_id}/start`
- **Good:** `PATCH /v1/users/{user_id}` with `{"status": "active"}`, or `POST …/projects/{project_id}/events` creating an event
- **Why:** the method carries the action, so a resource keeps one set of paths.

### HTTP-OP — An operation endpoint is a trailing literal segment, used only for a retry, a side-effecting call or a multi-resource transaction.
- **Applies to:** operation routes and their OpenAPI descriptions
- **Checked by:** review
- **Bad:** `POST …/schedules/{schedule_id}:sync`, which the router cannot match, or an operation added for convenience
- **Good:** `POST …/schedules/{schedule_id}/sync`, whose OpenAPI description names its category
- **Why:** `matchit` refuses a literal after a parameter inside one segment, and every other action is a state change.

The categories are an idempotent retry (`sync`, `retry`) and a side-effecting call the caller cannot undo by editing a field (`send`, `rotate`).
An atomic change to resources beyond the path's own resource is the third.

### HTTP-OPSTATE — An operation never sets a value that the resource's lifecycle field can take through `PATCH`.
- **Applies to:** new operation routes
- **Checked by:** review
- **Bad:** `POST …/projects/{project_id}/pause` while `Project.status` can already hold `paused`
- **Good:** `PATCH …/projects/{project_id}` with `{"status": "paused"}`
- **Why:** two routes to one state are two code paths that must agree forever.

A lifecycle field is `status`, `state`, `stage` or `lifecycle_state`.
The Pull Request (PR) description records the check: no such field exists, or the value is not settable through `PATCH`, and why.

### HTTP-PARAM — A path parameter is named `{<resource>_id}`, matches the body field of the same name, and has one spelling per concept.
- **Applies to:** route templates and wire types
- **Checked by:** review
- **Bad:** `{id}`, `{projectId}`, `{fid}`
- **Good:** `{project_id}`, matching `project_id` in the project's body
- **Why:** one spelling lets generated clients and the ownership check match names.

### HTTP-SLASH — A path has no trailing slash, and a request with one is redirected with `308` to the canonical path.
- **Applies to:** route templates and the router
- **Checked by:** review
- **Bad:** a template ending `/projects/`, or a trailing-slash request answered with a silent `404` or rewrite
- **Good:** the template `…/projects`, with `…/projects/` answered `308 Permanent Redirect` to it
- **Why:** one spelling per route keeps metrics and clients aligned, and `308` preserves the method and body.

### HTTP-PATHID — An identifier in the path is never repeated in the request body.
- **Applies to:** request wire types
- **Checked by:** review
- **Bad:** `PATCH …/projects/{project_id}` with `{"project_id": "…", "name": "Support triage"}`
- **Good:** `PATCH …/projects/{project_id}` with `{"name": "Support triage"}`
- **Why:** two copies of one identifier can disagree, and the server then has to pick one.

### HTTP-ID — A public identifier is a Universally Unique Identifier version 7 (UUIDv7), never a database serial integer.
- **Applies to:** wire types and route parameters
- **Checked by:** review
- **Bad:** `"id": 4182`
- **Good:** `"id": "0199f3c2-7a1b-7c3d-8e4f-0123456789ab"`, parsed and tested at the endpoint boundary
- **Why:** a serial integer leaks row counts and invites enumeration of other tenants' rows.

## Routing

### RTM — Route matching compares path segments, never substrings of the raw path.
- **Applies to:** routers and any custom path dispatch in `**/*.rs` and `**/*.ts`
- **Checked by:** review
- **Bad:** `if path.ends_with("/messages") { … }`, or dispatch whose correctness depends on match order
- **Good:** the template `"/v1/workspaces/{workspace_id}/projects/{project_id}/messages"`, matched by segment count and segment equality
- **Why:** substring matching spreads parsing across matchers and breaks on trailing slashes, double slashes and reordering.

A reserved literal segment is a predicate in both matchers, so they exclude each other in any order.
Each route variant has typed fields such as `credential_name` or `gate_id`, never a shared `leaf_id`.

### RES — A name a route reserves is refused on every write into the shared store, not only by the matcher.
- **Applies to:** routes that reserve a segment inside a shared table or key space
- **Checked by:** review
- **Bad:** the matcher reserves `/credentials/llm`, while `POST /credentials` accepts `{"name": "llm"}`
- **Good:** one constant feeds the matcher and a write validator answering `400`, and a test proves no row landed
- **Why:** a write under the reserved name lands where no route can read or delete it.

### HTTP-ROUTE — A route is a variant in its family enum and that family's `ALL` array, a `meta()` row, a plane handler arm and an annotation.
- **Applies to:** route definition files and handler modules
- **Checked by:** review
- **Bad:** a variant added to `ProjectRoute` but not to its `ALL` array, which compiles and answers `404`
- **Good:** the variant, its `ALL` entry, its `meta()` arm, its `project_handler_for` arm and `#[utoipa::path]`, then a test requesting the URL
- **Why:** the router is built from the table, so a variant missing from `ALL` is never mounted and nothing fails.

A verb this binary does not serve gets `None` from `handler_for` and answers `404`, never `501`.

### HTTP-TEMPLATE — A route template carries parameters only, never a concrete value.
- **Applies to:** `RouteMeta` templates
- **Checked by:** review
- **Bad:** an `http.route` value built from the request path, such as `/v1/workspaces/0199f3c2-…/projects`
- **Good:** `"/v1/workspaces/{workspace_id}/projects"`, a compile-time literal
- **Why:** the template is also the `http.route` span attribute, and a real path puts tenant identity into traces and one route value per request.

### HTTP-CLASS — A route's class matches its traffic: `Ops` for probes, `Stream` for a held Server-Sent Events connection, `Api` for the rest.
- **Applies to:** `RouteMeta` rows
- **Checked by:** review
- **Bad:** an event stream classed `Api`, so each held connection counts against the in-flight ceiling for minutes
- **Good:** `RouteClass::Stream` for `…/events/stream`, which has its own cap
- **Why:** `Ops` never sheds, `Stream` is capped apart, and `Api` is subject to the in-flight ceiling.

## Methods and idempotency

### HTTP-METHOD — The method follows its meaning: `GET` reads, `POST` creates or runs an operation, `PUT` replaces, `PATCH` changes fields, `DELETE` removes.
- **Applies to:** route rows and handlers
- **Checked by:** review
- **Bad:** `POST …/projects/{project_id}/update` for a partial change, or a `GET` with side effects
- **Good:** `PATCH …/projects/{project_id}`, and `PUT` only when the client supplies the identifier and the whole resource
- **Why:** clients, caches and SDKs infer retry safety from the method.

`DELETE` is idempotent and answers `204` for a row that is already gone.

### HTTP-PATCH — `PATCH` is idempotent unless the spec's Failure Modes section declares otherwise with a reason.
- **Applies to:** `PATCH` handlers and their tests
- **Checked by:** review
- **Bad:** a `PATCH` that appends to a list or increments a counter, with no declared exception
- **Good:** a test sends the same body twice and asserts identical row state and identical `200` responses
- **Why:** clients retry a `PATCH` after a timeout, and the retry must not change the outcome.

### HTTP-IDEMKEY — A `POST` that creates a billable, externally visible or side-effecting resource honors a UUIDv7 `Idempotency-Key` header.
- **Applies to:** `POST` handlers, their OpenAPI descriptions and their tests
- **Checked by:** review
- **Bad:** a create endpoint that inserts a second row when the client retries after a timeout
- **Good:** the server keeps `(workspace_id, key) → response` for at least 24 hours and replays it, and a changed body under that key answers `4xx`
- **Why:** a client cannot tell a lost response from a lost request, so only the key makes its retry safe.

Pure reads and internal-only `POST`s are exempt, and the OpenAPI description lists the endpoints that honor the key.
`POST /v1/workspaces` takes no key: a tenant-unique name makes a retry answer `409`, and clients recover by reading.
That exception covers no other endpoint.

### HTTP-MSGKEY — A project or draft `/messages` send carries one UUIDv7 `operation_id` in its body and refuses an `Idempotency-Key` header.
- **Applies to:** project and creator-private draft message sends, and draft creation with a first message
- **Checked by:** review
- **Bad:** `POST …/drafts/{draft_id}/messages` sent with an `Idempotency-Key` header
- **Good:** `{"operation_id": "<uuidv7>", …}` answered `202 {status, sequence, replayed}`, with `Location` at the messages collection
- **Why:** authorization and lifecycle checks run before replay, and the same key, message and actor recover the accepted message.

The same key with changed content or actor conflicts. These sends create no turn-status resource and no generic operation.
Creator-private text never appears in workspace streams.

### HTTP-LRO — Work that cannot finish inside the request answers `202` with `Location: /v1/operations/{operation_id}`.
- **Applies to:** long-running endpoints, except the conversation sends of HTTP-MSGKEY
- **Checked by:** review
- **Bad:** a bespoke polling path such as `…/imports/{import_id}/progress`
- **Good:** `202` with `{"operation_id": "…", "status": "pending"}`, then `GET /v1/operations/{operation_id}` returning `operation_id`, `done`, `status` and `started_at`
- **Why:** one polling shape lets every client and SDK wait on any operation the same way.

A finished operation carries `result` or an error envelope, and its terminal states are only `succeeded`, `failed` and `cancelled`.

## Handlers

### HTTP-EXTRACT — A handler takes only what it reads, as typed extractors, and returns `Result<Response, Refusal>`.
- **Applies to:** handler modules
- **Checked by:** review
- **Bad:** extracting a `Json` body the handler never reads, or re-parsing a path parameter the extractor already typed
- **Good:** `State(services)`, the auth context, `Path(path): Path<ProjectPath>`, and `Json(body)` only when the body is read
- **Why:** the route stack has already admitted, authenticated and authorized the request, so a handler parses, calls a service and returns.

### HTTP-GARDE — Input bounds are declared with `garde` on the request type, and refusal sentences come from a `Sentences` table.
- **Applies to:** request types, handlers and their parsers
- **Checked by:** review
- **Bad:** `if body.name.len() > 64 { … }` in a handler, or a hand-written function from `garde::Report` to a sentence
- **Good:** `#[garde(length(chars, max = MAX_NAME_CODEPOINTS))]` on the field, and a `const` `Sentences` table beside the handler
- **Why:** a declared bound cannot drift between handlers, and the test reads its sentence from the same table.

A rule `garde` lacks, such as `finite`, `nul_free`, `ascii_digits` or `charset`, comes from the repository's shared validation crate.
A crate that answers with its own error variant uses `PathTable<Variant>`.

### HTTP-VALID — A parser that a bound protects accepts only `&garde::Valid<T>`.
- **Applies to:** parsers of request fields
- **Checked by:** review
- **Bad:** `fn parse_cron(raw: &str)` called on a field whose bound sits beside the parser
- **Good:** the bound on a struct, and `fn parse_cron(spec: &garde::Valid<CronSpec>)`
- **Why:** `garde` runs every rule on a field without short-circuit, so a parser beside a bound still receives oversized input.

### HTTP-SENTENCE — A refusal sentence is a constant beside the parser that produces it.
- **Applies to:** handlers and parsers
- **Checked by:** review
- **Bad:** `Refusal::malformed("workspace_id must be a valid UUIDv7")` with the sentence inline
- **Good:** `Refusal::malformed(DETAIL_WORKSPACE_ID)`, and a test asserting the same constant
- **Why:** the test then reads the sentence from the place that produces it.

## Where these rules are silent

### HTTP-REFS — Where these standards are silent, external guides decide in a fixed order, and a conflict is recorded here.
- **Applies to:** HTTP design questions no rule on these pages answers
- **Checked by:** review
- **Bad:** adopting an external guide's pattern that contradicts a rule here without recording the conflict
- **Good:** the [Microsoft REST API Guidelines](https://github.com/microsoft/api-guidelines) for naming and pagination, then the [Google API Design Guide](https://cloud.google.com/apis/design) for resource modelling
- **Why:** a fixed order settles a dispute the same way every time.

After those come the GitHub REST documentation for filtering and the OpenAPI Specification for schema validity.
RFC 7807 and then RFC 9457 settle the error envelope.
