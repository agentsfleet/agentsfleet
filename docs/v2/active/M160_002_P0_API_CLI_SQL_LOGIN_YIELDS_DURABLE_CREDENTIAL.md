<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M160_002: Login hands the terminal a credential that names the human and outlives the session

**Prototype:** v2.0.0
**Milestone:** M160
**Workstream:** 002
**Date:** Aug 11, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the documented front door to the Command-Line Interface (CLI) yields a credential that dies in under a minute, so every command after `login` fails
**Categories:** API, CLI, SQL
**Batch:** B1 — §1 and §2 are the defect and ship together; §3 and §4 are the two failures a durable credential introduces and cannot ship later
**Branch:** `feat/m136-live-connector-proof` — shared with M136_001 by Indy's call, Aug 11, 2026; both workstreams land from one branch and one worktree
**Test Baseline:** unit=3512 integration=589
**Depends on:** none — no other workstream gates this. The Clerk `api` template lifetime is an operator setting Indy holds (see Discovery); the code path is correct either way and is tested against a stubbed mint
**Provenance:** found live during M136_001 proof work, Aug 11, 2026 — `agentsfleet login` reported success and `auth status` immediately reported `expired: yes`
**Canonical architecture:** `docs/AUTH.md`

---

## Overview

**Goal (testable):** A credential written by `agentsfleet login` still authenticates a command run long after the approved session's own token has expired, identifies the human who logged in, and is refused if presented to a deployment that did not mint it.

**Problem:** `login` runs a correct Elliptic Curve Diffie-Hellman (ECDH) device flow and then persists the wrong thing. What reaches disk is the session JSON Web Token (JWT) recovered from the handshake. No renewal path exists — a search of `cli/src` for `refresh(Token|Session)` and `refresh_token` returns nothing — because the CLI carries no Clerk Software Development Kit (SDK) and structurally cannot refresh a browser-coupled token. The dashboard comment authorising this mint (`ui/packages/app/app/cli-auth/[session_id]/page.tsx`) states the intent as roughly fifteen minutes and, in the same breath, records the template as currently sixty seconds. Both numbers are wrong for a terminal: one strands an operator who steps away, the other strands one who reads the success message. The durable primitive this needs does not exist — the only long-lived credential the platform issues is `core.api_keys`, which is scoped to a **tenant**, so persisting one would silently promote a person's terminal into a tenant-wide credential and reduce attribution to a free-text `created_by` column.

