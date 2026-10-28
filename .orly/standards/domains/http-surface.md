# HTTP surface: OpenAPI, versions and clients

Rules for how the public HTTP surface is documented, tested, changed and retired, and for the command-line client that calls it. A coding agent applies them before changing a public endpoint, field or command, and a reviewer judges a diff against them. Paths are in [http.md](http.md) and responses in [http-responses.md](http-responses.md).

## The OpenAPI document

### HTTP-OPENAPI — The repository's OpenAPI document is generated from the server, regenerated and committed with the code, and never edited by hand.
- **Applies to:** the repository's OpenAPI document, `#[utoipa::path]` annotations and wire types
- **Checked by:** review
- **Bad:** a hand edit to the OpenAPI document, or a handler change committed without regenerating it
- **Good:** the repository's regeneration command, then `git diff --exit-code` on the document it writes
- **Why:** the next regeneration reverts a hand edit, and no test compares the committed document with the build.

### HTTP-ANNOTATE — Every served route has an OpenAPI annotation, and every annotation names a mounted route and the refusals its guard guarantees.
- **Applies to:** handlers and each plane's `src/openapi.rs` collector
- **Checked by:** gate: verify.unit
- **Bad:** a mounted handler with no `#[utoipa::path]`, or an annotation that omits the `403` its scope guarantees
- **Good:** `#[cfg_attr(feature = "openapi", utoipa::path(…))]` beside the handler, named in that plane's collector
- **Why:** `test_coverage_gate_rust_source` and `test_documented_codes_match_refusals` refuse the gap and name the route and method.

### HTTP-PROSE — OpenAPI prose is written at its source: the annotation, or the doc comment on the wire type.
- **Applies to:** `#[utoipa::path]` descriptions and wire-type doc comments
- **Checked by:** gate: verify.lint
- **Bad:** maintainer rationale in a `///` comment that utoipa publishes, or a wording fix made in the generated JSON
- **Good:** consumer-facing prose in the annotation or `///` comment, and maintainer rationale in a `//` comment beside it
- **Why:** the documentation checks read the generated document, so the source is the only place a fix lasts.

### HTTP-TAG — Each top-level resource maps to exactly one OpenAPI tag.
- **Applies to:** the `tag` of every `utoipa::path` annotation
- **Checked by:** review
- **Bad:** project operations split across `Projects` and `ProjectEvents`, or one `Workspaces and Projects` tag
- **Good:** `tag = "Projects"` on every project operation, sub-resources included
- **Why:** generators group operations by tag, so one resource stays one client module.

### CIV — An OpenAPI validator follows every `$ref` to its target instead of skipping it.
- **Applies to:** scripts and tests that validate OpenAPI responses
- **Checked by:** review
- **Bad:** a check that inspects inline `content` and passes any response written as a `$ref`
- **Good:** a response with no inline `content` has its `$ref` resolved and compared with the expected shared response
- **Why:** a `$ref` to a legacy error shape otherwise passes the validator unseen.

### HTTP-SDK — A new response pattern is one that Software Development Kit (SDK) generators already handle cleanly.
- **Applies to:** wire types and OpenAPI schemas
- **Checked by:** review
- **Bad:** a polymorphic response body added without checking generator output
- **Good:** a shape mirrored from an endpoint known to generate cleanly under Stainless, Speakeasy, APIMatic and OpenAPI Generator
- **Why:** a non-standard pattern breaks client generation on the next bundle.

## Versions and change

### HTTP-VERSION — Every endpoint sits under a URI version prefix, `/v1` today, and no header selects a version.
- **Applies to:** route templates
- **Checked by:** review
- **Bad:** `X-API-Version: 2026-04-25`, or an unversioned `/projects`
- **Good:** `/v1/workspaces/{workspace_id}/projects`
- **Why:** one versioning scheme keeps routing, documentation and SDKs aligned.

`/v2` arrives only when a breaking change is unavoidable.

### HTTP-BREAK — A change inside `/v1` is additive: no rename, removal, type or unit change, tightened validation or new requirement.
- **Applies to:** wire types, route templates, query parameters and status codes
- **Checked by:** review
- **Bad:** renaming `project_name` to `name`, a lower maximum length, an optional field made required, or a new default `limit`
- **Good:** add `name`, fill both fields, mark `project_name` deprecated, and remove it only in `/v2`
- **Why:** clients the server cannot see rely on every exposed name, including path and query parameter names.

A different status code for an existing failure is breaking.
A new endpoint, optional request field, request enum value or error code is additive, and so is looser validation.

