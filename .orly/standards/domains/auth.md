# Authentication, authorization and secrets

Rules for who may reach an endpoint, how secrets are stored, shown and compared, and how signed or single-use proofs are checked. A coding agent applies them before touching guards, tokens, credentials or auth flows. A reviewer judges a diff against them, and auth is a security boundary with no exceptions beyond those stated here.

## Routes and ownership

### AUTH-DOC — Token lifecycle, session shape, the auth layer chain and credential storage follow the repository's `docs/AUTH.md`.
- **Applies to:** the repository's auth and credential modules, token-minting handlers, credential-typed spec Dimensions
- **Checked by:** review
- **Bad:** a new token prefix or session field that `docs/AUTH.md` does not describe
- **Good:** a change that matches the flow `docs/AUTH.md` documents, or updates that document in the same diff
- **Why:** the product's auth document is the one description of every flow, and code that departs from it goes unreviewed.

### AUTH-GUARD — Each route row declares exactly one guard, and the capability a caller needs is the row's scopes.
- **Applies to:** route definition files
- **Checked by:** review
- **Bad:** a second authentication check inside a handler, or a webhook route mounted under `Guard::Bearer`
- **Good:** `Guard::Bearer` with `Scopes::rw(PROJECT_READ, PROJECT_WRITE)`, `Guard::RunnerBearer` for runner credentials, and a signature guard for signed deliveries
- **Why:** the router mounts the guard from the row, so a handler never runs for a caller who should not reach it.

The signature guards are `Guard::WebhookHmac`, `Guard::WebhookSignature` and `Guard::Svix`.
In `Scopes::ByMethod { get, otherwise }`, `otherwise` is the more privileged scope, so an unplanned method is refused too often, never too rarely.

### AUTH-OPEN — A `Guard::Open` route states in its module documentation what verifies its caller.
- **Applies to:** routes mounted with `Guard::Open`
- **Checked by:** review
- **Bad:** an open route whose module says nothing about who may call it
- **Good:** a webhook module naming the per-project signature it verifies over the body, under a buffer cap
- **Why:** an open route that verifies nothing and says nothing is a bug, and its proof can only be checked after the body is read.

Only the probes `/healthz` and `/readyz` verify nothing.
Device-flow session routes verify the code itself, connector callbacks a signed `state` parameter, and webhooks a signature.

### WAUTH — Every workspace-scoped operation authenticates the caller and proves workspace ownership before it reads or writes data.
- **Applies to:** workspace routes, their guards and their tests
- **Checked by:** review
- **Bad:** discarding the principal, or relying on an ownership helper that no served route mounts
- **Good:** the row's guard and the template's `{workspace_id}` mount both checks, and tests refuse unauthenticated and wrong-workspace callers on the served route
- **Why:** without the ownership check, any authenticated caller reads another workspace by changing the path.

Handlers neither authenticate nor call an ownership check by hand, because the mounted layers do both.

### AWO — A path naming a workspace and a resource inside it proves the resource belongs to that workspace, and answers `404` when it does not.
- **Applies to:** routes with `{workspace_id}` and a child identifier such as `{project_id}`
- **Checked by:** review
- **Bad:** reading the project by `id` alone once the workspace check passes, or answering `403` on a mismatch
- **Good:** the statement filters `WHERE id = $2 AND workspace_id = $1`, and no row answers `404` with `PROJECT_NOT_FOUND`
- **Why:** the workspace check proves only the caller-to-workspace edge, and a `403` would reveal which identifiers exist elsewhere.

An insecure direct object reference (IDOR) test requests the route with another workspace's project and asserts `404`.

### BIL — Billing and credential endpoints require their dedicated capability, never workspace access alone.
- **Applies to:** routes that expose billing figures or credential material
- **Checked by:** review
- **Bad:** a project billing summary mounted with `PROJECT_READ`, so every member who can read projects sees cent totals
- **Good:** `Scope::BillingRead` on billing routes, and `SECRET_READ` or `SECRET_WRITE` on credential routes
- **Why:** workspace ownership says nothing about role, and billing and credential data belong to privileged members only.

A test calls the route as a workspace member without that scope and asserts `403`.

### CTX — Data a sandboxed agent must not read across tenants sits behind a process boundary, never a shared filesystem.
- **Applies to:** storage for memory, caches and per-tenant files that sandboxed agents can reach
- **Checked by:** review
- **Bad:** a bind-mounted `/var/lib/agent-memory/{project_id}/` shared by sibling sandboxes and scoped only by path
- **Good:** Postgres behind the scoped `memory_runtime` role
- **Why:** an agent with any shell or file tool reads a sibling's directory and bypasses the scoping the API enforces.

### AUTH-CORS — Cross-Origin Resource Sharing (CORS) is declared per endpoint, and no endpoint allows every origin.
- **Applies to:** endpoints the dashboard calls from a browser, and edge and router configuration
- **Checked by:** review
- **Bad:** `Access-Control-Allow-Origin: *`
- **Good:** the dashboard origin declared in the spec for the endpoints the browser calls, with cross-origin requests refused at the edge elsewhere
- **Why:** a wildcard lets scripts on any origin call the API from a user's browser.