**Solution summary:** A user-scoped credential is introduced — bound to the person, named for the machine, carrying the deployment that minted it, and structurally distinguishable from a session token so that storing the wrong thing fails a check rather than passing review. Login spends its sixty-second window on the one call that outlives it: minting that credential. Because a credential is minted per login rather than reused, two failures arrive with the fix and are answered in the same workstream — credentials would otherwise accumulate live and unrevoked on every re-login, and a durable credential replayed against the wrong deployment no longer self-corrects when the session token would have expired. The handshake, the verification-code surface, and logout's server-side revoke are correct and are not modified.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(auth): login mints a durable user-scoped credential, one per machine
- **Intent (one sentence):** An operator who logs in once keeps using the terminal — as themselves, against the deployment they logged into — until they log out.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/cli-auth/[session_id]/page.tsx` — the approve path, the ECDH encryption performed client-side, and the carve-out comment whose stated intent this workstream makes true.
2. `cli/src/commands/login.ts` — the six-step flow comment at the head is accurate; step 6 is the only step that changes.
3. `cli/src/commands/login-device-flow.ts` — the three CLI-facing endpoints and their exact response shapes; unchanged by this workstream.
4. `schema/240_api_keys.sql` — the tenant-scoped credential this deliberately does **not** reuse, and the shape `250` mirrors for hashing, revocation, and audit columns.
5. `cli/src/services/config.ts` and `cli/src/lib/state.ts` — the credential ladder and the state paths; the base URL survives in neither today.
6. `src/agentsfleetd/session/session_store_redis.zig` — `MAX_VERIFY_ATTEMPTS`; the anti-brute-force property §2 must not weaken.
7. `docs/AUTH.md` — the credential posture this extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/250_cli_credentials.sql` | CREATE | The user-scoped credential, with the partial unique index that makes two live credentials per machine unrepresentable |
| `schema/embed.zig` | EDIT | Register `250` in the embed and the migration array |
| `src/agentsfleetd/state/cli_credentials.zig` | CREATE | Store for mint, lookup-by-hash, list-for-user, revoke |
| `src/agentsfleetd/state/sql.zig` | EDIT | Statements for the new store, per the SQL Statement Modules rule |
| `src/agentsfleetd/http/handlers/auth/cli_credentials.zig` | CREATE | Mint, list, and revoke endpoints |
| `src/agentsfleetd/http/middleware/authenticate.zig` | EDIT | The credential joins the accepted principal set, resolving to a user rather than a tenant |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Registered codes for exchange failure, deployment mismatch, and revoked credential |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | User-facing entries for the new codes. Corrected at EXECUTE from `error_entries_runtime.zig`, which the drafted table named: `UZ-AUTH-023`/`024` live here, so their sibling does too |
| `src/agentsfleetd/db/index_usage_integration_test.zig` | EDIT | Declares the two indexes `250` creates; the suite refuses an undeclared index |
| `src/agentsfleetd/queue/redis_pool_test.zig` | EDIT | Out of scope, folded in on Indy's Aug 12 call (see Discovery): the acquire-overshoot bound is sized by the bug it guards, so a loaded machine stops failing a correct pool |
| `cli/src/commands/login.ts` | EDIT | Step 6 exchanges the session token before anything is persisted |
| `cli/src/commands/login-exchange.ts` | CREATE | The mint-and-revoke exchange, split out so `login.ts` holds its length cap |
| `cli/src/commands/logout.ts` | EDIT | Revokes CLI credentials only, and clears the stored deployment |
| `cli/src/services/credentials.ts` | EDIT | The persisted shape carries the credential, its identifier, and its deployment; validates the prefix on load |
| `cli/src/lib/state.ts` | EDIT | Stored state gains the deployment the credential belongs to |
| `cli/src/services/config.ts` | EDIT | The stored deployment joins the target ladder, below the flag and the environment variable |
| `cli/src/services/http-client.ts` | EDIT | A request refuses a credential minted against another deployment |
| `cli/src/lib/api-paths.ts` | EDIT | The credential paths join the centralised map |
| `cli/src/constants/cli-credential.ts` | CREATE | The prefixes, the credential's declared shape, and the machine-name grammar — single-sourced here because the client and the daemon must agree on them byte-for-byte (RULE UFS) |
| `cli/src/cli.ts` | EDIT | The empty-credentials literal moves to `state.ts`; it existed in three places and each gained a field |
| `cli/test/login-exchange.unit.test.ts` | CREATE | The load-shape refusals, the machine-name grammar, and the orphan sweep |
| `cli/test/login.acceptance.spec.ts` | EDIT | The mint is stubbed in the device-flow fixture; the persistence assertions flip from the session token to the credential |
| `cli/test/{api-url-resolution,config-precedence,connector,coverage-fill,services-coverage-fillers}` | EDIT | Fixtures seeded a JWT-shaped value into the credential field, which the load check now refuses — each reseeded with a well-formed credential |
| `cli/test/acceptance/fixtures/state-dir.ts` | EDIT | Adds the empty-state-dir fixture, the inverse of the stubbed one |
| `cli/test/acceptance/help-and-errors.spec.ts` | EDIT | Isolates `AGENTSFLEET_STATE_DIR`, so the auth-guard assertions stop reading the developer's real login |
| `cli/test/acceptance/flags-and-env.spec.ts` | EDIT | Same isolation; this workstream changes the persisted shape these specs observe |
| `cli/test/setup.ts` | EDIT | The preload defaults the runner to an empty state dir, so logged-out is the baseline for in-process tests |
| `cli/test/json-contract.test.ts` | EDIT | Its auth-required test takes its own state dir, since sibling files write credentials into the current one |
| `cli/test/helpers-cli-state.ts` | EDIT | Gains `useFreshStateDir` (hook-scoped) and `preserveStateDirEnv` (save/restore only) beside the existing `withFreshStateDir` — one home for state-dir isolation |
| `cli/test/{help,cli-alignment,workspace-create,doctor-json,api-key-env,services-credentials,login-logout-identity,login-helpers-funcfill,handlers-bind-wrap-effect,telemetry/identity}` | EDIT | Ten files migrated off private copies of the same scope guard onto the shared helpers; net −132 lines |
| `public/openapi/paths/auth.yaml`, `public/openapi.json` | EDIT | The three new endpoints are public surface |
| `docs/AUTH.md` | EDIT | Records the second credential class and how it differs from the tenant key |
| `~/Projects/docs/changelog.mdx` | EDIT | User-visible: login stops expiring immediately |
| `~/Projects/docs/cli/configuration.mdx` | EDIT | Documents the stored deployment and both precedence ladders |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (no refresh machinery: a durable credential needs none), **NLR** (the touched files carry comments asserting a JWT is persisted, which stop being true), **UFS** (the credential prefix, the machine-name grammar, and the new error codes repeat across surfaces), **ORP** (the session-token persistence path retires with no orphaned reader), **VLT** (credential material never reaches a log, an error body, or telemetry; only a hash is stored), **CTX** (a credential is bound to the deployment that minted it and must not cross), **SGR** (grants accompany the new table), **STS** (no static strings in Data Definition Language), **ITF** (immutability and revocation held by the schema, not by store discipline).
- **`~/Projects/dotfiles/dispatch/write_zig.md`** — the new store and handler: pg-drain, tagged-union results, `errdefer` placement, both Linux cross-compiles.
- **`~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md`** — TypeScript file-shape verdict at PLAN; `const` and import discipline.
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — `250` is a new single-concern file in the identity layer; edited in place, never `ALTER`ed.
- **`~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md`** — three new routes; typed refusals, no bare 500.
- **`~/Projects/dotfiles/docs/LOGGING_STANDARD.md`** — mint, revoke, and refusal are emit surfaces.
- **`docs/AUTH.md`** — read before touching the credential path.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — one new table | new single-concern file; no `ALTER`, no `DROP`; lands with `embed.zig` and the migration array in one commit |
| ZIG GATE | yes — new store, handler, middleware edit | pg-drain audit, tagged-union results, `errdefer` on every error path, both Linux targets |
| PUB / Struct-Shape | yes — new store and handler surface | shape verdict per new pub before the first call site lands |
| File & Function Length (≤350/≤50/≤70) | yes — `login.ts` gains a step; `authenticate.zig` gains a principal | the exchange and the credential resolver land as their own files, never as growth |
| UFS (repeated/semantic literals) | yes — prefix, machine-name grammar, error codes | one named constant per literal per file; codes shared verbatim between registry and runtime |
| ERROR REGISTRY | yes — three new refusal classes | registered codes with negative tests |
| LOGGING / LIFECYCLE | yes — new emit surface and a store lifecycle | structured events carrying identifiers only; init/deinit on the store |
| TypeScript FILE SHAPE DECISION | yes — one new CLI module | shape verdict at PLAN |
| UI Substitution / DESIGN TOKEN | no — no user interface files in scope | N/A |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/cli` (Supabase CLI, Go). `internal/utils/access_token.go:71-98` persists a durable Personal Access Token (PAT), never a session token — the step we get wrong. Its `AccessTokenPattern` (`access_token.go:16`, `^sbp_(oauth_)?[a-f0-9]{40}$`) is validated on **load**, not only on save, which is the mechanism §1 adopts so that storing a session token fails a check rather than passing review. Its resolution ladder (`access_token.go:35-54`) is the shape §4 mirrors.
- **Reference:** `schema/240_api_keys.sql` — the hashing, revocation, and audit column shape `250` mirrors. Deliberately **not** reused: `240:20` binds `tenant_id NOT NULL` and carries `created_by` as free text, so it identifies a tenant, not a person.
- **Divergence — we revoke, they do not.** `internal/logout/logout.go` deletes locally and never calls the server, so a Supabase PAT outlives its own logout and every re-login leaves the prior token live. §3 rejects that lifecycle while keeping the storage shape.
- **Divergence — the public key never rides the URL.** Supabase puts `public_key` in the browser query string (`login.go:197`), exposing it to history, referrer headers, and access logs. Ours is posted server-side and fetched by `session_id`. Preserved, not changed.

## Sections (implementation slices)

### §1 — A credential that names the human

The platform issues exactly one durable credential today and it belongs to a tenant, so a terminal holding one acts as the whole tenant and the audit trail records a free-text string where a person should be. This slice adds the missing primitive. **Implementation default:** one live credential per `(user, machine)` enforced by a partial unique index rather than by application logic, because the rule that must never break is the one the database refuses to represent. The credential carries a fixed prefix and is validated on load, so a session token written into that field is rejected at read time.

- **Dimension 1.1** — a credential resolves to the user who created it, never to a tenant-wide principal → Test `test_credential_resolves_to_its_user` — **DONE**
- **Dimension 1.2** — two live credentials for one `(user, machine)` cannot be created → Test `test_second_live_credential_per_machine_is_refused` — **DONE**
- **Dimension 1.3** — only a hash is stored; the credential itself is unreadable from the row → Test `test_row_holds_no_recoverable_credential` — **DONE**
- **Dimension 1.4** — a value lacking the credential prefix is refused on load rather than sent → Test `test_non_prefixed_value_is_refused_on_load` — **DONE**
- **Dimension 1.5** — a revoked credential authenticates nothing → Test `test_revoked_credential_is_refused` — **DONE**
- **Dimension 1.6** — the row records the machine and address that minted it, written once at mint and never on the authenticate path → Test `test_mint_records_attribution_and_auth_path_writes_nothing` — **DONE**

### §2 — Login spends its sixty seconds on something that lasts

The recovered session token is valid for roughly one minute — ample for exactly one call. That call mints the §1 credential, and the credential, not the session token, is what reaches disk. **Implementation default:** the exchange completes before anything is persisted, so a failed mint leaves the operator logged out and told why, rather than logged in with a credential already dead. *(Use case 1: CLI login.)*

- **Dimension 2.1** — the value written by login is a §1 credential, and no session token reaches disk → Test `test_login_persists_credential_not_session_token` — **DONE**
- **Dimension 2.2** — a credential written by login authenticates a call issued after the session token's own lifetime has elapsed → Test `test_credential_outlives_the_session_window`
- **Dimension 2.3** — the mint endpoint accepts the session token as its authorisation, so the exchange is possible within the window → Test `test_mint_accepts_session_token_auth` — **DONE**
- **Dimension 2.4** — a failed exchange persists nothing and reports a registered code, never falling back to the short-lived token → Test `test_failed_exchange_persists_nothing` — **DONE**
- **Dimension 2.5** — the retired session-token persistence path has no remaining caller → Test `test_session_token_persistence_has_no_caller` — **DONE**
- **Dimension 2.6** — after login, listing fleets succeeds using the stored credential → Test `test_login_then_list_fleets_succeeds` *(use case 5)*

### §3 — One machine, one live credential; logout ends it

Minting per login accumulates credentials — the defect the reference implementation ships. Because the credential is now durable, an orphaned one is live indefinitely rather than for a minute. A login revokes what the same machine left behind; a logout revokes this terminal's credential and deliberately leaves browser sessions alone, because the browser holds a different credential class that refreshes through Clerk and signing out of a terminal must not sign a person out of the dashboard they are reading. **Implementation default:** revoke-then-mint rather than reuse, because a credential's secret is returned once at creation and cannot be recovered later.

- **Dimension 3.1** — a second login from the same machine leaves exactly one live credential for it → Test `test_relogin_leaves_one_live_credential` *(use case 3)*
- **Dimension 3.2** — a login on one machine leaves another machine's credential live → Test `test_other_machines_credential_survives_login`
- **Dimension 3.3** — logout revokes this machine's credential server-side and clears local state → Test `test_logout_revokes_and_clears` *(use case 2)*
- **Dimension 3.4** — logout leaves browser sessions untouched → Test `test_logout_does_not_revoke_browser_session` *(use case 7)*
- **Dimension 3.5** — logout with no stored credential reports it and exits zero → Test `test_logout_when_logged_out_is_idempotent` *(use case 4)*
- **Dimension 3.6** — after logout, listing fleets refuses locally without a network call → Test `test_list_after_logout_refuses_locally` *(use case 6)*
- **Dimension 3.7** — a revoke that fails does not abort login; the orphaned identifier is reported → Test `test_failed_revoke_reports_and_continues`

### §4 — A credential remembers which deployment minted it

Stored state records the credential and the workspaces and nothing about where either came from, so the base URL is re-resolved from the environment every invocation and falls back to production. An operator who logs into a development deployment and runs any later command reaches production with a credential it never issued. A durable credential makes this strictly worse, because the mismatch stops self-correcting after sixty seconds. **Implementation default:** the deployment is stored beside the credential and compared before a request leaves, because a credential and the deployment that minted it are one fact.

- **Dimension 4.1** — after login, a command with no flag and no environment variable reaches the deployment that was logged into → Test `test_stored_deployment_is_the_default`
- **Dimension 4.2** — a credential is refused against another deployment before the request leaves the process → Test `test_credential_refused_against_other_deployment`
- **Dimension 4.3** — the target ladder keeps its order: flag, then environment variable, then stored deployment, then built-in default → Test `test_target_ladder_order_unchanged`
- **Dimension 4.4** — the credential ladder keeps its order: `AGENTSFLEET_API_KEY`, then the stored credential → Test `test_credential_ladder_order_unchanged`
- **Dimension 4.5** — logout clears the stored deployment with the credential → Test `test_logout_clears_stored_deployment`

## Interfaces

```
NEW TABLE  core.cli_credentials (schema 250)
  id, user_id -> core.users, tenant_id -> core.tenants, machine_name,
  credential_hash, credential_prefix, deployment, created_at,
  created_from_address, revoked_at
  UNIQUE (user_id, machine_name) WHERE revoked_at IS NULL
        -- one live credential per machine is unrepresentable, not merely
        -- unwritten; §3.1 rests on the index, not on store discipline
  Revocation is the only mutation this row ever takes, so there is no
  `updated_at` (it would be redundant with `revoked_at`) and no `last_used_at`
  (a column provisioned for stamping that has not shipped is speculative,
  RULE NDC — and stamping on the authenticate path would turn the hottest
  indexed read in the system into a write).
  Attribution is a MINT-TIME fact: machine_name and created_from_address are
  written once, at creation, and the authenticate path writes nothing at all.
  Sharing stays visible without any per-request bookkeeping, because a shared
  credential is minted on the sharer's own machine and therefore arrives as a
  second live row under one user_id carrying a different machine_name.

