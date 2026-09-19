<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M200_001: `agentsfleet whoami` names the person, tenant and credential class behind the credential on disk

**Prototype:** v2.0.0
**Milestone:** M200
**Workstream:** 001
**Date:** Sep 19, 2026
**Status:** PENDING
**Priority:** P1 — a signed-in terminal cannot name who it is signed in as, and the credential on disk is opaque by design
**Categories:** API, CLI, DOCS
**Batch:** B1 — single stream; the endpoint and its one client land together
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5, Sep 19, 2026)
**Canonical architecture:** `docs/architecture/user_flow.md` §8.0

---

## Overview

**Goal (testable):** `GET /v1/users/me` answers the calling principal's `user_id`, `email`, `display_name`, `tenant_id`, `tenant_name`, credential class and resolved scopes under every person credential class and no capability scope, and `agentsfleet whoami` renders it in both human and JSON form, exiting non-zero when nothing is signed in.

**Problem:** After `agentsfleet login` a terminal holds an `afc_` credential carrying no readable claims — by design, since capability resolves server-side from the row the credential names — and no endpoint will say whose row that is. `agentsfleet auth status` reports the credential's SOURCE (`file` / `env`), the target Uniform Resource Locator (URL) and a reachability verdict, printing the literal `opaque credential (scope resolved server-side)` where a person's identity would go. An operator with two accounts, or one terminal on development and another on production, has no command that answers "who am I". The Command-Line Interface (CLI) already anticipated the endpoint: `cli/src/lib/me-ping.ts:5` records that the spec called for `GET /v1/me`, that the handler never shipped, and that the post-login probe hits `/v1/tenants/me/billing` instead.