Public ingress is HTTPS only, enforced at the Cloudflare Tunnel. Adding browser access to an endpoint is a surface change.

## Secrets

### AUTH-SECRET — A secret-shaped response field is write-only or one-time-read, and its OpenAPI schema says which.
- **Applies to:** response fields whose names contain `token`, `secret`, `key`, `password` or `credential`
- **Checked by:** review
- **Bad:** `GET /v1/api-keys/{key_id}` returning the raw key
- **Good:** the raw key only in the create response, marked `x-secret-handling: one-time-read`
- **Why:** a secret readable on every `GET` spreads into caches, logs and screenshots.

A name that only identifies a secret, such as `key_name`, is not secret-shaped.

### AUTH-NOLOG — A resolved credential never reaches command output, a transcript, the event log or an agent's context.
- **Applies to:** code and tooling that loads secrets, and agent sessions that handle them
- **Checked by:** review
- **Bad:** pasting the output of `op read 'op://…'`, or a provider token written into a project event
- **Good:** `op read` at runtime into the process that needs it, and the secret substituted at the tool bridge inside the sandbox
- **Why:** a printed secret lands in transcripts and backups that outlive its rotation.

Log records follow LOG-SECRET in [logging.md](logging.md).

### AUTH-LOGID — A log call at `INFO` or above never carries a raw workspace, project or user identifier.
- **Applies to:** log calls in `**/*.rs` and `**/*.ts`
- **Checked by:** review
- **Bad:** `tracing::info!(workspace_id = %workspace_id, "project created")`
- **Good:** the identifier at `DEBUG`, or a same-line `// log-id-allowed: <reason>` when the value is safe, such as a hashed prefix
- **Why:** identifiers in routine logs let anyone with log access map tenants and their activity.

### AUTH-RANDOM — Identifiers and secrets come from the shared identifier module and the operating system's cryptographic source, never a custom generator.
- **Applies to:** code that mints identifiers, tokens, nonces or keys
- **Checked by:** review
- **Bad:** a seeded `SmallRng`, or `Math.random()`, minting a token
- **Good:** `app_core::id` for identifiers, and `app_crypto::entropy` for secret bytes
- **Why:** a predictable generator makes identifiers and secrets guessable.

## Comparisons and proofs

### CTM — Secrets, tokens and signatures are compared in constant time.
- **Applies to:** comparisons of tokens, passwords, nonces, digests and Hash-based Message Authentication Code (HMAC) tags
- **Checked by:** review
- **Bad:** `presented == expected` in Rust, `token === stored` in TypeScript
- **Good:** `expected.as_bytes().ct_eq(presented.as_bytes())` from `subtle`, or `timingSafeEqual` from `node:crypto`
- **Why:** a short-circuit comparison leaks, through its timing, how many leading bytes matched.

### CTC — A secret comparison takes the same path whatever the input lengths are.
- **Applies to:** constant-time helpers and their callers
- **Checked by:** review
- **Bad:** `if a.len() != b.len() { return false; }` before the comparison loop
- **Good:** compare fixed-length digests such as `HmacSha256Tag`, or loop over the shorter length and fold the length mismatch in after the loop
- **Why:** an early return on length tells the caller how long the expected secret is.

### CTE — A constant-time helper's comment claims only what the helper hides.
- **Applies to:** doc comments on comparison helpers
- **Checked by:** review
- **Bad:** "prevents timing leaks on secret value or length" above a loop over `min(a.len(), b.len())`
- **Good:** "`ct_eq` short-circuits on length, not content; the digest length is public"
- **Why:** a later caller with variable-length input would inherit a guarantee the code does not give.

### TWF — A freshness check rejects a timestamp too far in the future as well as one too far in the past.
- **Applies to:** webhook and signed-request verification
- **Checked by:** review
- **Bad:** `now - ts <= MAX_DRIFT` alone, which accepts any timestamp in the future
- **Good:** `ts <= now + MAX_DRIFT && now - ts <= MAX_DRIFT`, with the future bound checked first
- **Why:** an accepted future timestamp lets an attacker pre-sign a request and replay it after the window.

### OAE — An OAuth form body encodes every field value, the authorization code included.
- **Applies to:** OAuth token exchanges and any `application/x-www-form-urlencoded` body
- **Checked by:** review
- **Bad:** `format!("grant_type=authorization_code&code={code}&client_id={}", encode(client_id))`
- **Good:** `client.post(url).form(&Grant { … })`, which encodes every field through `serde_urlencoded`
- **Why:** a code containing `+`, `&` or `=` silently corrupts the body and draws a confusing error from the provider.

### GATDL — A single-use token in a key-value store is consumed by one script that compares and deletes atomically.
- **Applies to:** nonces, approval keys, cross-site request forgery (CSRF) tokens and device-flow codes held in a key-value store
- **Checked by:** review
- **Bad:** `GETDEL` followed by the comparison, or `GET` and `DEL` sent as two commands
- **Good:** one `EVAL` script that reads `KEYS[1]`, compares it with `ARGV[1]`, deletes the key only on a match, and returns 1 or 0
- **Why:** `GETDEL` lets a forged guess destroy the real token, and two commands let two requests both pass the read.

The script ships inside its crate through `include_str!`, as a `session/verify_consume.lua` file.