NEW  POST   /v1/cli-credentials          mint; auth: session token OR credential
NEW  GET    /v1/cli-credentials          list this user's live credentials
NEW  DELETE /v1/cli-credentials/{id}     revoke one of this user's credentials

UNCHANGED  POST /v1/auth/sessions, GET /v1/auth/sessions/{id},
           POST /v1/auth/sessions/{id}/verify — the handshake is untouched.
UNCHANGED  /v1/api-keys — tenant keys keep their meaning and their callers.

PERSISTED  credentials.json carries the credential, its identifier, and the
           deployment that minted it. Mode stays 0600. The session token is
           never written.

TWO LADDERS, deliberately separate — one selects a credential, one selects a
target, and they do not order against each other:
  credential:  AGENTSFLEET_API_KEY  ->  stored credential
  target:      --api  ->  AGENTSFLEET_API_URL  ->  stored deployment
               ->  built-in default

REFUSALS   registered codes; typed, never silent
           UZ-AUTH-0xx  credential exchange failed — nothing persisted
           UZ-AUTH-0xx  credential belongs to a different deployment
           UZ-AUTH-0xx  credential revoked
           (exact numbers assigned from the registry at EXECUTE)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Exchange fails | The mint call errors, or the session expired mid-flow | Login fails with the registered code; nothing persisted; the operator is told to log in again |