**Solution summary:** Serve the caller's identity from one new tenant-plane read, `GET /v1/users/me`, bearer-guarded and requiring no capability scope — the credential is the claim, and a route needing `billing:read` to prove a login is why the existing probe misreports a person without billing capability as rejected. The daemon resolves the subject the authenticator already proved through one join of `core.users` on `core.tenants`, and renders the credential class from the proven principal rather than from anything the caller sent. The CLI gains `agentsfleet whoami`, repoints both its post-login probe and its `auth status` probe at the new scope-free route, and makes `login` close by naming the person it just signed in — the moment the operator noticed was missing.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,cli): answer who a signed-in terminal is signed in as
- **Intent (one sentence):** An operator who has just run `agentsfleet login`, or who returns to a terminal days later, can ask the CLI which person, tenant and credential class it is acting as, and get the answer from the server rather than from a local guess.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/tenant/cli_credential.rs` — the nearest handler: it resolves a proven subject to a `core.users` row through a service seam, takes a person-class extractor rather than checking a class in its body, and carries the `utoipa::path` block shape this route copies.
2. `docs/REST_API_DESIGN_GUIDELINES.md` §1, §4, §7 — Uniform Resource Locator design, response field naming and the four-place route registration this endpoint must complete; §7's table names the silent failure (a variant missing from `ALL` mounts nothing and answers 404).
3. `rustd/crates/afd_http/src/auth/person.rs` — the `ClassPolicy` matrix. `PersonIdentity = Proven<AnyClass>` is the extractor this route takes, and `PersonCredential` is what the response's `credential` field renders from.
4. `rustd/crates/afd_tenant/src/cli_credential/mod.rs` + `rustd/crates/afd_tenant/src/sql/cli_credential.rs` — the store and statement shape the new identity read mirrors, including why the tenant on the joined user row is the authoritative one.
5. `cli/src/commands/auth.ts` and `cli/test/auth-effect.unit.test.ts` — the Effect-shaped command and the layer-composition test pattern `whoami` follows.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_http/src/route/tenant.rs` | EDIT | Adds the `CurrentUser` variant, its `ALL` entry, its verb and its `RouteMeta` — the row the router mounts from |
| `rustd/crates/afd_http/src/openapi.rs` | EDIT | Adds the `Users` tag the new operation is filed under, one tag per resource |
| `rustd/crates/afd_http/src/services/tenant.rs` | EDIT | Declares the `CallerProfiles` seam and implements it for the new store |
| `rustd/crates/afd_http/src/services/tenant_surface.rs` | EDIT | Adds the associated type and accessor the handler reaches the store through |
| `rustd/crates/afd_http/src/services/mod.rs` | EDIT | Re-exports the new trait beside its siblings |
| `rustd/crates/afd_wire/src/identity.rs` | CREATE | The response shape, separate from `tenant.rs` because that module is the api-key and credential payloads |
| `rustd/crates/afd_tenant/src/identity.rs` | CREATE | The store: one subject, one profile, one read |
| `rustd/crates/afd_tenant/src/sql/identity.rs` | CREATE | The `core.users` × `core.tenants` join the profile comes from |
| `rustd/crates/afd_wire/src/lib.rs`, `rustd/crates/afd_tenant/src/lib.rs`, `rustd/crates/afd_tenant/src/sql/mod.rs` | EDIT | Declare the three new modules |
| `rustd/crates/afd_tenant/src/error/{kind,detail,mod,raise}.rs` | EDIT | Renames `CliCredentialUnknownSubject` and its detail constant and constructor to the family-neutral `UnknownSubject` — one fact now read by two families; the wire sentence and code do not change |
| `rustd/crates/afd_api_tenant/src/handler/tenant/identity.rs` | CREATE | The handler: extractor in, one service call, render |
| `rustd/crates/afd_api_tenant/src/handler/tenant/mod.rs`, `.../afd_api_tenant/src/lib.rs`, `.../afd_api_tenant/src/openapi.rs` | EDIT | Declare and re-export the handler, return it from `tenant_handler_for`, and add it to this plane's document roster |
| `rustd/crates/agentsfleetd/src/plane/services.rs` | EDIT | Constructs the store and answers the new accessor |
| `rustd/crates/afd_api/tests/harness/services.rs` | EDIT | The router suite's stub answers the new accessor |
| `rustd/crates/afd_tenant/tests/integration_identity.rs` | CREATE | The read against live Postgres, including the unknown-subject refusal |
| `rustd/crates/afd_api/tests/router_identity.rs` | CREATE | The route is mounted, answers its declared verb, and refuses every credential class the policy excludes |
| `public/openapi.json` | EDIT | Regenerated from the build; never hand-edited |
| `cli/src/lib/api-paths.ts` | EDIT | Adds `USERS_ME_PATH`, the CLI's one spelling of the route |
| `cli/src/lib/me-ping.ts` | EDIT | The post-login probe becomes the identity read it was always meant to be, and stops needing a billing capability |
| `cli/src/commands/whoami.ts` | CREATE | The command Effect: read, render human or JSON, refuse when nothing is signed in |
| `cli/src/commands/login.ts` | EDIT | The success line names the person, from the identity the probe already fetched |
| `cli/src/commands/auth.ts` | EDIT | The reachability probe moves to the scope-free route, so a person without billing capability is no longer reported as rejected |
| `cli/src/program/{cli-tree,cli-tree-types,handlers-bind}.ts` | EDIT | Registers `agentsfleet whoami`, adds its handler slot, and binds the command Effect through the dispatcher |
| `cli/test/whoami.unit.test.ts` | CREATE | The command's behaviour, failure and rendering cases |
| `cli/test/me-ping.unit.test.ts` | CREATE | The probe's decode boundary and its failure mapping |
| `cli/test/acceptance/whoami.spec.ts` | CREATE | The end-to-end walk: a real subprocess, a stubbed server, both output modes |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (one spelling per fact: the route template, the credential-class wire words and the refusal sentence each exist once, and the CLI mirrors the path by constant rather than by literal) · **NDC** (no field, column or trait method written for a reader this diff does not add — `last_used_at` and a credential list stay absent) · **NLR** (the `CliCredentialUnknownSubject` rename is the touch-it-fix-it on a name this diff proves is family-neutral) · **NSQ** (the subject is a bound parameter, never interpolated) · **ORP** (the orphan sweep on every renamed symbol) · **CTM** is NOT engaged — nothing here compares a secret.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — §1 Uniform Resource Locator design and field naming, §4 response shape, §6 document regeneration, §7 route registration, §8 handler signature. The endpoint checklist at the head of that file is run at CHORE(close).
- **`docs/RUST_ERROR_STANDARD.md`** + `dispatch/write_rust.md` — every new fallible signature in `afd_tenant` and `afd_api_tenant`; the crate's `ErrorKind` is private and its `Error` is macro-generated, so the rename lands in `kind.rs` and its maps, never in a hand-written error type.
- **`dispatch/write_ts_adhere_bun.md`** — the TypeScript File Shape Decision at PLAN for `cli/src/commands/whoami.ts`, `const` and import discipline, and the Output service as the only rendering path (no `console.log` in a handler).
- **`docs/LOGGING_STANDARD.md`** — the handler's refusal event names, and the rule that an identity read logs no email.
- **`docs/DOCUMENTATION_RULES.md`** + `docs/CHANGELOG_VOICE.md` — the `~/Projects/docs` pages and the changelog `<Update>` this surface change requires.
- `docs/SCHEMA_CONVENTIONS.md` does NOT apply: no schema file is touched. The read uses `core.users`, `core.tenants` and the `uq_users_oidc_subject` index exactly as they already exist.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — a route template, three credential-class words and one path spelling cross a language boundary | The template lives once in `route/tenant.rs`; the CLI names it once in `api-paths.ts`; the class words are rendered from `PersonCredential` by one match and asserted against that match in the router suite |
| LENGTH GATE (≤350 file / ≤50 function / ≤70) | yes — `route/tenant.rs`, `handler/tenant/mod.rs` and `services/tenant.rs` all grow | Each new concern is its own file (`identity.rs` in three crates); the three edited files gain a variant, a re-export and one trait respectively, and each is measured against the cap before commit |
| MILESTONE-ID GATE | yes — new files under `rustd/` and `cli/src/` | Each new module's header carries `M200_001` where the repository's convention places it |
| LOGGING GATE | yes — the handler logs refusals | Scoped event names beside the existing `cli_credential_*` events; no email, no subject, no credential in any field |
| ERROR REGISTRY | yes — a refusal reaches a caller | No new code is minted: the unknown-subject refusal reuses `AUTH_FORBIDDEN` with the sentence already registered, and the rename touches only internal spellings |
| SCHEMA GUARD · ZIG GATE · UI GATE · DESIGN TOKEN GATE | no — no `schema/*.sql`, `*.zig` or `ui/packages/*` file is touched; the read uses `core.users`, `core.tenants` and `uq_users_oidc_subject` exactly as they exist | N/A |
| GREPTILE GATE | yes — end-of-turn read | The named rule identifiers above are obeyed by construction, and the end-of-turn read runs over this diff |
| Architecture consult | yes — a new public endpoint and a new command | `docs/architecture/user_flow.md` §8.0 is the consult; the login walk there gains the identity line, recorded in Discovery |
| Coverage | yes — both languages | `make test-unit-all` carries the Rust workspace and every TypeScript coverage gate; the CLI's own gate is enforced by `cli/scripts/enforce-coverage.mjs` under it |