### HTTP-ENUM — A new value in a response enum waits for `/v2` or ships as an `additional_*` companion field.
- **Applies to:** response enums
- **Checked by:** review
- **Bad:** adding `paused` to a response `status` enum inside `/v1` with no note
- **Good:** `status` left frozen, and `additional_status: "paused"` documented in the OpenAPI description
- **Why:** typed SDKs generate exhaustive matches, so a new value breaks compiles or falls through silently.

A new value that ships inside `/v1` anyway is named in the PR description with the consumers that need a release.

### HTTP-STABILITY — Every new response field declares `x-stability` as `stable`, `beta` or `internal`.
- **Applies to:** new response fields
- **Checked by:** review
- **Bad:** a new field with no class, which defaults to `stable` and starts the deprecation clock as it ships
- **Good:** `x-stability: beta` on a field whose shape may still change
- **Why:** the class decides whether the deprecation clock binds, and generators strip `internal` fields.

### HTTP-DEPRECATE — A deprecated endpoint or field sends `Deprecation`, `Sunset` and a `Link` to its migration guide for at least 90 days.
- **Applies to:** deprecated endpoints and fields
- **Checked by:** review
- **Bad:** removing a field one release after announcing it
- **Good:** `Deprecation: true`, `Sunset: <RFC 1123 date>`, `Link: <docs-url>; rel="deprecation"`, and `deprecated: true` with a migration hint in OpenAPI
- **Why:** clients need a visible signal and time to move before anything disappears.

Anything an external SDK consumer touches keeps the headers for 180 days.

### EP4 — An intentionally removed endpoint answers `410 Gone` with a named error code.
- **Applies to:** endpoint removal
- **Checked by:** review
- **Bad:** a removed route that now answers `404`
- **Good:** the route kept in the table, answering `410` with its registry code after the deprecation window, and a test proving it
- **Why:** `404` reads as a routing fault to monitors and clients, while `410` says the removal is permanent and intended.

A removal follows the repository's deployed clients and deprecation policy, and states the result clients will see.

### HTTP-SURFACE — A Pull Request (PR) that changes the HTTP surface opens its description with a surface diff.
- **Applies to:** PR descriptions for endpoint, path, query or schema changes
- **Checked by:** review
- **Bad:** a PR that adds a response field and renames a query parameter with no list of either
- **Good:** a "Surface diff" section listing added, renamed and removed `(method, path)` pairs and schema fields
- **Why:** reviewers then see breaking changes that the code diff buries.

Exposing an existing endpoint to browsers is a surface change too.

## Tests and budgets

### HTTP-TESTS — An endpoint's integration tests cover the happy path and one case for each refusal it can return.
- **Applies to:** handler test suites
- **Checked by:** review
- **Bad:** one happy-path test for a handler with four refusals
- **Good:** a test per refusal, plus the double-`PATCH`, `Idempotency-Key` replay and stale `If-Match` cases where they apply
- **Why:** each refusal is behaviour a client depends on, and an untested one drifts.

### HTTP-PERF — An endpoint meets p95 under 150 ms and p99 under 200 ms, and runs no unbounded query loop.
- **Applies to:** new and changed endpoints
- **Checked by:** review
- **Bad:** a one-line "this endpoint is slow" note standing in for a carve-out
- **Good:** a carve-out in the spec naming the endpoint, measured p50, p95 and p99 with the bench command, the load profile and the reason
- **Why:** an unmeasured exception becomes the new baseline.

The carve-out also names the milestone that retires it, or `accepted-permanent` with a justification the owner reviewed.

## Command-line client

### JCL — The command-line interface's `--json` output carries only stable, documented error codes.
- **Applies to:** `cli/src/**`
- **Checked by:** review
- **Bad:** undocumented `AGENT_ERROR` or `IO_ERROR` codes, or usage text returned as the error message
- **Good:** `{"error": {"code": "UNKNOWN_COMMAND", "message": "unknown command 'project-ls'"}}`, with the code from `REJECTION_CODE`
- **Why:** scripts switch on these codes, so an ad-hoc code or a prose message breaks them.

`UNKNOWN_COMMAND` names the unrecognized token.
A second `jsonMode` check on one path carries a comment saying why both exist.

### CLI-HINT — A renamed or removed command leaves no message, hint, comment or changelog line naming its old syntax.
- **Applies to:** `cli/src/**`, error hints and messages, log lines, doc comments and the changelog entry
- **Checked by:** review
- **Bad:** a hint that still says `Run 'acme install <template>'` after the positional form was removed
- **Good:** every hint and message names the new syntax, found by searching for the pattern and reading the sentence around each hit
- **Why:** a user reads a hint when already confused, and a dead command doubles the failure.

Neither lint nor the type checker reads message bodies, and hints often paraphrase the command.