| Session expires before exchange | The operator leaves the verification prompt open past the window | The same refusal, naming the expiry rather than a generic failure |
| Revoke fails during re-login | The prior credential cannot be revoked | Login continues and succeeds; the orphaned identifier is reported for manual revocation |
| Deployment mismatch | A stored credential is used against another deployment | Refused before the request leaves the process, under the registered code |
| Credential revoked elsewhere | An operator revokes it from another terminal or the dashboard | The next command answers the revoked code and points at login |
| Concurrent logins, one machine | Two logins race on the same `(user, machine)` | The partial unique index refuses the losing insert, which rolls back its own revoke; exactly one credential is live and the loser is answered a registered code |
| Mint fails after the revoke | A transient datastore error, or a lost index race, hits the insert | Revoke and insert are one transaction, so the rollback leaves the prior credential live — a failed re-login never leaves the operator with nothing |
| Stored state absent or unreadable | A hand-deleted or corrupt state file | Treated as logged out, never as a silent fallback to another deployment |
| Session token written into the credential field | A regression in the persistence path | Refused on load by the prefix check, before any request carries it |
| Tenant key presented where a user credential is required | A caller substitutes an `/v1/api-keys` value | Resolves as a tenant principal and is refused by any route requiring a user principal |

## Invariants

1. **A CLI credential always resolves to a user** — enforced by `user_id NOT NULL` and by the middleware's principal type, so a tenant-scoped value cannot satisfy a user-scoped route.
2. **One live credential per machine** — enforced by the partial unique index; a second is refused by Postgres, not by a code path that could be skipped.
3. **No session token is ever written to disk** — enforced on load by the prefix check, so a regression fails at read rather than silently persisting.
4. **A request never carries a credential minted elsewhere** — enforced in the client's send path, the single point every request passes through.
5. **Login never reports success without a live credential** — enforced by ordering: mint precedes persistence, persistence precedes the success message.
6. **The row never holds a recoverable credential** — only a hash and a display prefix are stored, mirroring `240`.
7. **Credential material never enters a log, an error body, or telemetry** — enforced by the redaction wrapper, asserted by a test scanning emitted output across every path.
8. **The verify attempt ceiling is unchanged** — `MAX_VERIFY_ATTEMPTS` stays at its current value; this workstream must not widen the brute-force surface it bounds.
9. **The credential is drawn from a cryptographically secure source at full entropy** — enforced by test against the generator, not by convention. The stored digest is plain and unsalted, which is safe only while the input is unguessable; a generator that ever drew from a clock, a counter, or a non-cryptographic source would silently turn the digest into a reversible record of live credentials.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cli_login_completed` | CLI | Login persists a durable credential | outcome, credential identifier, deployment host | no credential material, no session token, no verification code | `test_login_persists_credential_not_session_token` |
| `cli_credential_exchange_failed` | CLI | The mint call fails | reason, registered code | no credential material, no response body | `test_failed_exchange_persists_nothing` |
| `cli_prior_credential_revoked` | CLI | A previous credential for this machine is revoked | count, identifiers | no credential material | `test_relogin_leaves_one_live_credential` |
| `cli_deployment_mismatch_refused` | CLI | A credential is refused against another deployment | stored host, requested host | no credential material | `test_credential_refused_against_other_deployment` |
| `cli_credential_minted` | ops | The daemon mints a credential | user id, machine name, deployment | no credential material | `test_credential_resolves_to_its_user` |
| credential mint attribution | ops | A credential is minted at login | machine name, creating address, deployment, timestamp | operator-only; written once at mint, never on the authenticate path; no request path or payload | `test_mint_records_attribution_and_auth_path_writes_nothing` |

The existing login-completed analytics event keeps its name and its position in the flow; only its properties change. No funnel or playbook update is required — the event fires at the same point in the same flow.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_credential_resolves_to_its_user` | A request carrying the credential resolves the creating user, and a tenant-scoped route does not widen it |
| 1.2 | integration | `test_second_live_credential_per_machine_is_refused` | A second insert for one `(user, machine)` with no revocation is refused by the index |
| 1.3 | integration | `test_row_holds_no_recoverable_credential` | The stored row's columns cannot reconstruct the issued value |
| 1.4 | unit | `test_non_prefixed_value_is_refused_on_load` | A stored value without the prefix is refused at read, before any request |
| 1.5 | integration | `test_revoked_credential_is_refused` | A revoked credential answers the registered code |
| 1.6 | integration | `test_mint_records_attribution_and_auth_path_writes_nothing` | A mint records machine and address; a hundred authenticated requests afterwards leave every column byte-identical, `last_used_at` still NULL |
| 2.1 | unit | `test_login_persists_credential_not_session_token` | After a stubbed flow, the persisted value is the credential and the session token appears nowhere in the file |
| 2.2 | integration | `test_credential_outlives_the_session_window` | A credential authenticates a call issued after the session token's lifetime has elapsed |
| 2.3 | integration | `test_mint_accepts_session_token_auth` | A mint authorised by a session token succeeds, so the exchange is possible inside the window |
| 2.4 | unit | `test_failed_exchange_persists_nothing` | With the mint failing, no credential file is written and the registered code is returned |
| 2.5 | unit | `test_session_token_persistence_has_no_caller` | The retired persistence symbol has zero non-test references |
| 2.6 | integration | `test_login_then_list_fleets_succeeds` | After login, a fleet list returns the workspace's fleets |
| 3.1 | integration | `test_relogin_leaves_one_live_credential` | Two logins from one machine leave exactly one live credential for it |
| 3.2 | integration | `test_other_machines_credential_survives_login` | A credential named for another machine is still live after a login |
| 3.3 | integration | `test_logout_revokes_and_clears` | After logout the credential is revoked server-side and no local state remains |
| 3.4 | integration | `test_logout_does_not_revoke_browser_session` | A browser session authenticated before logout still authenticates after it |
| 3.5 | unit | `test_logout_when_logged_out_is_idempotent` | Logout with no credential reports it and exits zero, with no network call |
| 3.6 | unit | `test_list_after_logout_refuses_locally` | A list after logout refuses without issuing a request |
| 3.7 | unit | `test_failed_revoke_reports_and_continues` | With revoke failing, login still succeeds and the orphaned identifier is reported |
| 4.1 | unit | `test_stored_deployment_is_the_default` | With no flag and no environment variable, the resolved target is the stored deployment |
| 4.2 | unit | `test_credential_refused_against_other_deployment` | A stored credential against a different host is refused before send |
| 4.3 | unit | `test_target_ladder_order_unchanged` | Flag outranks environment variable outranks stored deployment outranks default |
| 4.4 | unit | `test_credential_ladder_order_unchanged` | `AGENTSFLEET_API_KEY` outranks the stored credential |
| 4.5 | unit | `test_logout_clears_stored_deployment` | After logout, no stored deployment remains |
| failure | integration | `test_concurrent_logins_leave_one_live_credential` | Two interleaved logins on one machine leave exactly one live credential |
| failure | integration | `test_failed_mint_leaves_the_prior_credential_live` | With the insert failing under an injected fault, the prior credential is still live — the revoke rolled back with it |
| failure | unit | `test_corrupt_state_reads_as_logged_out` | An unreadable state file reads as logged out, never as another deployment |
| failure | integration | `test_tenant_key_refused_on_user_scoped_route` | A tenant API key does not satisfy a route requiring a user principal |
| invariant | unit | `test_no_credential_material_in_emitted_output` | Across login, failure, and refusal paths, no emitted string contains credential or token material |
| invariant | unit | `test_credential_is_full_entropy_from_a_secure_source` | The generator draws from the cryptographic source; a large sample yields no duplicate, no shared prefix beyond the declared one, and passes the declared entropy floor |
| regression | integration | `test_device_flow_handshake_unchanged` | The session, approve, and verify calls are byte-identical to today |
| regression | integration | `test_verify_attempt_ceiling_unchanged` | The verify attempt ceiling is unchanged and still refuses past it |
| regression | integration | `test_tenant_api_keys_unchanged` | `/v1/api-keys` behaviour is identical to before |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The credential names a user, not a tenant (§1) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_credential_resolves_to_its_user` | `1 passed` | P0 | |
| R2 | One machine cannot hold two live credentials (§1) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_second_live_credential_per_machine_is_refused` | `1 passed` | P0 | |
| R3 | A credential outlives the session window (§2) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_credential_outlives_the_session_window` | `1 passed` | P0 | |
| R4 | No session token reaches disk (§2) | `cd cli && bun test --test-name-pattern 'test_login_persists_credential_not_session_token'` | `0 fail` | P0 | |
| R5 | Re-login leaves exactly one live credential (§3) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_relogin_leaves_one_live_credential` | `1 passed` | P0 | |
| R6 | Logout does not touch browser sessions (§3) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_logout_does_not_revoke_browser_session` | `1 passed` | P0 | |
| R7 | A credential never crosses deployments (§4) | `cd cli && bun test --test-name-pattern 'test_credential_refused_against_other_deployment'` | `0 fail` | P0 | |
| R8 | Both ladders keep their order (§4) | `cd cli && bun test --test-name-pattern 'ladder_order_unchanged'` | `0 fail` | P0 | |
| R9 | The handshake is untouched | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_device_flow_handshake_unchanged` | `1 passed` | P0 | |
| R10 | The brute-force ceiling is not widened | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_verify_attempt_ceiling_unchanged` | `1 passed` | P0 | |
| R11 | No credential material is emitted | `cd cli && bun test --test-name-pattern 'test_no_credential_material_in_emitted_output'` | `0 fail` | P0 | |
| R11b | The credential carries full entropy, so the unsalted digest is safe | `zig-out/bin/agentsfleetd-tests --test-filter test_credential_is_full_entropy_from_a_secure_source` | `1 passed` | P0 | |
| R12 | Live proof: a credential survives past the session window | `agentsfleet login && agentsfleet auth status --json \| jq -e '.expires_at == null'` | exit 0 — the stored credential carries no expiry | P0 | |
| R13 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted; the session-token persistence path is removed from within `credentials.ts` and `login.ts`.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the session-token persistence path | `grep -rnE "saveAccessToken\|SaveAccessTokenInput" cli/src \| grep -v _test` | only the credential-shaped writer remains |
| any refresh scaffolding | `grep -rniE "refresh(Token\|Session)\|refresh_token" cli/src` | 0 matches — a durable credential needs none (RULE NDC) |

## Out of Scope

- **Operating-system keyring storage.** The reference implementation prefers it and falls back to a `0600` file; we ship that fallback. Adopting the keyring costs a native dependency in a distributed product and a per-platform support surface. Moving later changes only where the credential rests, not what it is.
- **Named multi-deployment profiles.** §4 binds a credential to the one deployment that minted it, which is what stops the silent production fallback. Holding several deployments at once and switching between them is a larger surface and is not needed here.
- **Changing the handshake or the verification-code surface.** Both are correct, and the public key is already kept out of the browser URL, which the reference implementation does not manage. Untouched.
- **A global sign-out.** Logout ends this terminal's credential. Revoking every credential a user holds everywhere is a distinct product action with a distinct surface.
- **Preventing credential sharing.** A durable credential can be handed to someone else by the account holder, who approves in their browser and forwards the six-digit code — every control fires correctly, because the person consenting is the person sharing. That cannot be closed cryptographically, and this workstream does not try: Dimension 1.6 makes each use attributable so the question is answerable from a query, and nothing here limits concurrency, counts seats, or refuses a second machine. Whether that data warrants enforcement is a billing decision, taken with the data rather than ahead of it.
- **Retiring `core.api_keys`.** Tenant keys remain the right credential for service-to-service callers. This adds a second class; it does not replace the first.
- **Credential expiry policy.** A durable credential is durable. Rotation on a timer is a server-side product decision.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator logs in, walks away, comes back, and the next command just works — as themselves.
2. **Preserved user behaviour** — the login flow looks identical: same browser approval, same six-digit code, same success message. Only the credential behind it changes.
3. **Optimal-way check** — the direct route is to spend the short-lived token on a durable one. The unconstrained-optimal adds keyring storage and named profiles; both are named in Out of Scope and neither is needed to make login work.
4. **Rebuild-vs-iterate** — iterate on the handshake, which is correct and is the expensive part. The new table is genuinely new because no user-scoped credential exists to extend.
5. **What we build** — a user-scoped credential, a mint at the end of login, revocation of this machine's prior credential, and a deployment binding stored with it.
6. **What we do NOT build** — refresh machinery, keyring storage, multi-deployment profiles, a global sign-out, or any change to the handshake or the tenant key.
7. **Fit with existing features** — sits beside tenant API keys rather than replacing them. The features it must not destabilise are the device-flow handshake and the verify attempt ceiling, both regression-tested unchanged.
8. **Surface order** — Application Programming Interface first; the CLI consumes it. No user interface change and no new command: `login` and `logout` keep their names and arguments.
9. **Dashboard restraint** — no new dashboard surface in this workstream. Credentials are listable and revocable through the API; a management view is a later product decision, not a gap in this one.
10. **Confused-user next step** — every refusal names its cause and its fix: a failed exchange says to log in again, a deployment mismatch names both deployments, and a revoked credential points at login.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices split by what fails independently — the missing primitive (§1), the wrong thing being persisted (§2), the credentials that would pile up once one is minted per login (§3), and the deployment a durable credential would be replayed against indefinitely (§4). §3 and §4 cannot ship later: both describe failures that a *durable* credential creates and a sixty-second one masks.
- **Rejected — persist a tenant API key.** The first draft of this spec. Killed in adversarial review: `240:20` binds `tenant_id NOT NULL` with `created_by` as free text, so a terminal would authorise as the whole tenant, attribution would collapse to a string, and §3's revoke-on-relogin would let one member revoke a credential another member or a service depends on. The reference implementation's PAT is user-scoped; reading its storage shape without its ownership model is what produced the error.
- **Rejected — add refresh to the session token.** The shape the current code implies and the one the reference implementation deliberately avoids. It means a refresh credential, a renewal path, and a clock-skew surface, to arrive at what a durable credential gives with none of them. RULE NDC forbids building it speculatively.
- **Rejected — only lengthen the Clerk template.** Necessary relief and Indy is applying it (see Discovery), but insufficient: a session token remains the wrong kind of thing to leave on disk however long it lives, and the code comment's own stated target of roughly fifteen minutes still strands an operator who steps away.
- **Rejected — tell operators to run `api-key create` themselves.** The workaround in use during M136_001. It leaves the documented front door broken and makes first-run a two-command ritual with a copy-paste in the middle.
- **Patch-vs-refactor verdict:** a **refactor** for the credential model, because a user-scoped class genuinely does not exist and cannot be reached by extending the tenant one; a **patch** for the CLI, where one step of six changes.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: to be recorded as the work proceeds.

- **Why this spec exists (Aug 11, 2026).** Found during M136_001's live proof run. `agentsfleet login` reported success; `agentsfleet auth status` immediately reported `expired: yes`, `server_check: unauthorized (UZ-AUTH-003)`, with `saved_at 2026-08-11T08:11:00.206Z` and `expires_at 2026-08-11T08:11:52.000Z` — a fifty-two-second credential. A search of `cli/src` for `refresh(Token|Session)` and `refresh_token` returned no hits, confirming no renewal path was ever intended.

- **Root cause has two layers (Aug 11, 2026).** The carve-out comment in `ui/packages/app/app/cli-auth/[session_id]/page.tsx` states the design intent as roughly fifteen minutes and, in the same comment, records the `api` template as currently sixty seconds — the discrepancy is documented against itself in the source. **Layer one** is that template setting, which is operator configuration, not code. **Layer two** is that even the intended fifteen minutes is the wrong shape for a terminal: it is a session token, and the CLI carries no Clerk SDK and cannot refresh one, which the same comment states. This workstream fixes layer two; layer one is Indy's.
  > Indy (2026-08-11): "Yes fold both layers in M160_001 and rewrite it for Opation A now. I will fix the clerk dashboard setting." — context: the Clerk `api` template lifetime is applied by Indy out of band; this spec's code path is correct at any template value and is tested against a stubbed mint. The quote names this workstream by its original number; it was renumbered to M160_002 on Aug 12, 2026 to free the M160_001 identifier already held by the shipped acceptance-e2e workstream.

- **Decisions taken (Aug 11, 2026), from the use-case walk.**
  > Indy (2026-08-11): "1. Yes agreed to revoke CLI credentials only." — context: logout ends the terminal's credential and leaves browser sessions alone, because the browser holds a Clerk-refreshed credential class the CLI cannot use. Dimension 3.4.
  > Indy (2026-08-11): "2. Case 3 relogin auto revokes the prior credential for that machine only." — context: other machines keep working across a re-login. Dimensions 3.1 and 3.2.

- **Adversarial review, Aug 11, 2026 (Orly, Chief Technology Officer capacity).** The first draft of this spec was rejected before commit. Five findings: the tenant-scoped credential was the wrong ownership model and invalidated the CLI-only scope (F1, blocking — this rewrite); one precedence ladder conflated credential selection with target selection (F2 — now two ladders, Dimensions 4.3 and 4.4); the "no session token on disk" invariant claimed type-safety over a JSON file (F3 — now a prefix validated on load, Dimension 1.4, adopted from `access_token.go:16`); the mint endpoint's acceptance of session-token authorisation was assumed and unverified (F4 — now Dimension 2.3); and the only live rubric row raced a wall clock (F5 — now R12, asserting the stored credential carries no expiry).

- **Consult — device-flow security review, Aug 11, 2026.** The ECDH exchange was examined for whether encrypting to a CLI-supplied public key creates exposure. It does not: the public key is public by construction, and only the ephemeral private key — which never leaves the CLI process — decrypts. The phishing path (an attacker starts a login and sends the approval link to a signed-in victim) is closed by the verification code, which is generated in the victim's browser with `crypto.getRandomValues` under rejection sampling and never reaches the attacker; `MAX_VERIFY_ATTEMPTS` bounds guessing against the code space. Both properties are load-bearing for a *durable* credential and are pinned by `test_verify_attempt_ceiling_unchanged` (Invariant 8).

- **Sharing is a known, accepted consequence (Aug 11, 2026).** Raised during the use-case walk: today's sixty-second token is incidentally an anti-sharing control, and making the credential durable turns account sharing from a pointless nuisance into a one-time arrangement. Named here so it is on the record before it ships.
  > Indy (2026-08-11): "I dont want to spin too much time on such loop holes, since I have used the above mechanism to share my Claude subscription with my other team make lets say Bob. Sp add what is needed cheap to have auditable results and lets move on." — context: Dimension 1.6 adds last-use attribution on the update the authenticate path already performs. No enforcement, no concurrency limit, no seat counting.

- **Consult — attribution belongs at mint, not at use (Aug 11, 2026, PLAN).** The first amendment specified last-use columns written on the authenticate path. `240_api_keys.sql:12-15` had already refused exactly that, in writing: stamping every request turns the hottest indexed read in the system into a write. A coalesced-write compromise was then proposed and also rejected as unnecessary. The reference implementation settles it — `~/Projects/oss/cli` tracks no usage at all; its whole token record is identity and creation time. Attribution is therefore a mint-time fact, and the sharing question it exists to answer is already answered by mint-time data: a shared credential is minted on the sharer's own machine, so it appears as a second live row under one `user_id` with a different `machine_name`. `last_used_at` is provisioned NULL and left unwritten, mirroring `240`'s posture and its deferral to asynchronous stamping.
  > Indy (2026-08-11): "1. Is politically stupid, since you only update when you logout and login back - not on every call like list or so?" — context: per-request stamping removed; Dimension 1.6 now asserts the authenticate path writes nothing.

- **Consult — the user foreign key diverges from `240`, deliberately (Aug 11, 2026, PLAN).** `240:7-10` makes `created_by` a plain string specifically so an automation key outlives the admin who minted it; erasing a departed admin must not break nightly jobs. A personal credential inverts that requirement: if the human is erased, every terminal holding their credential must stop, or offboarding is theatre — and a credential shared with a colleague would outlive the account it belongs to. `250` therefore carries `user_id UUID NOT NULL REFERENCES core.users ON DELETE CASCADE`, and the schema file records the divergence so the next reader does not read it as an oversight.
  > Indy (2026-08-11): "2. Yes makes sense, go ahead to have cli_credentials with a FK to user_id" — context: the divergence is intended, not an inconsistency with the sibling table.

- **Scope decisions, Aug 12, 2026 (post-combine triage).** Three calls taken after the first full verification lane on the shared branch.
  > Indy (2026-08-12): "Keep it" — context: `9c491ceac` fixes a pre-existing `queue/` flake (the pool acquire-overshoot bound) unrelated to this workstream. Approved as an in-scope inclusion rather than a separate branch; the Files-Changed table records it as such.
  > Indy (2026-08-12): "just keep it simple you say zig build list-tests, check_zig_test_reachability.py, make audit - just follow a simpler route, not complicating and doing the same or little different with a new approach." — context: the never-compiled-module gap. **No new gate, no new script, no governance change.** Every new module this workstream adds carries a `refAllDecls` test block until it has real callers; the existing `check-test-reachability` already compiles every test root, so that block is what makes the module compile-checked. The repo-wide hole stays open by decision, not oversight.
  > Indy (2026-08-12): "Finish M136's §5 too" — context: M136_001 is no longer parked. Both workstreams complete on this branch and both specs move to `done/` before the Pull Request (PR) opens.

- **Consult — a green `zig build` is not a compiled module (Aug 12, 2026, EXECUTE).** `state/cli_credentials.zig` carried two genuine compile errors while the build reported success: `copyListed` fed `pg.Row.get`'s error union straight into `alloc.dupe`, and `listForUser` built its accumulator with Zig-0.15-era `std.ArrayList(T){}`. Neither was analysed, because Zig only compiles a function body once something references it, nothing in the tree called the module, and `tests.zig` referenced it with a bare `_ = @import(...)` — which evaluates a module without analysing its bodies. `check-test-reachability` did not catch it: that gate inspects `test` blocks, and the file had none. Found only by adding `refAllDecls`, which is why the convention above is now mandatory for this workstream's modules.

- **Metrics review** — to be recorded at CHORE(close).
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results to be recorded per `AGENTS.md` CHORE(close).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