## Prior-Art / Reference Implementations

- **Reference (API):** `rustd/crates/afd_api_tenant/src/handler/tenant/cli_credential.rs` — same plane, same subject resolution, same person-class extractor discipline, same `utoipa::path` block shape. The divergence is the extractor: this route takes `PersonIdentity` (`AnyClass`) where the mint takes `FreshSession`, because asking who you are is the one thing every person credential may do.
- **Reference (route row and statement):** `TenantRoute::CliCredentials` in `rustd/crates/afd_http/src/route/tenant.rs` already carries `Scopes::Always(NONE)` and argues in its doc comment why no capability scope can express a principal-mode rule; `SELECT_USER_IDENTITY_BY_SUBJECT` in `rustd/crates/afd_tenant/src/sql/cli_credential.rs` is the narrow subject lookup the new statement extends with a tenant join and two profile columns — a second statement rather than a widened first one, because the mint path must not pay a join for columns it never reads.
- **Reference (CLI):** the 7 Pillars, as `cli/src/commands/auth.ts` already applies them. This spec aligns with **command → handler → errors split** (`cli-tree.ts` registers, `commands/whoami.ts` is the pure Effect, `errors/` carries the refusal), **handler purity** (no `console.log`, no `process.exit` — the Output service renders and the dispatcher maps the exit code), **output as a service** (one Effect, two renderings chosen by `CliConfig.jsonMode`), **structured errors with a suggestion field** (`AuthError` carries `suggestion` and `code`), **the 3-tier pyramid** (unit / in-process integration / subprocess acceptance) and **auto-JSON when piped** (inherited from the bind site's `stdoutIsTtyFromCtx`). No pillar is diverged from.
- **External convention:** GitHub's `GET /user` and Stripe's `GET /v1/account` are the shape this mirrors — the caller's own record under a plural collection with a `me` alias, which is also what `/v1/tenants/me/*` already does here. The stale `GET /v1/me` spelling recorded in `cli/src/lib/me-ping.ts` is NOT adopted: it is neither a plural noun nor a resource, and §1 of the Representational State Transfer (REST) guidelines forbids it.

## Sections (implementation slices)

### §1 — The identity read, from subject to profile

The domain half: one store, one statement, one record. `afd_tenant::identity::Identities::profile(subject)` answers the `core.users` row the authenticator's proven subject names, joined to its tenant for the display name a person actually recognises. A subject with no local row is REFUSED, never provisioned — the argument `cli_credential::user_of` already makes, and the reason the refusal constructor is shared rather than duplicated. **Implementation default:** a second statement beside `SELECT_USER_IDENTITY_BY_SUBJECT` rather than a widened one, because the mint and revoke paths read two columns and must not pay a join for five.

- **Dimension 1.1** — `profile` answers `user_id`, `email`, `display_name`, `tenant_id` and `tenant_name` for a subject with a row, taking the tenant from the joined user row rather than from any other copy → Test `test_profile_answers_the_joined_user_and_tenant`
- **Dimension 1.2** — a subject with no `core.users` row is refused with the shared unknown-subject error, and nothing is inserted → Test `test_profile_refuses_an_unknown_subject`
- **Dimension 1.3** — `display_name` is `NULL`-safe: a row with no display name answers `None` rather than an empty string, and the response omits the field → Test `test_profile_carries_no_display_name_when_the_column_is_null`
- **Dimension 1.4** — the unknown-subject error kind, detail constant and constructor carry one family-neutral name, and the wire code and sentence are byte-identical to what the credential family already answers → Test `test_unknown_subject_refusal_is_unchanged_on_the_wire`

### §2 — The route, the guard and the document

The edge half: the route row, the handler, the plane arm and the regenerated document. The row declares `Guard::Bearer` with `Scopes::Always(NONE)` — the credential is the claim, and a capability requirement here is what makes the existing login probe misreport a person who holds no billing capability. The handler takes `PersonIdentity`, so a runner token cannot reach it and every person credential can. The credential class in the response is rendered from the proven `PersonCredential` by one match, never from anything the caller sent. **Implementation default:** template `/v1/users/me`, because §1 requires a plural noun resource and `/v1/tenants/me/*` establishes the `me` alias segment; the stale `/v1/me` spelling in the CLI comment is superseded in the same diff that removes the comment.

- **Dimension 2.1** — `GET /v1/users/me` is mounted and answers 200 with the profile for each of the three person credential classes → Test `test_users_me_answers_every_person_credential_class`
- **Dimension 2.2** — the response's `credential` field reads `session_token`, `tenant_api_key` or `cli_credential` according to the proven class, and the match is total → Test `test_credential_class_renders_from_the_proven_principal`
- **Dimension 2.3** — `scopes` carries the principal's resolved capabilities in their wire spelling, and an empty set renders as an empty list rather than an absent field → Test `test_scopes_render_in_wire_spelling`
- **Dimension 2.4** — a runner-plane token is refused, and a request carrying no credential is refused, each with the code its guard already declares → Test `test_users_me_refuses_a_runner_and_an_anonymous_caller`
- **Dimension 2.5** — the route requires no capability scope: a principal holding an empty scope set still reads its own identity → Test `test_users_me_needs_no_capability`
- **Dimension 2.6** — `public/openapi.json` carries the operation under the `Users` tag with the bearer scheme the row declares, and equals what the build emits → Test `test_openapi_build_is_the_source`
- **Dimension 2.7** — the handler logs no email, no subject and no credential on any path → Test `test_identity_handler_logs_no_identity_material`

### §3 — `agentsfleet whoami`, and the two probes that were waiting for this route

The client half. `whoami` reads the route and renders it; `login` closes by naming the person, using the identity its post-login probe already fetched, so the answer costs no extra request; `auth status`'s reachability probe moves to the scope-free route. The probe move is a fix, not a tidy: `/v1/tenants/me/billing` requires `billing:read`, so a signed-in person without that capability is currently told the server rejected their token. **Implementation default:** `whoami` is a top-level command and not `auth whoami`, because the name is the muscle memory the request came in as, and no alias is added — `auth status` keeps its own job, which is where the credential came from and whether the target answers.

- **Dimension 3.1** — `agentsfleet whoami` prints the person, tenant, credential class and target Uniform Resource Locator in a human block on a terminal → Test `test_whoami_renders_the_human_block`
- **Dimension 3.2** — `agentsfleet whoami --json` prints the server's fields plus the resolved target, and nothing else → Test `test_whoami_renders_json`
- **Dimension 3.3** — with no credential on disk and none in the environment, `whoami` names the command that fixes it and exits non-zero without a request → Test `test_whoami_refuses_when_nothing_is_signed_in`
- **Dimension 3.4** — a server refusal is surfaced with its own code and the re-authentication suggestion, and a response that decodes to nothing usable is a typed failure rather than a partial render → Test `test_whoami_surfaces_a_refusal_and_a_malformed_body`
- **Dimension 3.5** — `login` reports the person it signed in, and a successful login whose identity read fails still reports success rather than failing on the rendering → Test `test_login_names_the_person_and_survives_an_unnameable_one`
- **Dimension 3.6** — `auth status` probes the scope-free route, so a principal holding no `billing:read` capability reads as authenticated → Test `test_auth_status_does_not_need_billing_capability`
- **Dimension 3.7** — the post-login probe reads the identity route, and its failure still clears the freshly written credential file → Test `test_me_ping_reads_identity_and_still_rolls_back`

### §4 — The published surface

The documentation half, on its own branch in `~/Projects/docs`. A new endpoint and a new command are both published surfaces, so neither ships undocumented.

- **Dimension 4.1** — `cli/agentsfleet.mdx` carries `agentsfleet whoami` in the command table and in the help transcript → Test `docs_cli_page_carries_whoami`
- **Dimension 4.2** — `changelog.mdx` gains one `<Update>` following `docs/CHANGELOG_VOICE.md`, claiming nothing the diff does not ship → Test `docs_changelog_entry_matches_the_diff`
- **Dimension 4.3** — `docs/architecture/user_flow.md` §8.0 records that login closes by naming the person, and that the identity read is the scope-free probe both auth paths use → Test `docs_architecture_walk_names_the_identity_read`

## Interfaces

```
GET /v1/users/me
  Guard:  Bearer — a session token, an agt_t tenant api-key, or an afc_ command-line
          credential. A runner token (agt_r) is refused. No capability scope required.
  200 →  { "user_id": "<uuidv7>", "email": "person@example.com",
           "display_name": "Ada Lovelace",        // omitted when the column is NULL
           "tenant_id": "<uuidv7>", "tenant_name": "Ada's Workshop",
           "credential": "cli_credential",        // | session_token | tenant_api_key
           "scopes": ["fleet:read", "secret:read"] }   // [] when the claim resolved none
  401 →  no credential, or one this route does not accept (problem+json envelope)
  403 →  UZ-AUTH-001, detail "Authenticated subject has no user record"
  429 / 500 / 503 → the standard envelope

afd_tenant::identity
  Identities::new(database: Db) -> Self
  Identities::profile(&self, subject: &str) -> Result<Profile>
  Profile { user: Uuid7, email: String, display_name: Option<String>,
            tenant: Uuid7, tenant_name: String }

afd_http::services::CallerProfiles      // new member on TenantSurface: profiles()
  profile(&self, subject: &str) -> impl Future<Output = afd_tenant::Result<Profile>> + Send

cli/src/commands/whoami.ts
  whoamiEffect: Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output>
cli/src/lib/me-ping.ts
  readIdentity(token: Redacted<string>) -> Effect<CallerIdentity, MeValidationError, HttpClient>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown subject | A live credential whose identity-provider subject has no `core.users` row — an erased account, or a credential minted against another deployment's directory | 403 `UZ-AUTH-001`, detail "Authenticated subject has no user record". Nothing is inserted. The CLI prints the sentence and the re-authentication suggestion, and exits non-zero |
| No credential at all | `agentsfleet whoami` on a machine that never logged in, or after `logout` | The command refuses locally, names `agentsfleet login`, and makes NO request — the same shape `auth status` uses today |
| Runner token presented | An `agt_r` credential on a person route | Refused by the plane rule before the handler runs; the caller sees the guard's own code, not an identity error |
| Malformed response body | A proxy or a version skew answers 200 with something that is not a profile | The CLI's decode boundary fails typed, names the field that was missing, and renders nothing partial |
| Datastore unreachable | Postgres down or the pool exhausted during the read | 503 with the standard envelope; the CLI reports unreachable rather than unauthenticated, so an operator does not delete a working credential over an outage. A principal whose claim resolved to no capabilities is NOT a failure: the route needs none and answers 200 with `scopes: []` |
| Identity read fails after a successful mint | The post-login probe cannot reach the route, or is refused | The credential file is cleared before the error propagates — the behaviour `rollbackOnMeFailure` already guarantees, preserved verbatim |
| Identity unnameable at login | The mint succeeded, the probe succeeded, and the body carried no usable display name | `login` still reports success and falls back to the email, then to the generic line. A rendering gap never fails a completed login |

## Invariants

1. **No capability scope gates the identity read** — enforced by the route row's `Scopes::Always(NONE)` and asserted by a router-suite case that drives a principal with an empty scope set to a 200. A future edit adding a scope fails that test.
2. **The credential class comes from the proven principal, never from the request** — enforced by a total match over `PersonCredential` inside the handler; a new credential class does not compile until the match names it.
3. **A proven subject is never provisioned on the read path, and resolves to at most one user** — enforced by the statement: the identity module issues one `SELECT` and holds no writer (rubric row R6), the integration suite asserts the row count is unchanged after a refused read, and `uq_users_oidc_subject` makes a second match unrepresentable.
4. **No identity material reaches a log line** — enforced by the handler's hoisted log fields, which carry an event name and a request identifier and no profile value, and by a unit case that captures the emitted fields and asserts the set.
5. **One spelling of the route** — enforced by `cli/src/lib/api-paths.ts` holding the only CLI literal and every CLI call site importing it; the existing const-name audit (`cli/scripts/audit-const-names.mjs`) runs over the new file.
6. **The published document is the build's output** — enforced by `test_openapi_build_is_the_source`, which fails on a hand-edited `public/openapi.json`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cli_command_executed` | product | Every CLI invocation, `whoami` included, through the existing instrumentation seam | command label, exit code, duration | The seam carries no arguments and no output; no email, password, token, One-Time Password (OTP) or key material | `test_whoami_emits_the_command_event` |
| `identity_read_failed` | ops | The handler refuses or the store reports | error code, request identifier, scoped event name | No email, no subject, no tenant name, no credential | `test_identity_handler_logs_no_identity_material` |

No funnel changes: `whoami` is a read that joins no journey, and `login`'s existing events are untouched — the new success line renders from data the login path already fetched. Discovery records the metrics review verdict at VERIFY.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_profile_answers_the_joined_user_and_tenant` | A seeded user under a named tenant → all five fields, tenant taken from the joined user row |
| 1.2 | integration | `test_profile_refuses_an_unknown_subject` | Subject `user_nobody` → the shared unknown-subject error; `core.users` row count unchanged |
| 1.3 | integration | `test_profile_carries_no_display_name_when_the_column_is_null` | A user row with `display_name IS NULL` → `None`, and the serialized body omits the key |
| 1.4 | unit | `test_unknown_subject_refusal_is_unchanged_on_the_wire` | The renamed kind → `UZ-AUTH-001` and the byte-identical detail sentence |
| 2.1 | integration | `test_users_me_answers_every_person_credential_class` | One request per class against the built router → 200 and the same profile, `credential` differing per class |
| 2.2 | unit | `test_credential_class_renders_from_the_proven_principal` | Each `PersonCredential` variant → its wire word; a session token with a workspace ceiling still reads `session_token` |
| 2.3 | unit | `test_scopes_render_in_wire_spelling` | A scope set of three → the three wire strings; an empty set → `[]`, not absent |
| 2.4 | integration | `test_users_me_refuses_a_runner_and_an_anonymous_caller` | An `agt_r` token → refused by the plane rule; no `Authorization` header → 401 |
| 2.5 | integration | `test_users_me_needs_no_capability` | A principal whose claim resolved to an empty scope set → 200 with its own profile |
| 2.6 | unit | `test_openapi_build_is_the_source` | The committed artifact equals the emitted document, byte for byte after the trailing newline |
| 2.7 | unit | `test_identity_handler_logs_no_identity_material` | Captured log fields over the success and both refusal paths → no field whose value is an email, a subject, a tenant name or a credential |
| 3.1 | unit | `test_whoami_renders_the_human_block` | A stubbed 200 with every field → the section header and one line per rendered key, in order |
| 3.2 | unit | `test_whoami_renders_json` | `--json` over the same body → one JSON object carrying the server's fields plus the resolved target Uniform Resource Locator |
| 3.3 | unit | `test_whoami_refuses_when_nothing_is_signed_in` | Empty credential store and empty environment → `AuthError`, the `agentsfleet login` suggestion, and zero HTTP calls recorded |
| 3.4 | unit | `test_whoami_surfaces_a_refusal_and_a_malformed_body` | A 403 with `UZ-AUTH-001` → that code preserved; a 200 carrying `{}` → a typed decode failure and no partial output |
| 3.5 | unit | `test_login_names_the_person_and_survives_an_unnameable_one` | A profile with a display name → the name in the success line; a profile with none → the email; a body with neither → the generic line and still an exit code of zero |
| 3.6 | unit | `test_auth_status_does_not_need_billing_capability` | The recorded request path on the probe → the identity route, not the billing route |
| 3.7 | unit | `test_me_ping_reads_identity_and_still_rolls_back` | A refused identity read after a persist → the credential clear ran and the original failure propagated |
| | e2e | `whoami.spec.ts` | A real `agentsfleet whoami` subprocess against the acceptance lane's stub server → exit 0 and the person's email on stdout; and with no credential, a non-zero exit naming `agentsfleet login` |
| | unit (regression) | `test_cli_credential_mint_still_reads_the_narrow_statement` | The mint path's subject lookup is unchanged: two columns, no join, same statement constant |
| | unit (regression) | `test_auth_status_still_reports_an_unreachable_target` | A target that refuses every route → `unreachable`, not `unauthorized` — the existing classification survives the probe move |
| 4.1 | manual | `docs_cli_page_carries_whoami` | The built CLI's `--help` transcript and the page's command table both name `agentsfleet whoami`; evidence is the docs-repo Pull Request link |
| 4.2 | manual | `docs_changelog_entry_matches_the_diff` | Every claim in the new `<Update>` resolves to a shipped file in this diff; no marketing words per `docs/CHANGELOG_VOICE.md` |
| 4.3 | manual | `docs_architecture_walk_names_the_identity_read` | §8.0's login walk names the identity read and the person-naming success line, and matches the shipped code |

Idempotency and replay rows are N/A: the endpoint is a `GET` with no side effect and no retry semantics of its own, and the CLI's existing retry policy applies unchanged.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A signed-in terminal names the person it is signed in as (§3) | `cd cli && bun run build && node dist/bin/agentsfleet.js whoami --json` | exit 0; stdout parses as JSON carrying `email` and `tenant_name` | P0 | |
| R2 | Nothing signed in refuses locally and names the fix (§3) | `AGENTSFLEET_CONFIG_DIR=$(mktemp -d) node cli/dist/bin/agentsfleet.js whoami` | non-zero exit; stderr contains `agentsfleet login` | P0 | |
| R3 | The route needs no capability scope (§2) | `cd rustd && cargo test -p afd_api needs_no_capability` | exit 0 | P0 | |
| R4 | The published document is the build's output (§2) | `cd rustd && cargo test -p afd_api --features openapi,test-util test_openapi_build_is_the_source` | exit 0 | P0 | |
| R5 | `login` closes by naming the person (§3) | `cd cli && bun test test/login-effect.unit.test.ts` | exit 0 | P1 | |
| R6 | The identity read holds no writer (§1) | `grep -cE '\b(INSERT\|UPDATE\|DELETE)\b' rustd/crates/afd_tenant/src/sql/identity.rs` | `0` | P0 | |
| R7 | One CLI spelling of the route (§3) | `grep -rn '/v1/users/me' cli/src \| grep -v api-paths.ts` | no output | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S4 | Integration lane green (live Postgres and Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version files agree | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S8 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** every declared `conform` and `verify.*` invocation from `.oracle/orly.json` is copied verbatim into a Verify cell above with a mechanically checkable Expected. Command timing is `dispatch/lifecycle.md`'s; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows cite the final `orly gate pr` results in Pull Request Session Notes. Every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ needs an Indy-acked deferral quote in Discovery. A P0 whose scope moves whole into a named successor spec is marked `MOVED to M{N}_{NNN} R{n}` and is not ❌, on the three conditions `docs/TEMPLATE.md` sets: the successor carries the row, both specs record the mapping, and Discovery carries Indy's verbatim authorisation. A MOVED row never renders ✅.

## Dead Code Sweep

**1. Orphaned files** — N/A, no files deleted. `cli/src/lib/me-ping.ts` is rewritten in place: its caller, its rollback behaviour and its error type all survive, and only the path it reads and the value it returns change.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `CliCredentialUnknownSubject` | `grep -rn -w "CliCredentialUnknownSubject" rustd/ \| head` | 0 matches |
| `DETAIL_CLI_CREDENTIAL_UNKNOWN_SUBJECT`, `cli_credential_unknown_subject` | `grep -rnE -w "DETAIL_CLI_CREDENTIAL_UNKNOWN_SUBJECT\|cli_credential_unknown_subject" rustd/ \| head` | 0 matches |
| `ME_PING_PATH`, `pingMe` | `grep -rnE -w "ME_PING_PATH\|pingMe" cli/src cli/test --include='*.ts' \| head` | 0 matches |

## Out of Scope

- **A credential list** — "which terminals hold a live credential for me" is a real question and a different endpoint (`GET /v1/cli-credentials`, documented-and-unserved since the port). Adding it here would put a second resource behind one command.
- **Dashboard identity, and `auth status` growing identity fields** — the web app resolves the signed-in person from the identity provider directly and needs nothing here; the two commands keep separate jobs, which is where the credential came from and whether the target answers, versus who the credential belongs to.
- **A `--tenant` or account switcher** — one account per credential today; switching is a login, and a selector implies multi-tenant membership that `core.memberships` does not yet serve.
- **Any alias spelling** — no `auth whoami`, no `/v1/me` compatibility route. The repository's no-compatibility-aliases rule holds, and nothing shipped under either spelling.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator comes back to a terminal they left open on Friday, types `agentsfleet whoami`, and reads their own email and tenant name back. They stop wondering whether the next command will run against the right account.
2. **Preserved user behaviour** — `agentsfleet login`, `logout` and `auth status` all keep their current exit codes, JSON shapes and human output, except that `login`'s success line gains a name and `auth status` stops falsely reporting rejection for a person who holds no billing capability. Every existing script that parses `auth status --json` keeps working: no field is removed or renamed.
3. **Optimal-way check** — This is the direct route. The unconstrained-optimal shape would have the credential itself carry the identity, so the terminal could answer offline; that is rejected on purpose, because a readable credential is a credential whose claims drift from the record behind it, and the repository's whole authentication design resolves capability server-side. Asking the server is the correct cost.
4. **Rebuild-vs-iterate** — Iterate. Nothing legacy to unwind: the endpoint was specified once, never built, and the client already carries the seam that was waiting for it. Unifying subject-to-user resolution across its three current sites is the only refactor available, and it does not block this moment.
5. **What we build** — One `GET` endpoint, one domain read, one CLI command, one line added to `login`, one probe path corrected, and the three published pages.
6. **What we do NOT build** — A credential list (a second resource behind one verb), a tenant switcher (implies membership the schema does not yet serve), an offline identity cache (drifts from the record), and any alias spelling of the command or route (the no-aliases rule).
7. **Fit with existing features** — It compounds with `login`, `logout` and `auth status`, and it is what `doctor` should eventually cite when it reports connectivity. The one feature it must not destabilise is the login flow itself: the post-login probe is load-bearing — it is what deletes a credential that does not authenticate — so the rollback path is preserved verbatim and tested as a regression.
8. **Surface order** — CLI-first, the repository default. The dashboard has no gap here: it already knows who is signed in.
9. **Dashboard restraint** — N/A — no user interface surface changes. Nothing is added to the dashboard, so there is no control to hide.
10. **Confused-user next step** — `agentsfleet whoami` IS the self-serve move for "which account am I on". When it refuses, it names `agentsfleet login`; when the server refuses, it names re-authentication and the target Uniform Resource Locator that may be wrong.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Four Sections along the seams the code already has — domain read, edge and document, client, published surface. The split is not arbitrary: §1 is provable against live Postgres with no router, §2 is provable against the built router with a stubbed store, §3 is provable with layer composition and no server, and §4 is a different repository. Each Section's tests run without the next one existing.
- **Alternatives considered:** (a) **Cache the identity at login and answer `whoami` from disk** — one fewer endpoint, zero server work, and wrong: the cached copy drifts from the record on a rename or an account change, and the command's whole value is being authoritative. (b) **Widen `/v1/tenants/me/billing` to carry identity** — no new route at all, and it puts a person's identity behind a billing capability, which is the exact defect this spec fixes. (c) **Add the profile read to `TerminalCredentials`** — no new service seam, two impls untouched, but it puts a read that is not about credentials into a trait documented as "mint and revoke, and nothing else". The seam is cheap; the lie is not.
- **Patch-vs-refactor verdict:** this is a **patch** because the missing thing is one route and one command, and every seam it needs — the person-class extractor, the subject the authenticator already proved, the service accessor pattern, the CLI's probe indirection — is already in place. The one rename it carries (`CliCredentialUnknownSubject` → `UnknownSubject`) is touch-it-fix-it on a name this diff makes wrong, not a refactor smuggled in. The larger refactor that is genuinely available — unifying the three independent subject-to-user lookups in `cli_credential`, `preference` and `signup` behind one resolver — is real and is NOT taken here; it should be its own workstream, because folding it in would put a change to the login and dashboard read paths inside a Pull Request about a read-only command.
## Discovery (consult log)

- **Consults** — Architecture consult on `docs/architecture/user_flow.md` §8.0 (revision `5eb6f3885`): the login walk records `agentsfleet login # Clerk OAuth → token in ~/.config/agentsfleet/credentials.json` and names no identity read, so the new route conflicts with nothing there and the walk gains a line at DOCUMENT. Naming consult against `docs/REST_API_DESIGN_GUIDELINES.md` §1: `/v1/users/me` chosen over the `/v1/me` spelling recorded in `cli/src/lib/me-ping.ts:5`, because §1 requires a plural-noun resource and `/v1/tenants/me/*` already establishes the `me` alias segment — the stale comment is an agent-authored note in a client file, not a decision of Indy's, and it is removed in the same diff. Source finding, `cli/src/lib/me-ping.ts:5-10`: the post-login probe hits `/v1/tenants/me/billing` and says so, which is why §3 treats the probe move as a fix rather than a tidy. Collision check per §1: no `status`, `state` or `lifecycle_state` field exists on any user resource in `public/openapi.json`, and this endpoint declares no operation-style verb, so the check is moot by construction.
- **Metrics review** — pending, recorded at VERIFY; authoring verdict is that no analytics or funnel playbook update is expected, because `whoami` adds no journey and `login`'s events are untouched.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` per Section and at the boundary, `/orly-write-integration-test` at the boundary (expected to apply — the identity read crosses a module boundary with real Postgres input and output), gstack `/review`, then `orly-babysit-prs` after each push.
- **Deferrals** — none at authoring.
