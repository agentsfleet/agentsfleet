<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_002: CLI Browser Authorization Flow (verification code + ciphertext transport + login UX hardening)

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 002
**Date:** May 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — closes the session-id phishing-without-terminal attack class on the device-authorization flow; removes plaintext JSON Web Token (JWT) from server-side transport, storage, log, queue, and metrics surfaces; and consolidates every in-flight CLI-login concern (UX polish, error taxonomy, Effect-TS migration of the auth commands) into a single spec so the auth surface ships coherent instead of stitched. **Honest scope: this spec does NOT authenticate a specific Command Line Interface (CLI) installation, device, or autonomous agent.** Persistent machine identity is deferred to M75_xxx Agent Identity.
**Categories:** Application Programming Interface (API), CLI, User Interface (UI)
**Batch:** B2 — standalone. M74_001 is no longer a hard dependency (see *Relationship to M74_001* below); M71_001 P2 §1-§5 absorbed here (see *Relationship to M71_001 P2* below).
**Branch:** feat/m74-002-cli-browser-authorization-flow (already created, origin-tracked)
**Depends on:** None hard. M74_001 (Effect-TS substrate) is a code-cleanliness ordering preference — if M74_001 lands first, the new login Effect targets the substrate directly; if not, the login handler ships in the existing async/promise shape and M74_001 sweeps it later as part of its bulk migration. Either order works.
**Provenance:** human-written from `HANDOFF_SUPABASE_HARDENING_SPEC.md` (Captain ask, May 17, 2026). Rewritten May 17, 2026 after a threat-model challenge from Captain and cross-Large-Language-Model (LLM) review (Claude + ChatGPT) exposed that the prior framing conflated transport confidentiality with CLI authentication. Re-rewritten May 17, 2026 after a second ChatGPT pass surfaced concrete protocol fixes (hashed verification code, dedicated verify endpoint, formal state machine, replay protections, rate limits, pinned crypto primitives, removed Uniform Resource Locator (URL)-borne public-key transport, safer `token_name` defaults) AND after Captain consolidated the in-flight auth-login surface (M71_001 P2 §1-§5 + M74_001 §3) into a single spec.

**Canonical architecture:** `docs/AUTH.md` Flow 1 (CLI device flow). Updated as part of this spec.

---

## Relationship to other auth-touching specs

The CLI-login surface had three live, overlapping specs as of May 17, 2026. This spec absorbs the auth-relevant portions:

| Spec | Status | Disposition |
|---|---|---|
| **M74_001** Effect-TS migration | PENDING | Substrate-only after the trim. M74_002 owns the auth-command Effect rewrite. M74_001's §3 ("First-command migration paired with M74_002") is deleted from M74_001 because M74_002 owns it now. |
| **M71_001 P2** CLI Login Resilience and UX Polish | PENDING | §1-§5 (countdown, hydration warning, per-error AUTH_PRESET, exp-backoff polling, single-blip tolerance) **absorbed into this spec**. M71_001 P2 retained for §6-§11 (dashboard trigger panel + website OnboardingFlow + Hero CTA — non-auth UX work deferred from M68); rename/rescope happens separately. The dimensions M71_001 P2 had marked Out-of-Scope and explicitly deferred to "the cli-auth handshake hardening sibling spec" (D20 idempotency, D21 token-name flag, D24 token validation via `/me`, D25 argv-leak warning, D26 TTY-priority env resolution, D32 `logout --all`) are **in scope here**. |
| **M71_001 P1** CLI Command Effects (session_id + device_id + NDJSON traces) | DONE (PR #329 merged) | Out of scope. session_id + device_id already flow as base properties on every PostHog event; new analytics emits from this spec inherit that base shape. |

---

## Relationship to `zmb_t_` API keys (Flow 3) and agent keys

`zombiectl login` issues a Clerk-signed JWT — never a `zmb_t_` tenant API key, never a `zmb_` agent key. The three surfaces are intentionally separate per `docs/AUTH.md`:

| Surface | Credential | Lifecycle | Provisioned via | Used by |
|---|---|---|---|---|
| **Flow 1 — CLI device flow** (M74_002, this spec) | Clerk-signed JWT, audience `https://api.usezombie.com`, ~15 min Time-To-Live (TTL), CLI re-runs `login` on 401 | Browser-mediated, human-approved, transient | `zombiectl login` | Humans on workstations, local coding agents (Cursor / Claude Code) — interactive use |
| **Flow 2 — UI dashboard** | Same Clerk-signed JWT shape, minted browser-side via `getToken({template:"api"})` | Per-request mint | Clerk Frontend Application Programming Interface (FAPI) called by the dashboard | Browser dashboard only |
| **Flow 3 — tenant API key** | `zmb_t_<hex>` (70 chars), Static Hex Asymmetric (SHA)-256 hash persisted; raw value shown once | Long-lived; rotatable; revocable | `POST /v1/api-keys` from the dashboard | Service-to-service integrations (n8n, Zapier, custom scripts, external customer agents) |
| **Agent keys** | `zmb_<hex>` bound to a specific zombie | Long-lived per-zombie scope | `POST /v1/workspaces/{ws}/agent-keys` from the dashboard | Webhook-driven external integrations posting events to a single zombie |

The `bearer_or_api_key` middleware (`src/auth/middleware/bearer_or_api_key.zig`) routes by `Authorization: Bearer` prefix: `zmb_t_*` → Database (DB) hash lookup; anything else → Clerk Open Identity Connect (OIDC) Java Web Key Set (JWKS) verification. Both shapes carry through the Bearer header transparently — handlers never know which one was used.

**Implication for `zombiectl`:** an operator with `ZMB_TOKEN=zmb_t_…` already set in their environment does NOT need `zombiectl login` to talk to the API — the Bearer middleware accepts the `zmb_t_` key transparently. The login flow exists for the case where the operator wants a Clerk-mediated short-lived JWT instead (typically because they prefer browser-authenticated sessions or do not have tenant-admin access to provision a `zmb_t_` key).

**`zombiectl login` MUST detect this case** (see §5 D26 + new D26b below) and inform: *"`ZMB_TOKEN` is set in your environment — that takes precedence over `credentials.json` on interactive shells. `zombiectl login` will not affect `ZMB_TOKEN`; only your local `credentials.json` is replaced."* — then offer to continue or abort.

**`zombiectl login` does NOT mint `zmb_t_` keys.** The dashboard's "Create API Key" surface (which lands in a separate spec, not here) remains the only path to a `zmb_t_`. This separation is load-bearing: tenant-admin keys must require an explicit dashboard action with audit visibility; embedding that action inside the device-flow Approve click would be a privilege-escalation surface (a phishing attack against the device flow would also yield a long-lived admin key). If long-lived CLI credentials are needed for unattended workloads, **see M75_xxx Agent Identity below** — that is the right milestone, not this one.

The OpenAPI surface lands new auth endpoints (PATCH /approve, POST /verify, DELETE) under the existing `Authentication` tag. The `Tenant Api Keys` tag (`/v1/api-keys`) and `Agent Keys` tag (`/v1/workspaces/{ws}/agent-keys`) are untouched by this spec.

---

## Relationship to AUTH.md Flow 2 (end-user dashboard signup/login)

**Flow 2 — end-user dashboard signup/login at `app.usezombie.com` — is UNCHANGED by this spec.** Every property of the existing experience continues to work as before:

| Surface | Status |
|---|---|
| End-user signup at `app.usezombie.com/sign-up` (Clerk-hosted) | Unchanged |
| End-user signin at `app.usezombie.com/sign-in` (Clerk-hosted) | Unchanged |
| Clerk OAuth round-trip (GitHub / Google / etc.) | Unchanged |
| `__session` cookie on the dashboard origin (Token A per AUTH.md) | Unchanged |
| `clerkMiddleware()` in `proxy.ts` reading the cookie | Unchanged |
| Dashboard pages (`/zombies`, `/settings`, etc.) | Unchanged |
| Browser → Clerk FAPI minting Token B via `getToken({template:"api"})` | Unchanged |
| Dashboard → zombied API calls via `/backend/:path*` rewrite carrying Token B | Unchanged |
| Server-Sent Events (SSE) stream route handler injecting Bearer | Unchanged |

The ONLY new dashboard surface in M74_002 is the `/cli-auth/{session_id}` page (a single page reachable only when a CLI initiates a login flow and directs the user's browser to it). End users who never run `zombiectl login` never see this page; their experience is identical to before.

`docs/AUTH.md` Flow 2 / Flow 3 / Webhook auth sections are explicitly out of scope (see *Out of Scope*).

---

## Relationship to autonomous agents

**M74_002 is a human-mediated CLI/browser authorization flow. It is NOT the long-term autonomous agent identity protocol.** This separation is load-bearing — merging them produces an auth model that cannot be reasoned about.

| Surface | Trust model | Owning milestone |
|---|---|---|
| **Human-led agent only** (`zombiectl login` from a human operator's workstation, OR a local coding agent like Cursor / Claude Code running on the same workstation where the human can complete the browser approval and terminal verification) | Clerk identity → API-audience JWT via browser-mediated device flow with verification-code binding | **M74_002 (this spec)** |
| **Persistent autonomous agent** (CI runners, Kubernetes workloads, scheduled `zombiectl` on a headless box, hosted agent platforms calling our API, cron jobs, background workers, anything where there is no human present to approve in a browser AND type a verification code into a terminal) | Persistent agent keypair → signed challenges → scoped credentials → server-side agent inventory → revocation | **M75_xxx Agent Identity** (to be authored) |

**Hard rule (Fix #5 from ChatGPT review 3):**

> **M74_002 supports only human-led agents — a human MUST be present at flow time to complete browser approval AND terminal verification.**
>
> **Unattended use of `zombiectl login` is a spec violation, not a supported deployment shape.** Examples that are NOT supported and MUST NOT be retrofitted as "agent auth":
>
> - Continuous Integration (CI) runners (GitHub Actions, GitLab CI, CircleCI, Jenkins).
> - Cron jobs, systemd timers, scheduled background execution.
> - Kubernetes workloads, deployments, jobs.
> - Hosted agent platforms calling the API on behalf of a human.
> - Headless containers running `zombiectl login` against a pre-supplied verification code.
> - "Local agent" frames where the agent runs in the background without a human present at flow time.
>
> Use **M75_xxx Agent Identity** for any of the above. The presence of a human at flow time is load-bearing — remove it and the verification-code property collapses (an attacker who controls the agent's environment can complete the flow themselves without the operator's awareness).
>
> This is enforced by **discipline + documentation**, not by code. The login flow has no programmatic way to detect "is a real human present" — that's part of why misuse must be called out as a spec violation rather than left as a runtime error.

The verification code in M74_002 exists precisely because the human terminal ↔ browser binding is the trust property. Remove the human and the verification code collapses into theatre. For autonomous agents:

- Do **not** issue Clerk JWTs to long-running agents as their durable credential. Clerk fundamentally models human identity; using it as a machine identity primitive fights rotation, revocation, workload scoping, ephemeral infrastructure, CI leakage, and delegated execution.
- The correct shape is: human runs `zombiectl login` (M74_002), then runs `zombiectl agent enroll` (M75_xxx) to mint a scoped agent credential. The human approval is the bootstrap; the agent runs independently afterward via signed challenges. Reference patterns: GitHub Apps, Kubernetes service accounts, SPIFFE/SPIRE identities, Cloudflare Access service tokens, GitHub Actions Open Identity Connect (OIDC) federation.

Any future debate over "should M74_002 also support autonomous agents?" terminates here: **no**. M74_002 stays scoped to human-mediated authorization. Agent identity is M75_xxx.

---

## Threat Model

**This is the load-bearing section. Every implementation decision below must trace to an attacker capability listed here. If a decision does not trace to a listed threat, it is out of scope — flag it and re-read this section.**

### What this flow protects against

| # | Threat | Closed by |
|---|---|---|
| 1 | **Session-row plaintext disclosure.** Database (DB) dumps, application logs, queue inspection, metrics blobs. Today the server-side session row carries `{ status, token: "<jwt>" }`. | Elliptic Curve Diffie-Hellman (ECDH) ciphertext transport. After this spec the row carries `{ status, cli_public_key, dashboard_public_key, ciphertext, nonce, verification_code_hmac, ... }`. The JWT is never persisted server-side in plaintext. |
| 2 | **Passive network observation of the JWT.** Transport Layer Security (TLS)-inspecting corporate proxies, captured Hypertext Transfer Protocol Secure (HTTPS) payload logs, intermediaries that terminate and re-issue TLS. These see the `PATCH /approve` body. Today it's a JWT in cleartext. | ECDH ciphertext transport. After this spec, intermediaries see ciphertext only. |
| 3 | **Session-id phishing without terminal access.** Attacker who has only the `session_id` (URL sniff, browser history sync, shoulder-surf). Today, polling `GET /v1/auth/sessions/{id}` after Approve hands them a plaintext JWT. | The `verification_code` requirement plus the move of ciphertext release from `GET` to `POST /verify`. An attacker without terminal access to the user's machine cannot present the matching code and cannot trigger the ciphertext-release endpoint. |
| 4 | **Verification-code disclosure via passive server-side compromise.** Attacker who reads the session row from the DB / logs / queue / memory snapshot extracts the plaintext code and replays. **Or** an attacker who reads ONLY the stored hash and attempts offline brute force across the 1,000,000-entry 6-digit space. | `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || verification_code)` storage. The server never persists the plaintext code; it stores only the keyed HMAC. **Offline brute force needs the pepper too** — a Redis dump alone is insufficient (the pepper lives in process memory after Vault load, never on disk under normal operation). Replay needs the plaintext code, which exists only in the dashboard JS process (display) and the CLI process (type). |
| 5 | **Verification-code brute force.** Attacker with `session_id` tries the 1,000,000 6-digit code space. | Rate limit: ≤ 5 verify attempts per `session_id`, then transition to `aborted`. |
| 6 | **Ciphertext replay.** Attacker who successfully captured a single `POST /verify` response retries the same `session_id`. | Atomic transition `verified → consumed` on the same database write that returns the ciphertext. A consumed session refuses every subsequent verify call. |

### What this flow does NOT protect against

**Each of these is real, and the spec is brutally explicit that we have not closed them.** Future readers debating "should M74_002 also do X?" against any of these should redirect to M75_xxx or a later milestone.

| # | Threat | Why this spec does not close it |
|---|---|---|
| 1 | **Compromised browser session.** Cross-Site Scripting (XSS) on the dashboard, malicious browser extension, session-cookie theft, injected analytics script, compromised Node Package Manager (NPM) dependency in the dashboard bundle. | The plaintext JWT lives momentarily in the dashboard JavaScript (JS) process before encryption. Anything with execution access to that process sees the JWT. ECDH does not help. AUTH.md must call this out explicitly per the AUTH.md additions in §7. |
| 2 | **Malware on the CLI host.** Compromised `zombiectl` machine, malicious user-space process, memory scraping. | `cli_priv` lives in CLI process memory during the flow; the decrypted JWT lives in `credentials.json` after. Local malware reads either. |
| 3 | **Attacker with simultaneous browser + terminal access.** User running attacker-supplied software ("paste this curl command into your terminal"). | The verification code cannot defend against the user actively typing the code into the attacker's tool. |
| 4 | **Device impersonation / fake `zombiectl` binaries.** Any actor can generate a valid ECDH keypair using publicly known mathematics; any actor can ship a binary called `zombiectl`. | Possessing a valid public key proves nothing about identity. This spec does not authenticate "this is the real `zombiectl` binary on the real user's laptop." Closed by M75_xxx (persistent device identity) or a separate distribution-trust milestone (binary signing). |
| 5 | **Autonomous-agent authentication.** A CI runner, Kubernetes workload, or unattended `zombiectl` invocation cannot complete the human-mediated verification step. | Out of model. See *Relationship to autonomous agents*. M75_xxx owns this. |
| 6 | **Active API or proxy response modification (key-substitution MITM).** An attacker who can modify API responses (compromised API process, active malicious reverse proxy, compromised TLS-terminating intermediary acting maliciously rather than passively) can swap `cli_public_key` in the GET /sessions response, intercept the encrypted PATCH /approve body, decrypt with their own substituted key, re-encrypt to the real CLI's key, and forward — invisibly stealing the JWT. | **Out of scope for v2.0; deferred to v2.1.** ECDH closes *passive* server-side disclosure (Redis dump, logs, queues, captured-but-not-modified TLS) but a public-key value passed through the API and then trusted by the dashboard is unauthenticated Diffie-Hellman — the textbook MITM. Future closure (URL fragment binding + HKDF transcript binding) is detailed in *Out of Scope → Future improvements*. **The spec must not claim closure of this attack class.** Every "API never holds decrypt capability" assertion below is softened to "honest API never holds decrypt capability." |

### Security properties by layer

This table is the contract. Every line of code in this spec must trace to one of these properties. Any claim of "auth hardening" in the abstract should be re-read against this table.

| Layer | Property | Out of scope |
|---|---|---|
| TLS | Server authenticity (cert chain to a trusted Certificate Authority (CA)) + transport encryption (network observers see ciphertext, not plaintext HTTP) | Endpoint compromise on either side |
| Clerk session | Browser-user authentication (the human at the keyboard owns the Clerk identity) | Hijacked browser session, shared workstation |
| **Verification code** | **Browser ↔ terminal authorization binding** — proves the human approving in the browser is the same human typing into the local terminal | User pasting attacker-supplied commands |
| `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id \|\| code)` storage | Disclosure-resistance of the verification code against passive server-side compromise. **Pepper-based**: a Redis dump alone cannot recover the code via offline brute force; the attacker would also need the pepper, which lives in zombied process memory only (loaded from Vault at boot, never written to disk). | Compromise of the dashboard JS process where the code is displayed; compromise of the CLI process where it is typed; compromise of the API process memory where the pepper lives |
| ECDH P-256 | Ciphertext-only session transport — no intermediate server, log, or DB row sees the JWT in plaintext | Compromise of the dashboard or CLI endpoints |
| AES-256 / Galois Counter Mode (GCM) | Tamper detection — any ciphertext modification produces a hard `DecryptError`, not silent corruption | — |
| Atomic `verified → consumed` transition | Single-read ciphertext — captured response cannot be replayed against the same session | Replay using a fresh session (closed by `verification_code` + rate limits) |
| Verify-attempt rate limit (≤5 / session) | Brute-force resistance on the 6-digit code | Distributed brute force across many sessions (closed by session-creation rate limit per Internet Protocol (IP) + per Clerk user) |
| `token_name` | Auditability only — operator can list active sessions by label; not a security control | Trust signal of any kind |

---

## Non-goals

This spec does **NOT**:

- Authenticate a specific device, `zombiectl` installation, or autonomous agent. See *Relationship to autonomous agents* and M75_xxx.
- Establish hardware trust (Trusted Platform Module (TPM) / Secure Enclave / Web Authentication (WebAuthn) / passkey).
- Prevent fake CLI implementations (no binary signing, no attestation).
- Prevent malware on the local machine from stealing the JWT post-login.
- Persist long-term cryptographic identity. ECDH keys in this spec are ephemeral and single-flow.
- Replace TLS — TLS is still required for transport (defense in depth + server authenticity).
- Replace Clerk authentication — the dashboard still relies on Clerk for human authentication.
- Make session storage durable. Sessions stay in-memory in `src/auth/sessions.zig` for v2.0; persistence is a separate decision (see Out of Scope).
- Add a dashboard "sessions" page with revoke buttons. That is a UX follow-up; this spec scopes mint-time only.

---

## Why each component exists

Brutally separated to prevent the conflation that motivated this rewrite.

### Verification code — primary authorization binding

**The verification code is the security property that closes the phishing-without-terminal attack.** Without it, possession of `session_id` is sufficient to complete the login (the attacker creates the session, phishes Approve, polls the result). With it, the code-typing step proves the human who clicked Approve in the browser is the same human controlling the terminal that initiated the session.

The verification code does NOT depend on ECDH. The code mechanism works equally well in a plaintext-JWT design (GitHub's `gh auth login` uses this pattern without any transport encryption beyond TLS).

### `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || code)` storage — code disclosure resistance

The server never persists the plaintext verification code. The dashboard sends the plaintext code to the API via the PATCH /approve request body (over TLS to a Clerk-authenticated endpoint); the server computes `verification_code_hmac = HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || verification_code)` and stores only the HMAC. The CLI later POSTs the plaintext code on /verify; the server re-computes the HMAC with the same pepper + session_id and compares constant-time. **The dashboard does NOT compute the HMAC** — only the API process has the pepper. This is a small departure from the earlier "dashboard hashes locally" framing in revision 2; ChatGPT's third review correctly pointed out that a salted-SHA256 (no key) is brute-forceable offline from a Redis dump. Moving the keyed-HMAC computation server-side and treating the pepper as a Vault-loaded secret closes that attack class.

**The real rule is:** the API never *persists* the plaintext code — it sees it momentarily on the PATCH body to compute the HMAC, then discards. This is the standard one-time-password (OTP) verification shape (the same shape used for password resets, magic-link tokens, etc. — the verifier sees the token to verify it, then never stores it).

**Pepper provisioning:**

- `AUTH_SESSION_CODE_PEPPER` env var; required at zombied boot (fail-fast like `REDIS_URL` per Fix 2).
- Source: Vault. Paths per the existing pattern (`feedback_vault_tooling`):
  - `op://ops/ZMB_CD_PROD/AUTH_SESSION_CODE_PEPPER` (production)
  - `op://ops/ZMB_CD_DEV/AUTH_SESSION_CODE_PEPPER` (development)
  - `op://ops/ZMB_LOCAL_DEV/AUTH_SESSION_CODE_PEPPER` (local dev)
- Loaded once at process start via existing `src/state/vault.zig`; held in zombied process memory for the lifetime of the process. Never written to disk, never logged.
- Length: 32 bytes (256 bits) of CSPRNG output, base64url-encoded. Documented in deploy README.
- Rotation policy: pepper rotation invalidates every in-flight session (their HMACs no longer match). Operationally cheap because sessions are 5-min-TTL — drain old sessions on rotation by waiting 5+ minutes between provisioning the new pepper and cutting over. For v2.0, rotation is manual + Captain-approved; future spec lands automated rotation if needed.

This protects against the attack class: passive server-side compromise (DB dump, log scrape, memory snapshot, queue inspection) that yields the HMAC + session_id but not the pepper.

### Dedicated `POST /verify` endpoint — clean separation of polling and ciphertext release

`GET /v1/auth/sessions/{id}` stays semantically read-only and returns status only. Ciphertext release lives on `POST /v1/auth/sessions/{id}/verify`. Three reasons:

1. **Replay semantics are clean.** The POST endpoint can atomically transition `verified → consumed` on the same database write that returns the ciphertext. A consumed session refuses every subsequent verify call. Mixing this into a GET that's also the poll endpoint produces ambiguous semantics under concurrent polls.
2. **Rate limiting is clean.** Verify attempts have a per-session cap (≤5); polling has a backoff cap. Mixing them forces one limit to dominate.
3. **Audit logs are clean.** A verify attempt is a security-relevant event; a poll is not. Splitting endpoints lets the audit pipeline filter on the security-relevant calls cheaply.

### ECDH transport encryption — confidentiality only

ECDH in this flow does NOT authenticate the CLI. Anyone can generate a valid ECDH keypair using publicly known mathematics; possession of a valid public key proves nothing about identity.

ECDH exists solely to ensure intermediate transport and storage surfaces never contain a plaintext JWT. Surfaces protected:

- The API server's session table / store
- Application logs
- Queue inspections (if the session row ever transits a queue)
- Metrics pipelines
- Database dumps
- Captured HTTPS payloads from a TLS-inspecting proxy

The JWT exists in plaintext **only**:

- Inside the dashboard JS process, momentarily, before encryption
- Inside the `zombiectl` process, momentarily, after decryption

All other surfaces receive ciphertext. **A compromised dashboard or a compromised CLI defeats this. ECDH does not help against either.** Listed in Non-goals and explicitly repeated in the AUTH.md update.

### `token_name` — auditability only, with safer defaults

A human-readable device label persisted with each credential. Operator-facing: `zombiectl auth status` shows which session is active. Optionally surfaced on a future dashboard sessions surface.

**Default value is the platform family, not the hostname.** `macos-cli`, `linux-cli`, `windows-cli`, or `freebsd-cli` based on `process.platform`. Hostname-username defaults (e.g. `kishore-macbook-pro` or `prod-admin-root`) leak workstation naming conventions and operational metadata; the safer default is opaque-by-default. Operators override via `--token-name <label>`.

Not a security control — an attacker can set `token_name` to any string; it is an audit hook, not a trust signal.

### Already-authenticated detection on dashboard and CLI

A UX guardrail, not a security boundary. Surfaces "this will replace your previous CLI session on **{previous_token_name}**" so the user does not silently invalidate their own active session. Defense-in-depth against a user accidentally completing a phishing flow against their own active CLI.

---

## Concrete attacker walkthroughs

Explicit so future agents debating "do we need X?" read these first.

### Attack A — Session-id phishing without terminal access

```
Attacker:
1. Generates atk_priv, atk_pub via crypto.subtle.generateKey({ namedCurve: "P-256" }).
2. Calls POST /v1/auth/sessions with { public_key: atk_pub, token_name: "user-laptop" }.
   Receives atk_session_id.
3. Phishes the user with the verify URL: https://app.usezombie.com/cli-auth/{atk_session_id}.
   (No public_key in the URL — that's stored server-side; see Protocol §1.)
4. User clicks Approve in their browser (Clerk session already valid).
5. Dashboard GETs /v1/auth/sessions/{atk_session_id} → receives cli_public_key=atk_pub and token_name.
6. Dashboard generates a 6-digit verification_code, encrypts the JWT with atk_pub via ECDH,
   PATCHes /approve with { dash_pub, ciphertext, nonce, verification_code_hmac }.
7. Dashboard displays the verification_code on the user's screen.

Result:
- User has no zombiectl session matching atk_session_id.
- If the user has their own zombiectl running, it polls a DIFFERENT session_id (their own).
- The displayed code is for the attacker's session_id; the user has no way to feed it into
  the attacker's CLI (which they don't have terminal access to).
- The attacker's session sits in verification_pending until expiry / 5-attempt cap, then
  transitions to aborted. Ciphertext never released.

Outcome: BLOCKED by verification_code. NOT blocked by ECDH.
```

### Attack B — Database / log / memory compromise (passive server-side)

```
Attacker compromises the API server's session storage (DB dump, log capture, queue
inspection, memory snapshot).

Before M74_002:
- Session row: { status: "complete", token: "<jwt>" }
- Attacker extracts the JWT, uses it directly until expiry (~15 min Clerk Time-To-Live (TTL)).

After M74_002:
- Session row: { status: "verification_pending", cli_public_key, dashboard_public_key,
                ciphertext, nonce, verification_code_hmac, verification_attempts, ... }
- Attacker has ciphertext but no cli_priv (lives only in the CLI process memory).
- Attacker has verification_code_hmac but not the plaintext code AND not the pepper.
  Offline brute force across the 1M-entry 6-digit space requires recomputing
  HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || candidate) for each candidate.
  Without the pepper (which lives only in zombied process memory, Vault-loaded at
  boot), the attacker cannot compute any candidate HMAC. Closed by Fix #1 (HMAC pepper).
- The verification code is moot for an attacker who already controls the server — but
  they still cannot recover the JWT without cli_priv from the CLI machine.

Outcome: BLOCKED by ECDH. JWT confidentiality preserved unless the attacker
ALSO compromises cli_priv on the user's machine.
```

### Attack C — Attacker runs their own CLI with their own keypair

```
Attacker controls a machine. Generates valid atk_priv, atk_pub. Wants the JWT.

Without verification code:
1. Attacker creates session_id, gets a verify URL.
2. Phishes the user with the URL.
3. User approves.
4. Attacker POSTs /verify with anything → server returns ciphertext.
5. Attacker decrypts with atk_priv → has JWT.

  ECDH did NOT prevent this. The attacker is a valid participant in the ECDH protocol.

With verification code:
1. Same setup through step 3.
2. Dashboard shows verification_code on the USER's screen.
3. Attacker has no way to see the user's dashboard screen.
4. Attacker POSTs /verify with a guess — fails. 4 more guesses, then session aborts.
5. Session expires without ciphertext release.

Outcome: BLOCKED by verification_code + verify-attempt rate limit.
```

### Attack D — Passive TLS-inspecting proxy

```
Attacker has visibility into TLS-terminated traffic at a corporate proxy (common at
enterprises with TLS-inspection / Secure Sockets Layer (SSL)-decryption appliances).

Before M74_002:
- PATCH body: { status: "complete", token: "<jwt>" }
- Proxy logs the JWT in cleartext.

After M74_002:
- PATCH /approve body: { dashboard_public_key, ciphertext, nonce, verification_code } (plaintext code over TLS to Clerk-authenticated endpoint; API computes HMAC server-side, stores only the HMAC, discards the plaintext)
- POST /verify body: { verification_code }
- POST /verify response: { dashboard_public_key, ciphertext, nonce }
- Proxy logs ciphertext + hash + code; without cli_priv it cannot decrypt; the code is
  one-shot (consumed atomically with ciphertext release) so post-capture replay fails.

Outcome: BLOCKED by ECDH + atomic consume.
```

### Attack E — Verification-code brute force

```
Attacker has session_id (from any source). Wants to guess the 6-digit code.

1. POST /verify { verification_code: "000000" } → 400 InvalidCode (1/5)
2. POST /verify { verification_code: "000001" } → 400 InvalidCode (2/5)
3. POST /verify { verification_code: "000002" } → 400 InvalidCode (3/5)
4. POST /verify { verification_code: "000003" } → 400 InvalidCode (4/5)
5. POST /verify { verification_code: "000004" } → 400 InvalidCode (5/5) + session → aborted

Subsequent verify calls return 410 SessionAborted. Attacker tried 5 of 1,000,000 codes
(0.0005% of the space).

Outcome: BLOCKED by per-session verify-attempt rate limit.
```

### Attack G — Active API or proxy key substitution (NOT closed by v2.0)

```
Attacker has active control of an API response path (compromised zombied process,
malicious reverse proxy upstream of zombied, TLS-terminating intermediary acting
maliciously, container-runtime escape on the API host).

1. CLI POSTs /v1/auth/sessions with { cli_public_key, token_name }. Session created
   honestly; Redis row stores the real cli_public_key.
2. User opens the verify URL.
3. Dashboard GETs /v1/auth/sessions/{id}. The active attacker intercepts the response
   on the way back to the browser and substitutes attacker's atk_public_key for the
   real cli_public_key.
4. Dashboard sees atk_public_key in the response, generates dash_priv, derives shared
   = dash_priv × atk_public_key, encrypts JWT to atk_public_key, PATCHes /approve.
5. Attacker reads the PATCH body server-side (or proxy-side). Decrypts ciphertext with
   atk_priv → has the JWT.
6. Attacker re-encrypts the same JWT to the real cli_public_key (which they have from
   step 1) and writes that ciphertext back to the session row.
7. CLI POSTs /verify with the correct verification code (the human typed it; nothing
   in the protocol detects that the dashboard saw a different public key than the CLI
   sent). CLI receives the re-encrypted ciphertext, decrypts with cli_priv → has JWT.
8. Login succeeds for the user. Attacker silently also has the JWT.

Status: NOT BLOCKED by v2.0. This is unauthenticated Diffie-Hellman; passing the
cli_public_key through the API and trusting the API to return it honestly is the
textbook setup for MITM.

v2.1 closure: URL fragment binding — the CLI puts cli_public_key into the URL fragment
(fragments aren't sent to the server; they exist only in the browser). The dashboard
compares the fragment value to the API-returned value; mismatch aborts. HKDF info
binds both pubkeys + session_id into the AES key derivation so any substitution makes
decryption fail. Detail in *Out of Scope → Future improvements*.
```

### Attack F — Distributed brute force across many sessions

```
Attacker scripts: create 200,000 sessions, try 5 codes each = 1,000,000 attempts.

Closed by:
1. Session-creation rate limit per IP (default: 10 / IP / minute, see Rate Limits §).
2. Session-creation rate limit per Clerk user once the user logs in to the dashboard to
   approve (default: 20 / user / hour).
3. The 6-digit code is per-session — guessing across sessions is not amortized; each
   session_id has its own code.

Outcome: BLOCKED at session-creation budget. Attacker cannot fan out fast enough.
```

---

## Step-by-step flow — who initiates, who responds

This diagram shows the **temporal sequence** of the flow: `zombiectl login` is the initiator; UI, API, and Clerk respond. Read top to bottom.

```mermaid
sequenceDiagram
    actor User
    participant CLI as zombiectl
    participant UI as UI / Dashboard<br/>(app.usezombie.com)
    participant API as API server<br/>(api.usezombie.com)
    participant Clerk

    User->>CLI: zombiectl login [--token-name LABEL]
    Note over CLI: generate (cli_priv, cli_pub) via crypto.subtle<br/>default token_name = "macos-cli" | "linux-cli" | "windows-cli"
    CLI->>API: POST /v1/auth/sessions<br/>{ public_key: cli_pub, token_name }
    API-->>CLI: { session_id }
    CLI-->>User: open URL<br/>app.usezombie.com/cli-auth/{session_id}

    Note over CLI: poll loop with exponential backoff<br/>(1s → 5s cap, ±20% jitter)<br/>+ live countdown "Session expires in MM:SS"

    User->>UI: open URL in browser
    Note over UI: Clerk session validates (cookie)
    UI->>API: GET /v1/auth/sessions/{id}
    API-->>UI: { status: "pending", cli_public_key, token_name }
    UI-->>User: "Approve CLI login for {token_name}?"
    User->>UI: click Approve

    UI->>Clerk: POST /tokens (template: api)<br/>+ Clerk session cookie
    Clerk-->>UI: { user-jwt }

    Note over UI: generate (dash_priv, dash_pub)<br/>shared = dash_priv × cli_pub<br/>key = HKDF-SHA256(shared, info="m74-002-v1")<br/>ciphertext = AES-256-GCM(jwt, key, nonce)<br/>verification_code = random 6-digit (Cryptographically Secure Pseudo-Random Number Generator (CSPRNG))
    UI->>API: PATCH /v1/auth/sessions/{id}/approve<br/>{ dashboard_public_key, ciphertext, nonce, verification_code }<br/>(plaintext code over TLS, Clerk-authenticated)<br/>Authorization: Bearer <user-jwt>
    API-->>UI: 200
    Note over API: state transition pending → verification_pending
    UI-->>User: "Type {verification_code} into your CLI"

    loop CLI poll (exp backoff + countdown)
        CLI->>API: GET /v1/auth/sessions/{id}
        API-->>CLI: { status: "verification_pending" }
    end

    Note over CLI: prompt "Verification code:" (suppressed in --no-input)
    User->>CLI: types verification_code
    CLI->>API: POST /v1/auth/sessions/{id}/verify<br/>{ verification_code }
    Note over API: hmac = HMAC-SHA256(PEPPER, session_id || code)<br/>constant-time compare against stored verification_code_hmac<br/>match → atomic state transition<br/>verification_pending → verified → consumed<br/>(returned in same Lua-EVAL write)
    API-->>CLI: 200 { dashboard_public_key, ciphertext, nonce }
    Note over CLI: shared = cli_priv × dashboard_public_key<br/>key = HKDF-SHA256(shared, info="m74-002-v1")<br/>jwt = AES-256-GCM-decrypt(ciphertext, key, nonce)
    CLI->>CLI: write { token, token_name } to credentials.json (mode 0o600)
    CLI->>API: GET /v1/me (token-validation ping; D24)
    API-->>CLI: 200 (or 401 → DecryptError surfaced)
    CLI-->>User: "logged in as {token_name}"
```

Two facts the diagram pins:

1. **The CLI is the initiator.** Every interaction with the UI, API, or Clerk is downstream of `zombiectl login`. The user typing the verification code closes the loop back to the CLI.
2. **Clerk is the JWT mint.** The UI talks to Clerk only at one step (POST /tokens). The API server never talks to Clerk in this flow (Clerk's involvement is Java Web Key Set (JWKS)-only — the API verifies the JWT's signature when the CLI later uses it on normal API calls).

---

## JWT confidentiality path — where the secret lives in plaintext

This is a **different view** from the sequence above. The sequence shows *who calls whom* (temporal). This diagram shows *where the JWT exists in plaintext vs ciphertext* (data lifecycle of the secret). They point in opposite directions because the JWT is born in the dashboard process (right after Clerk mints it) and ends in the CLI process (after decryption) — but the user-initiated flow direction is the opposite (CLI starts, ends with CLI receiving the JWT).

```
┌──────────────────────┐
│  UI / Dashboard      │  ← plaintext JWT lives here momentarily
│  process             │     (vulnerable to XSS / extensions / page compromise —
│  (browser tab)       │      out of scope for this spec; see Non-goals)
│                      │
│  Clerk mint → JWT    │
│  AES-256-GCM encrypt │
│  with HKDF-derived   │
│  shared secret       │
└──────────┬───────────┘
           │ PATCH /v1/auth/sessions/{id}/approve
           │ { dashboard_public_key, ciphertext, nonce, verification_code }
           │   (plaintext code over TLS; API computes HMAC server-side,
           │    persists verification_code_hmac only, discards plaintext)
           ▼
┌──────────────────────┐
│  API server          │  ← ciphertext + hashed code only
│  session row         │     (no decrypt capability; the API never holds a
│  { ciphertext,       │      key that can recover the JWT, nor the plaintext
│    nonce,            │      verification code)
│    code_hash,        │
│    cli_public_key,   │
│    dashboard_pubkey} │
└──────────┬───────────┘
           │ POST /v1/auth/sessions/{id}/verify { verification_code }
           │ (only after CLI presents the matching code;
           │  atomic verified → consumed on the same write)
           ▼
┌──────────────────────┐
│  zombiectl process   │  ← plaintext JWT reconstituted here
│  cli_priv in memory  │     (vulnerable to local malware reading the process —
│  AES-256-GCM decrypt │      out of scope for this spec; see Non-goals)
│  → JWT to            │
│    credentials.json  │
└──────────────────────┘
```

**Explicit (honest-server assumption):** An honest API server that stores and returns the original public keys never possesses decryption capability. Compromise of an honest API server's database, logs, queues, or metrics pipeline cannot recover the JWT. The verification code itself never lives plaintext server-side — only `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || code)`. **An *active malicious* API server (or a TLS-terminating proxy acting maliciously rather than passively) can substitute `cli_public_key` in the GET /sessions response and execute a textbook unauthenticated-Diffie-Hellman MITM** — see "What this flow does NOT protect against" row 6. v2.1 closes that gap via URL-fragment binding; v2.0 explicitly does not.

**Equally explicit:** The UI / dashboard process and the `zombiectl` process each hold the plaintext JWT momentarily. Compromise of either endpoint compromises the JWT regardless of this spec.

**Why these two diagrams point in opposite directions:** The CLI initiates the flow, but the JWT is *produced* at the dashboard (after Clerk mints it) and *consumed* at the CLI. The data lifecycle of the secret is dashboard → API → CLI. The temporal sequence of the flow is CLI → UI → API → Clerk → API → CLI. Both views are accurate; conflating them was the source of the earlier confusion.

---

## Session lifecycle (state machine)

| State | Meaning | Enters from | Exits to |
|---|---|---|---|
| `pending` | CLI created session; browser not yet approved | (initial via POST /sessions) | `verification_pending` (dashboard PATCH /approve) · `expired` (TTL) · `aborted` (explicit cancel / replacement) |
| `verification_pending` | Browser approved; ciphertext + hashed code staged; awaiting CLI to present the code | `pending` | `consumed` (CLI POST /verify success — `verified` and `consumed` collapse into a single atomic transition) · `expired` (TTL) · `aborted` (5 failed verify attempts / explicit cancel) |
| `consumed` | Ciphertext returned to CLI exactly once; terminal success state | `verification_pending` | (terminal) |
| `expired` | Session TTL elapsed (default 5 minutes from creation) | `pending` · `verification_pending` | (terminal) |
| `aborted` | Explicit cancellation (`DELETE /v1/auth/sessions/{id}` from the same CLI) OR 5 failed verify attempts OR replaced by a fresh session under the same Clerk user | `pending` · `verification_pending` | (terminal) |

**Hard invariants:**

- No backward transitions. A state can only progress forward; `consumed` cannot revert to `verification_pending`, etc.
- `verified` is not a distinct stored state. The `verification_pending → consumed` transition happens atomically in the same database write that returns the ciphertext on `POST /verify`. The "verified" name appears only in audit logs (a successful verify attempt logs `auth.session.verified`) — the row itself jumps straight to `consumed`.
- Terminal states (`consumed`, `expired`, `aborted`) refuse every subsequent state-mutating call (HTTP 410 Gone).

---

## Cryptographic primitives (pinned)

Security specs should not leave crypto implicit.

| Primitive | Value | Why pinned |
|---|---|---|
| Curve | P-256 (NIST) | `crypto.subtle` supports it natively in both Node.js ≥20 and modern browsers (Q4 decision). |
| Key derivation | HKDF-SHA-256 | Output 32 bytes. `info` parameter: ASCII literal `"m74-002-v1"` — versioned so a future protocol change can rev the info string without colliding. `salt` parameter: empty (`Uint8Array(0)`) — the ECDH shared secret is already high-entropy. |
| Authenticated encryption | AES-256-GCM | 256-bit key from HKDF, 96-bit random nonce per encryption, 128-bit authentication tag. |
| Nonce | 96-bit, generated via `crypto.getRandomValues(new Uint8Array(12))` | Per-encryption fresh; never reused under the same key. Single-flow keys make collision astronomically unlikely; documented for explicitness. |
| Verification code | 6 random digits via CSPRNG (`crypto.getRandomValues`) | 1,000,000 entries; 5-min TTL; ≤5 verify attempts per session. Brute force closed by attempt cap, not code entropy. |
| Verification-code storage | `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id \|\| code)` (Fix #1 from ChatGPT review 3) | Keyed HMAC defeats offline brute force from a Redis dump alone — the attacker needs the pepper too, and the pepper lives in zombied process memory (Vault-loaded at boot, never on disk). The earlier salted-SHA256 design was brute-forceable in 1M ops; the pepper closes that. Constant-time comparison via `std.crypto.utils.timingSafeEql` (Zig) on the comparison side. |

**Crypto library**: `crypto.subtle` on both sides (CLI Node.js ≥20 standard library, browser Web Crypto API). Zero extra dependencies; identical API surface. No `tweetnacl` / `@noble/curves` to avoid drift.

**Verification-code entropy — future improvement (not v2 blocker).** Six digits = 1,000,000 entries; combined with the 5-attempt cap + 5-min TTL, brute-force success is 0.0005% per session-lifetime. Acceptable for v2.0. A future iteration should move to 8 alphanumeric characters in a TOTP-style segmented format (e.g. `X4K9-TQ`) — improves entropy ~37× while preserving human-typability. Tracked in *Out of Scope → Future improvements*.

---

## Session storage (Redis-backed, centralized)

**The current `src/auth/sessions.zig` is in-memory and capped at 64 concurrent sessions per process** (`max_sessions: usize = 64`). This is a production-incompatible shape: under multi-instance deployment, an Approve hitting pod A and a poll hitting pod B see different stores; the 64-session cap melts under any meaningful login concurrency; a rolling deploy loses every in-flight login.

**This spec rewrites session storage to use Redis** (the pool already exists at `src/queue/redis_pool.zig` from M69_004 — reuse, do not create a second connection layer). The in-memory `SessionStore` is deleted in entirety at the end of §1 — there is no dual-stack or per-instance fallback. RULE NLG-clean: no `legacy_*` naming; the new `SessionStore` replaces the old one byte-for-byte and the old struct is removed.

| Concern | Shape |
|---|---|
| Key schema | `auth:session:{session_id}` → JSON blob matching the *Session Schema* below. Hash-tagged so cluster mode keeps all session keys for one session on the same shard if/when Redis cluster is adopted. |
| Per-session TTL | `EXPIRE auth:session:{id} 300` (5 minutes) at creation; refreshed only on state transitions, not on read. Expiry is the primary garbage-collection mechanism (see *Session garbage collection* below). |
| Rate-limit counters | `auth:ratelimit:ip:{ip}:{minute_bucket}` (TTL 60s; token-bucket via `INCR` + `EXPIRE NX`). `auth:ratelimit:user:{clerk_user_id}:{hour_bucket}` (TTL 3600s). Separate keys, separate TTLs, no shared state with session blob. |
| Verify-attempt counter | Stored inside the session JSON blob (`verification_attempts`), not a separate Redis key — keeps the atomic transition + counter increment in one write. |
| Atomic verified → consumed transition | Lua script via `EVAL` — atomically: read session, check state == `verification_pending`, compare hash, increment-or-flip-state, write back, return ciphertext. Single round-trip; no partial-state windows. |
| Consume-idempotency window | After `verified → consumed`, the response payload (`{dashboard_public_key, ciphertext, nonce}`) is retained in the session blob for 60 seconds along with `consumed_client_fingerprint`. POST /verify retries within the window from the same fingerprint return the same payload; outside the window (or different fingerprint) → 410 `SessionConsumed`. After 60s the payload fields are wiped from the blob via a follow-up `EXPIRE`-driven hook (see *Fix 1* in Discovery). |
| Pub/sub for cross-instance signal (not v2) | Out of scope — pub/sub on session state changes is only needed if the CLI poll needs sub-second latency on Approve. Current 1-5s backoff makes the per-poll Redis GET acceptable. |

**Deployment requirement, pinned:** the API server MUST run against a Redis instance that is either:

- Single-node Redis reachable from every API pod (acceptable for dev / single-region prod), OR
- Redis Sentinel / Cluster with at least one reachable primary per pod

In-memory session storage is **not** acceptable under any multi-pod deployment topology. Sticky routing as a substitute is acceptable **only** for ephemeral dev environments; production requires the centralized store. Documented in `docs/AUTH.md` and in the deploy README.

---

## Session garbage collection

| Mechanism | Cadence | What it cleans |
|---|---|---|
| Redis `EXPIRE` (primary) | Per-key TTL (300s for pending sessions; 60s extension for the consume-idempotency window) | The vast majority of expired sessions. Redis evicts automatically. |
| Background sweep (secondary) | Every 60s, single sweep per pod | A defensive scan over `SCAN 0 MATCH auth:session:* COUNT 100` checking for any session blob whose `expires_at_ms` has elapsed but whose key is still alive (e.g., if a TTL was inadvertently cleared). Logs a metric on every prune. **No-op in steady state** — purely belt-and-suspenders. |
| Per-tenant / per-IP hard caps | Enforced at creation via rate limits, not via post-hoc pruning | Bounds the upper memory cost of any one actor. |
| Redis `maxmemory-policy` recommendation | Deploy-time config (not enforced by code) | Recommend `allkeys-lfu` so under memory pressure, least-frequently-accessed session keys are evicted first; documented in deploy README. |

**Hard invariant:** no in-memory session map in the API process. The entire session state lives in Redis. The 64-session in-memory cap is deleted with the old `SessionStore`. Per-request, the handler reads/writes Redis; no process-local cache.

---

## TLS / transport assumptions (pinned)

| Property | Requirement | Where enforced |
|---|---|---|
| **HTTPS-only** | All `/v1/auth/*` endpoints MUST be served over HTTPS. HTTP requests to these paths return HTTP 308 to the HTTPS equivalent (or 421 Misdirected Request if scheme cannot be promoted). | Load balancer / reverse proxy — `api.usezombie.com` already enforces this in prod; the API server itself does not terminate TLS. Document in `docs/AUTH.md` and the deploy README. |
| **HSTS** | Response includes `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload` on every response from the API hostname. | Load balancer or response-header middleware. New: add the middleware if absent (`src/http/middleware/security_headers.zig` — grep first; create if not present). |
| **Secure cookies** | N/A for the API server (zombied never sets cookies — the Clerk `__session` cookie lives on the dashboard host only). Documented to prevent future drift. | Inspection at code-review time. |
| **TLS minimum version** | TLS 1.2 minimum, TLS 1.3 preferred. | Load balancer config; documented in deploy README. |
| **No HTTP fallback for the dashboard's PATCH /approve call** | The dashboard `cli-auth/{session_id}` page calls PATCH /approve via `fetch()` — the browser refuses mixed content if the page is served over HTTPS but `fetch()` targets HTTP. Inherits HSTS protection automatically. | Browser behavior + dashboard origin policy. |

**No new code in v2.0** unless `security_headers.zig` middleware is absent (grep confirms before/during implementation). If absent, this spec authors a minimal one returning HSTS on every API response.

---

## Rate limits (pinned)

| Surface | Limit | Cause-of-action when exceeded |
|---|---|---|
| `POST /v1/auth/sessions` (session creation) | 10 / IP / minute | HTTP 429 with `Retry-After`; AUTH_PRESET `RateLimited`. |
| `POST /v1/auth/sessions` (session creation, after dashboard auth context attaches via Clerk on PATCH) | 20 / Clerk user / hour | HTTP 429 on the PATCH /approve call; dashboard surfaces "too many login attempts". |
| `POST /v1/auth/sessions/{id}/verify` (per session) | 5 attempts total | HTTP 410 `SessionAborted`; session transitions to `aborted`. |
| `GET /v1/auth/sessions/{id}` (polling) | Server-enforced minimum interval 750 ms; CLI honors `Retry-After` if returned | HTTP 429 with `Retry-After`; CLI's D29 backoff already respects this. |
| Expired sessions | Hard reject (HTTP 410) on any state-mutating call | Per-state semantics in the lifecycle table. |

Rate-limit storage: **Redis-backed token buckets** using the centralized session store from above. Per-IP and per-Clerk-user counters live at separate keys (`auth:ratelimit:ip:{ip}:{minute_bucket}` etc.) with their own TTLs; verify-attempt counters live inside the session JSON blob to keep the atomic transition in one write. **No process-local rate-limit state** — every limit decision reads Redis (the API process is rate-limit-stateless). If a middleware already exists at `src/http/middleware/rate_limit.zig`, mirror its shape; otherwise this spec authors a minimal token-bucket middleware atop the existing `redis_pool.zig`.

---

## Replay protections (invariants)

These are tested explicitly in the Test Specification.

1. **Verification code is single-use, with a bounded consume-idempotency window.** A successful POST /verify atomically transitions the session to `consumed` in the same Lua-scripted write that returns the ciphertext. Subsequent POST /verify calls within 60 seconds **from the same client fingerprint** (sha256 of `request.remote_addr || request.user_agent || session_id`) return the same payload (handles "consume succeeded, response lost, client retried"). Outside the 60-second window, OR from a different fingerprint, OR after the payload-retention TTL elapses → HTTP 410 `SessionConsumed`. After 60 seconds the cached payload is wiped from the session blob (TTL-driven, see *Session storage*).
2. **Ciphertext is single-read per client fingerprint.** Item (1)'s mechanism: only the originating fingerprint can replay during the window; any other source gets 410. This narrows the replay surface to "captured-network-packet-within-60s-from-same-source", which is dominated by the existing TLS + network-perimeter assumptions.
3. **Verified sessions cannot revert.** The state machine is monotonic; there is no path from `consumed` / `expired` / `aborted` back to any active state.
4. **PATCH /approve is single-write.** Calling PATCH /approve against a session already in `verification_pending` returns HTTP 409 Conflict. The dashboard MUST NOT retry PATCH /approve if it has previously succeeded for the same session.
5. **`session_id` is high-entropy.** Universally Unique Identifier version 7 (UUIDv7); 128 bits; Cryptographically Secure Pseudo-Random Number Generator (CSPRNG); not enumerable.
6. **`session_id` is capability-bearing**, classified equivalent to a password-reset token. **`session_id` appears only in the primary CLI-generated verification URL (`https://app.usezombie.com/cli-auth/{session_id}`) and in the API route paths that consume it (`/v1/auth/sessions/{id}`, `/v1/auth/sessions/{id}/approve`, `/v1/auth/sessions/{id}/verify`).** It MUST NOT appear in: log emits at info/warn/error level (use `redactSessionId()`); analytics / telemetry / metrics labels; secondary URLs (deep links, redirect targets, "share this page" affordances); error response bodies routed to non-trusted surfaces; copied diagnostic bundles / support tickets; tooling screenshots (e.g. dashboard error screenshots that capture the URL bar are operationally acceptable but document the sensitivity in any sharing flow). See *Log redaction* below for enforcement details.
7. **Audit log writes are append-only.** Audit events (see *Audit events* below) are written to a separate logger sink (`auth_audit`) and never to the general info-level application log. The audit sink is the only place where `session_id` appears verbatim.

---

## Consume-idempotency semantics (Fix 1)

The naïve "single-use code, single-read ciphertext" invariant creates a real production failure mode: the CLI POSTs /verify, the server completes the atomic consume, the response gets dropped on a network hiccup, the CLI retries — and the second call returns 410 `SessionConsumed` even though the CLI never received the first response. The user sees "login failed" after the server logged "login succeeded." Standard retry middleware makes this WORSE (the retry happens automatically and silently fails).

**Resolution (Option A from ChatGPT review):**

| Property | Value |
|---|---|
| Replay window | 60 seconds from `consumed_at_ms` |
| Fingerprint match | `sha256(request.remote_addr || request.user_agent || session_id)` — narrow enough to defeat network-level capture-and-replay from a different host; loose enough to survive the same CLI process retrying on its own connection. **`remote_addr` derivation is pinned: see *Trusted client IP extraction* below.** Mis-derivation (trusting unsigned `X-Forwarded-For` headers, or using the load-balancer IP) would either collapse all CLIs behind one proxy into the same fingerprint (loose replay) or let a client-supplied header forge a fingerprint match (forged replay). |
| Payload retention | The full `{dashboard_public_key, ciphertext, nonce}` payload is retained in the session blob for the 60-second window, then wiped via a TTL-driven hook. |
| Behavior in window | Same fingerprint → 200 + same payload (idempotent). Different fingerprint → 410 `SessionConsumed` (with `Retry-After: 0` and explicit "this session can no longer be recovered; re-run `zombiectl login`"). |
| Behavior outside window | Any source → 410 `SessionConsumed`. |
| CLI side | The login Effect's POST /verify retry policy: one retry at 1.5s if the first call times out or returns a 5xx; otherwise no retry. Two retries within 60s from the same `cli_priv` produce the same decrypted JWT (deterministic). |

**Tested by:** `test_verify_idempotent_replay_within_window` + `test_verify_replay_rejected_outside_window` + `test_verify_replay_rejected_from_different_fingerprint`.

---

## Trusted client IP extraction (Must-change B from ChatGPT review 4)

`request.remote_addr` for consume-idempotency fingerprinting AND for the per-IP rate-limit buckets MUST be derived from the trusted proxy chain. Two failure modes if this is wrong:

| Mis-derivation | Consequence |
|---|---|
| Use the load-balancer IP directly (no header parsing) | Every CLI behind the LB shares the same `remote_addr`. Fingerprint collisions for unrelated sessions; per-IP rate limit becomes a per-LB limit (one CLI exhausts the budget for all CLIs in its region). |
| Trust `X-Forwarded-For` / `Forwarded` from any source | A malicious client sets the header to a victim's IP; their fingerprint then matches the victim's replay window; per-IP rate-limit attribution forges to the victim. |

### Derivation rules (pinned)

| Rule | Detail |
|---|---|
| `TRUSTED_PROXY_IPS` configuration | Comma-separated allowlist of reverse-proxy / load-balancer IPs OR CIDR ranges. Loaded from env at zombied boot (`TRUSTED_PROXY_IPS=...`). Values per deploy: Cloudflare's IP ranges in production, the local proxy IP in dev, `127.0.0.1` in test. **Required** in any deploy that fronts zombied with a proxy. Direct-internet deploys (no proxy) set it to empty and the API trusts only the raw TCP peer address. |
| Header trust rule | If `request.remote_addr` (the raw TCP peer) is in `TRUSTED_PROXY_IPS`, parse `X-Forwarded-For` left-to-right and take the **first** entry that is NOT itself in `TRUSTED_PROXY_IPS` — that is the client IP. If `X-Forwarded-For` is absent in this case, fall back to the TCP peer. If the TCP peer is NOT in `TRUSTED_PROXY_IPS`, **ignore** every `X-Forwarded-For` / `Forwarded` header and use the TCP peer directly (client-supplied forwarding headers are ignored when the request did not arrive through a configured trusted proxy). |
| `Forwarded` (RFC 7239) | Treated the same as `X-Forwarded-For`. Same trust rule. |
| Misbehaving proxies | A proxy in `TRUSTED_PROXY_IPS` that drops `X-Forwarded-For` causes every request through it to attribute to the proxy IP itself. Operationally a deploy bug, not a security issue — caught by the per-IP rate limit firing at the proxy IP. Documented in deploy README. |
| Implementation surface | `src/http/middleware/trusted_client_ip.zig` (NEW unless an equivalent exists — grep first). Runs before `rate_limit.zig` and before any handler that reads `remote_addr`. Stores the derived client IP on the request context; downstream code reads from there, never from raw headers. |

### Coverage

- The consume-idempotency fingerprint hash (`Fix #1`) uses the derived IP, never the raw TCP peer when the request arrived via a trusted proxy.
- The per-IP rate-limit buckets (`auth:ratelimit:ip:{ip}:{minute_bucket}` per *Rate limits*) use the derived IP for the same reason.
- Audit events that carry an `ip` field record the derived IP.

**Tested by:** `test_trusted_client_ip_trusts_xff_from_known_proxy`, `test_trusted_client_ip_ignores_xff_from_unknown_source`, `test_trusted_client_ip_falls_back_to_tcp_peer_when_xff_absent`, `test_rate_limit_per_derived_client_ip` (asserts the bucket key is the derived IP, not the LB IP), `test_consume_fingerprint_uses_derived_client_ip`.

---

## Audit events (Fix 3 + Fix 4)

Auth events flow to a dedicated `auth_audit` logger sink, separate from the general application log. Sink target (file path, syslog facility, JSON-over-HTTPS endpoint) is deploy-time config; the spec pins only the event shape.

**Session-id pseudonymization (Fix 4 from ChatGPT review 3):** audit events do NOT carry the raw `session_id`. They carry `session_id_hash` (a keyed HMAC) plus `session_id_prefix` (the first 8 hex characters of the raw session_id, for human correlation across systems without the pepper). The HMAC key is a separate `AUDIT_LOG_PEPPER` env var — NOT the same value as `AUTH_SESSION_CODE_PEPPER` (separate concerns: code-disclosure resistance vs audit-log pseudonymization; rotating one should not invalidate the other).

| Property | Value |
|---|---|
| `session_id_hash` | `HMAC-SHA256(AUDIT_LOG_PEPPER, session_id)` — 32 bytes, base64url-encoded in events. Same session_id always produces the same hash (correlation preserved across all events for one session). |
| `session_id_prefix` | First 8 hex chars of the raw session_id. Human-typeable for ops debugging; not capability-bearing (8 hex chars = 32 bits = 4B entries, too coarse to brute-force the verify endpoint). |
| Incident-mode (full `session_id` in audit) | Gated by `AUTH_AUDIT_INCLUDE_FULL_IDS=true` env flag. Off by default. When enabled, zombied emits a startup warning to the `.auth` scope at WARN level: `"AUTH_AUDIT_INCLUDE_FULL_IDS=true — audit events will carry raw session_ids. Use only in dev/staging or under a documented incident."`. Production deploys MUST NOT enable this — enforced operationally, not by code. |

**Pepper provisioning** (mirrors `AUTH_SESSION_CODE_PEPPER`):

- `AUDIT_LOG_PEPPER` env var; required at zombied boot (fail-fast).
- Vault paths: `op://ops/ZMB_CD_PROD/AUDIT_LOG_PEPPER`, `op://ops/ZMB_CD_DEV/AUDIT_LOG_PEPPER`, `op://ops/ZMB_LOCAL_DEV/AUDIT_LOG_PEPPER`.
- Loaded once at process start; 32 bytes (256 bits) CSPRNG; base64url-encoded.
- Rotation: rotating `AUDIT_LOG_PEPPER` breaks cross-event correlation for past sessions but does not affect security (existing sessions continue to function — `verification_code_hmac` uses a different pepper). For v2.0, rotation is manual + Captain-approved.

**Restricted-routing requirement (Fix 4, deploy contract):**

- `.auth_audit` sink MUST NOT route to customer-visible logs (no product analytics, no customer-shared log aggregations, no public dashboards).
- `.auth_audit` sink MUST have tighter access control than `.auth` — operationally, this means a separate access-control list (ACL) or a separate log destination (e.g., a security-team-only S3 bucket, a separate Loki tenant, a separate syslog facility).
- The deploy README documents this requirement; the AUTH.md security-classification table includes it.
- This is a deploy-side discipline, not enforced by zombied code. The spec is honest about that: defense in depth + clear documentation, not a code-level guarantee.

### Audit event schemas

| Event | Trigger | Required fields |
|---|---|---|
| `auth.session.created` | POST /v1/auth/sessions returns 201 | `event`, `ts` (ISO-8601), `session_id_hash`, `session_id_prefix`, `token_name`, `ip`, `user_agent`, `request_id` |
| `auth.session.approved` | PATCH /v1/auth/sessions/{id}/approve returns 200 | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `clerk_user_id`, `token_name`, `ip`, `user_agent`, `request_id` |
| `auth.session.verify_failed` | POST /v1/auth/sessions/{id}/verify returns 400 (wrong code) | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `attempt` (1-5), `ip`, `user_agent`, `reason` (`invalid_code` / `rate_limited` / `not_approved`), `request_id` |
| `auth.session.verified` | POST /verify returns 200 (immediately before the consume write) | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `token_name`, `attempt` (winning attempt number), `ip`, `user_agent`, `request_id` |
| `auth.session.consumed` | The atomic consume write | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `consumed_client_fingerprint` (sha256 hex), `request_id` |
| `auth.session.consumed_replay` | POST /verify hits the consume-idempotency window from the same fingerprint | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `consumed_client_fingerprint`, `replay_within_ms`, `request_id` |
| `auth.session.aborted` | Session transitions to `aborted` via rate-limit-exhaustion, explicit DELETE, or replacement | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `reason` (`rate_limit_exceeded` / `explicit_cancel` / `replaced`), `clerk_user_id` (if known), `ip`, `request_id` |
| `auth.session.expired` | Background sweep finds an expired session | `event`, `ts`, `session_id_hash`, `session_id_prefix`, `expired_at_ms`, `created_at_ms` |
| `auth.ratelimit.exceeded` | Any rate-limit threshold tripped | `event`, `ts`, `surface` (`session_create` / `verify` / `patch_approve` / `poll`), `bucket_key` (e.g. `ip:1.2.3.4` or `user:u_abc`), `ip`, `request_id` |

**Shape contract:** every event is a single-line JSON object. `event` is the first field, `ts` is the second. Field order otherwise unspecified. No nested objects in the v2 contract; future extensions add new top-level fields. **No event carries the plaintext verification code, the HMAC of the code, the ciphertext, the public keys, or the raw `session_id` in default mode.**

**Sink contract:** `auth_audit` is a dedicated `std.log.scoped(.auth_audit)` (Zig) — separate scope from the general `.auth` scope so deploy-side log routing can fan it to a restricted sink per the requirement above.

---

## Log redaction — `session_id` is sensitive (Fix 4)

**`session_id` is capability-bearing** — combined with the verification code, it authorizes ciphertext release. Treated the same way a password-reset token would be.

| Surface | What can appear | What must NOT appear |
|---|---|---|
| `std.log.scoped(.auth)` info/warn/error | `request_id`, status names, error categories, sanitized error messages | Full `session_id`, full verification code, ciphertext bytes, public keys (informational risk only but redact anyway) |
| `std.log.scoped(.auth)` debug/trace | `session_id` redacted to first 8 hex chars (`abcd1234…`) + length suffix | Full `session_id` |
| `std.log.scoped(.auth_audit)` | `session_id_hash` (HMAC keyed with AUDIT_LOG_PEPPER) + `session_id_prefix` (first 8 hex chars) per Fix #4. Full `session_id` only when `AUTH_AUDIT_INCLUDE_FULL_IDS=true` (env-gated, startup warning emitted, dev/staging only). | Plaintext verification code (always redact; not even hashed), `verification_code_hmac` value, ciphertext bytes, raw session_id in default mode |
| HTTP response error bodies | `request_id`, error code (`UZ-AUTH-XXX`), generic message | `session_id` (the client already knows it; echoing it back is fine in success responses but never in errors routed to log-aggregators) |
| Metrics / traces | High-cardinality labels avoided | `session_id` as a tag (would explode cardinality + leak capability into observability surfaces) |

**Enforcement:**

- `src/auth/sessions.zig` and `src/http/handlers/auth/sessions.zig` log emits MUST use the `redactSessionId(id)` helper that returns `"{first8}…(len={n})"` — added to `src/auth/sessions.zig` as a pub fn.
- A grep-based test (`test_session_id_never_logged_unredacted`) scans the compiled binary's string table for any patterns matching `session_id` followed by a full hex sequence, OR scans the test-mode log capture for full-id matches. Implemented as a Zig integration test with a synthetic full request flow + log-capture assertion.
- The audit sink (`.auth_audit` scope) carries `session_id_hash` + `session_id_prefix` per Fix #4 (not the full ID by default); incident-mode env flag allows full IDs.
- Existing redaction patterns (`src/zombie/event_loop_harness_redaction_test.zig`, `src/executor/redaction_canary.zig`) are referenced for shape; the new auth-side helper does not duplicate that infrastructure, just mirrors its discipline.

**AUTH.md addition:** the security-classification table gains a row: `session_id` = "sensitive ephemeral capability — treat as password-reset token."

---

## Clock-skew handling (Fix 5)

| Property | Value |
|---|---|
| **Authoritative time source** | Server `std.time.milliTimestamp()`. All expiry decisions are made server-side. |
| **Client-side `expires_at_ms`** | Returned on `GET /v1/auth/sessions/{id}` as informational metadata for the D22 countdown display. **Never used by client for security decisions.** |
| **CLI countdown semantics** | Computed from `(expires_at_ms - clientNow)`. If the client clock skews forward by >5s, the countdown displays a value lower than the true server-side remaining time — login still succeeds if completed before the server's true expiry. If the client clock skews backward, the countdown overstates remaining time — the CLI hits an unexpected `Timeout` / `Expired` from the server before the visible countdown reaches zero. Acceptable; documented in D22 prose. |
| **Server-side grace window** | When evaluating expiry, the server uses `now_ms >= expires_at_ms + grace_ms` with `grace_ms = 30_000` (30 seconds). Prevents a client whose clock is slightly ahead of the server's from getting "expired" while the visible countdown still shows time remaining. |
| **Cross-pod clock consistency** | All pods must run NTP-synced clocks (deploy assumption). Drift >1 second between pods is a deploy-bug, not handled by this spec. |

**Tested by:** `test_server_grace_window_30s` + `test_client_countdown_does_not_gate_security`.

---

---

## Memory hygiene (intent-documented)

JavaScript runtimes do not guarantee zeroization. Intent is documented so reviewers can audit lifecycle scope rather than rely on memory guarantees the runtime cannot provide.

| Buffer | Intent | Where enforced |
|---|---|---|
| CLI `cli_priv` | Lives only inside the `loginEffect` closure; not stored to disk, not exported via `--debug`. Discarded when the Effect completes (success or failure). | `zombiectl/src/commands/auth.ts` — variable scope; no module-level holder. |
| Dashboard `dash_priv` | Lives only inside the React handler closure for the Approve click. Discarded after the PATCH /approve response. Never written to `localStorage` / `sessionStorage` / cookies. | `ui/packages/app/app/cli-auth/[session_id]/page.tsx` — handler-local. |
| Shared secret (CLI side) | Derived inside the decrypt step; passed through HKDF; discarded after AES-GCM init. | Same closure as `cli_priv`. |
| Shared secret (dashboard side) | Same shape; discarded after AES-GCM init. | Same closure as `dash_priv`. |
| Decrypted JWT buffer (CLI side) | Lives just long enough to write to `credentials.json`; the in-memory buffer is not held by any long-lived structure. | `zombiectl/src/lib/credentials.ts` — write-and-drop. |

Any future code that holds these buffers in long-lived structures (module-scoped maps, event-bus payloads, `console.log` calls) is a regression on this section.

---

## Captain decisions (pinned)

These were Open Questions in the prior revision. Captain answered May 17, 2026 (with cross-LLM input from ChatGPT). They are now part of the spec, not subject to re-debate during implementation.

| # | Question | Decision | Rationale recap |
|---|---|---|---|
| Q1 | `token_name` visibility | Client-side in `credentials.json` (visible in `zombiectl auth status`) **AND** lightweight server audit row recording "device X (token_name=<label>) logged in at <ts>" without binding `token_name` into JWT claims. JWT stays Clerk-owned and stateless. | Audit visibility without polluting Clerk's identity model. |
| Q2 | Verification-code direction | **Dashboard shows code, CLI prompts.** | Matches OAuth Device Flow expectations; user already trusts browser visual surface; avoids CLI-generated phishing confusion. |
| Q3 | Backward compatibility during migration | **No compatibility shim — plaintext PATCH never shipped in production.** Captain directive May 17, 2026 (third ChatGPT review): there is no in-flight production traffic on the plaintext PATCH path; treat it as if it never existed. The Zig handler implements only the new shapes (`POST /sessions`, `GET /sessions/{id}`, `PATCH /sessions/{id}/approve`, `POST /sessions/{id}/verify`, `DELETE /sessions/{id}`, `DELETE /sessions/all`). The plaintext PATCH operation is deleted from `src/http/handlers/auth/sessions.zig`, deleted from `public/openapi/paths/authentication.yaml`, removed from `public/openapi.json` on regeneration, and gets zero audit events / zero error variants / zero tests. Any client (including dev/staging-side stale `zombiectl` builds) hitting the old path receives the standard 404 `route not found` — no `UpgradeRequired` ceremony, no deprecation header, no Sunset header. RULE NLG-perfect: there is no "legacy" path because there was never anything in production to be legacy to. | Captain's call: pre-v2.0 with no prod traffic on the old shape means the cleanest move is to act as if plaintext never existed. The spec's security claim ("plaintext JWT removed from server-side transport/storage") is then unconditionally true on the production system from day one, with no deprecation window during which the claim is false. |
| Q4 | Crypto library | `crypto.subtle` both sides. | Zero extra dependencies; identical API surface browser + Node.js; avoids `tweetnacl` / `@noble/curves` drift. |
| Q5 | Verification-code transport | Dedicated `POST /v1/auth/sessions/{id}/verify` endpoint. **Never** via query parameter; **never** overloaded onto the GET polling endpoint. | Clean state-machine semantics; atomic verified→consumed; clean rate-limit per surface; clean audit logs. |
| Q6 | ECDH + verification code, or verification code only? | **Both.** | Verification code closes the phishing-without-terminal attack class; ECDH closes the server-side-disclosure attack class. Implementation cost is small once the substrate is in place; threat model is cleaner with both shipping together. AUTH.md must continue to state explicitly that ECDH is NOT authentication. |
| Q7 | Effect-TS error variant naming | Follow M74_001's tagged-class convention. If M74_001 has not landed when this spec is implemented, M74_002 ships plain `Error` subclasses (`VerificationFailedError`, `DecryptError`, …) and M74_001 maps them to tagged classes during its bulk migration. | Don't invent a one-off auth-specific naming style. |

---

## Implementing agent — read these first

1. **The Threat Model + Why each component exists sections above** — every decision below traces to them.
2. `docs/AUTH.md` end-to-end. **Flow 1** is the primary target. Read this before writing any spec claim; prior agents hit Captain rejections for getting Flow 1 vs Flow 3 confused.
3. [`supabase/cli`'s `apps/cli/src/next/commands/login/login.handler.ts`](https://github.com/supabase/cli/blob/main/apps/cli/src/next/commands/login/login.handler.ts) (228 lines) end-to-end. The pattern being ported (verification-code prompt + ECDH-decrypt + `credentials.json` persist).
4. `zombiectl/src/commands/auth.{js,ts}` + `zombiectl/src/commands/core.ts` — current `commandLogin` (~70-211), `commandLogout` (~213-220), and the named-stage decomposition from M68 §13 D27. M74_002 rewrites these on top of Effect-TS (if M74_001 has landed) or the current async shape (if not).
5. `zombiectl/src/lib/error-map-presets.ts` — `AUTH_PRESET` is the existing error remap table. §4 below extends it with the six D28 codes plus the M74_002 protocol-specific variants.
6. `zombiectl/test/login.unit.test.ts` (post-D42 migration) — the pre-existing exit-code + stdout-shape contracts. The plural-flagged contracts that pin `exit 0` when hydration fails are binding.
7. `src/auth/sessions.zig` + `src/http/handlers/auth/sessions.zig` — the SessionStore (in-memory) and the current `PATCH` / `POST` / `GET` handler surface.
8. The dashboard `/cli-auth/{session_id}` page in `ui/packages/app/app/` (find via `grep -rln 'cli-auth' ui/packages/app/`). Read it end-to-end before drafting §3.
9. `docs/v2/done/M71_001_P1_CLI_COMMAND_EFFECTS_IDENTITY_SESSION_TRACE.md` — session_id + device_id base properties on every PostHog event. New analytics emits from this spec inherit that base shape.
10. `docs/v2/done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` §13 — the parent spec's "Deferred to follow-up" list for D20/D21/D24/D25/D26/D32, which this spec inherits.

---

## Applicable Rules

- `docs/AUTH.md` — applies to every change. Flow 1 sequence diagrams + field shapes update as part of this work; the Security-properties-by-layer table from above lands durable in AUTH.md.
- `docs/REST_API_DESIGN_GUIDELINES.md` — applies to session-handler shape changes. Endpoint additions: `PATCH /v1/auth/sessions/{id}/approve`, `POST /v1/auth/sessions/{id}/verify`.
- `docs/greptile-learnings/RULES.md` — universal. **RULE NLG** forbids `legacy_*` / `V2` framing pre-2.0.0 (`cat VERSION` < 2.0.0). **The Q3 directive — plaintext PATCH never existed in production, deleted entirely from the milestone — is RULE NLG-perfect:** there is no compat shim, no `acceptPatchPlaintextDeprecated` handler, no deprecation header path. **RULE NDC** (no dead code at write time) and **RULE NLR** (touch-it-fix-it) both fire on the auth-command rewrite. **RULE UFS** — every protocol literal (`"m74-002-v1"` HKDF info, `"verification_pending"` status string, `"ciphertext"` JSON key, etc.) goes through a named constant.
- `docs/ZIG_RULES.md` — applies to `src/auth/sessions.zig` + `src/http/handlers/auth/sessions.zig`. PostgreSQL drain lifecycle if any new DB-backed work lands; default to in-memory unless drift demands persistence.
- `docs/SCHEMA_CONVENTIONS.md` — N/A (sessions stay in-memory in v2.0 per `src/auth/sessions.zig`'s current shape).
- TypeScript: `as any` / `!` / `@ts-expect-error` forbidden in this diff per the global TS-strict migration (see `feedback_ts_migration_intent`). Every Effect carries a discriminated-union error type.

---

## Overview

**Goal (testable):** `zombiectl login` rejects a wrong verification code with a typed `VerificationFailedError` and a documented exit code; a successful login fetches the token via ECDH-decrypt of the dashboard's PATCH /approve ciphertext (never in plaintext on the wire or in the session row); the verification code is never persisted server-side in plaintext; the minted credential carries a platform-default device label visible in `zombiectl auth status` and on a server audit row; an already-authenticated CLI prompts to replace the session before kicking off a fresh browser flow; the poll loop survives a single transient blip with exponential backoff and jitter; workspace-hydration failures surface as a stderr warning while keeping the success exit code; and every recoverable poll error maps to one of six distinct `AUTH_PRESET` codes.

**Three distinct property families:**

1. **Protocol hardening.** Verification code (authorization binding) + ECDH ciphertext transport (confidentiality) + hashed code storage (code-disclosure resistance) + dedicated verify endpoint (clean state machine + clean replay semantics) + rate limits (brute-force resistance) + pinned crypto primitives.
2. **CLI command surface.** Login handler rewrite on Effect-TS (or current async shape if M74_001 hasn't landed), `--token-name` flag with platform-safe defaults, idempotency check, `/me` token-validation ping, argv-leak warning for `--token`, TTY-priority env-var resolution, `zombiectl logout --all`.
3. **Login UX hardening.** Session-expiry countdown, fail-loud workspace hydration on stderr, per-error AUTH_PRESET tightening (six codes), exponential-backoff polling with jitter, single-blip tolerance inside the poll loop.

All three are required to ship a coherent CLI auth surface. Either of (1) or (3) alone leaves operators with a UX gap; (2) is the surface they touch. Consolidating them here lets the entire auth-flow ship in one PR-review-able unit instead of dribbled across three milestones.

---

## Files Changed (blast radius)

### API (Zig)

| File | Action | Why |
|------|--------|-----|
| `src/auth/sessions.zig` | REWRITE | **DELETE** the in-memory `SessionStore` (StringHashMap + `max_sessions: usize = 64`) in entirety. Rewrite on Redis (use `src/queue/redis_pool.zig` from M69_004). New `SessionState` per the *Session Schema* below. Embed the Lua EVAL script for the atomic verify→consume transition. Add `redactSessionId(id) []const u8` helper for log-redaction use. Add the secondary background sweep helper. |
| `src/http/handlers/auth/sessions.zig` | REWRITE | **Delete the existing plaintext PATCH handler in entirety** (no `acceptPatchPlaintextDeprecated` shim — Q3 directive). Implement: `POST /v1/auth/sessions` (creation), `GET /v1/auth/sessions/{id}` (polling, status + cli_public_key + token_name + expires_at_ms only), `PATCH /v1/auth/sessions/{id}/approve` (dashboard, Clerk-authenticated), `POST /v1/auth/sessions/{id}/verify` (CLI, no auth, single Lua-EVAL atomic verified→consumed + consume-idempotency window per Fix 1), `DELETE /v1/auth/sessions/{id}`, `DELETE /v1/auth/sessions/all`. Every log emit uses `redactSessionId()`. Every response includes the standard error envelope; never echoes session_id in error bodies. |
| `src/http/middleware/rate_limit.zig` (existing if present, else NEW) | EDIT or CREATE | Redis-backed token-bucket: per-IP for session creation, per-Clerk-user for PATCH /approve. Verify-attempt counter lives in the session blob (not here). Emits `auth.ratelimit.exceeded` audit event on every block. **IP source: the derived client IP from `trusted_client_ip.zig`, NEVER the raw TCP peer when behind a proxy.** |
| `src/http/middleware/trusted_client_ip.zig` (existing if present, else NEW per Must-change B) | EDIT or CREATE | Derives the trusted client IP from the proxy chain per *Trusted client IP extraction*. Reads `TRUSTED_PROXY_IPS` env at boot; parses `X-Forwarded-For` / `Forwarded` only when the TCP peer is a configured trusted proxy. Stores the derived IP on the request context for downstream consumers. Runs before `rate_limit.zig` and before any handler reading `remote_addr`. |
| `src/http/middleware/security_headers.zig` (existing if present, else NEW per Fix 6) | EDIT or CREATE | Adds `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload` to every response. Confirm via `grep -rln 'Strict-Transport-Security' src/` before authoring — if the load balancer already adds it, the middleware is a defense-in-depth no-op (still authored so test `test_hsts_header_present_on_auth_responses` passes deterministically in unit tests without a load balancer). |
| `src/auth/audit.zig` (NEW) | CREATE | Emits the structured events listed in *Audit events* — one function per event variant taking the typed payload struct. Loads `AUDIT_LOG_PEPPER` from Vault at boot (fail-fast if missing). Computes `session_id_hash = HMAC-SHA256(AUDIT_LOG_PEPPER, session_id)` and `session_id_prefix = first8(session_id)` for every event; raw `session_id` only when `AUTH_AUDIT_INCLUDE_FULL_IDS=true` (env-gated). Uses `std.log.scoped(.auth_audit)` (separate from `.auth` scope per Fix 3 + Fix 4). JSON-line format; one field per event row in the audit-events table. |
| `src/auth/session_store_redis.zig` (NEW, called from `src/auth/sessions.zig`) | CREATE | Encapsulates the Redis-backed CRUD: `create()`, `get()`, `approve()`, `verifyAndConsume()` (Lua EVAL), `delete()`, `deleteAllForUser()`, `runBackgroundSweep()`. Calls into `src/queue/redis_pool.zig`. Single source of truth for session storage; the handler never touches Redis directly. |

### CLI (TypeScript / `zombiectl/`)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/auth.ts` | REWRITE | New login flow as Effect-TS Effect (on M74_001 substrate if present) or async function (if not). ECDH keypair gen → POST /sessions → open browser → poll with backoff + countdown → prompt for verification code → POST /verify → ECDH-decrypt → write `credentials.json` → `/me` ping → emit success. Plus `logoutEffect` (per-session) and `logoutAllEffect` (D32 — calls server-side `DELETE /v1/auth/sessions/all` for the active user). |
| `zombiectl/src/commands/core.ts` | EDIT | Hosts the named stages from M68 §13 D27. Extend `pollUntilComplete` (D29 backoff + D30 blip tolerance + D22 countdown), `persistAndHydrate` (D23 stderr warning), `emitLoginResult` (unchanged). Add D20 idempotency check (prompts before overwriting an existing credential). |
| `zombiectl/src/lib/credentials.ts` | EDIT | Persist `{ token, token_name }` (token_name non-optional, defaulted by the login path). Mode 0o600. Add `tokenName` accessor used by `zombiectl auth status`. |
| `zombiectl/src/lib/error-map-presets.ts` | EDIT | Extend `AUTH_PRESET` with six D28 codes (`InvalidSession`, `ExpiredSession`, `NetworkError`, `RateLimited`, `Timeout`, `Interrupted`) plus M74_002 protocol-specific variants (`VerificationFailed`, `DecryptError`, `SessionAborted`, `SessionConsumed`, `MeValidation`). No `UpgradeRequired` per Q3 directive (deprecated PATCH path does not ship). |
| `zombiectl/src/lib/argv-redact.ts` (NEW) | CREATE | D25 — detect `--token <value>` in `process.argv`, emit a one-line stderr warning that the token will leak into shell history and process lists; recommend `ZMB_TOKEN` env-var path. Pure function; no I/O. |
| `zombiectl/src/program/auth-token.ts` | EDIT | D26 — TTY-priority env-var resolution: when stdin is a TTY, `ZMB_TOKEN` > `ZOMBIE_TOKEN` > `credentials.json`. When stdin is not a TTY (scripted), `credentials.json` > `ZMB_TOKEN` > `ZOMBIE_TOKEN` (scripts expect credentials-file precedence). |
| `zombiectl/src/lib/platform.ts` (NEW or EDIT) | CREATE or EDIT | `defaultTokenName()` returns `"macos-cli" | "linux-cli" | "windows-cli" | "freebsd-cli"` from `process.platform`. No hostname. |
| `zombiectl/src/program/io.ts` | EDIT | Add the inline-update spinner pattern for D22 countdown ("Session expires in MM:SS"). Reuse existing `stream.write("\r" + text)` pattern. |
| `zombiectl/src/lib/me-ping.ts` (NEW) | CREATE | D24 — `pingMe()` returns Promise<true \| ApiError>. Called by login after `credentials.json` write to validate the token actually works. Failure surfaces a clear "token saved but failed validation" error and exit code distinct from `VerificationFailedError`. |
| `zombiectl/src/errors/auth.ts` (CREATE; or extend M74_001's if present) | CREATE or EDIT | `VerificationFailedError`, `DecryptError`, `SessionAbortedError`, `SessionConsumedError`, `MeValidationError`. Per Q7, tagged-class form if M74_001 landed, plain `Error` subclasses otherwise. **No `UpgradeRequiredError`** (Q3 directive). |
| `zombiectl/test/auth/login.unit.test.ts` (NEW) | CREATE | ECDH round-trip + verification-code happy/sad paths + atomic-consume-replay assertions. |
| `zombiectl/test/login.unit.test.ts` | EDIT | Extend with D22 countdown ticks, D23 stderr warning, D28 per-code mapping, D29 backoff math, D30 single-blip survival, D20 idempotency prompt, D24 /me ping failure, D25 argv-leak warning, D26 env-var precedence. Existing exit-code pin tests must continue to pass byte-for-byte. |
| `zombiectl/test/auth/login.acceptance.spec.ts` (NEW) | CREATE | One acceptance case: full flow with injected single 503 + correct verification code → exit 0 with credentials persisted. Mirrors §13 D31 acceptance pattern. |

### UI (TypeScript / `ui/packages/app/`)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/cli-auth/[session_id]/page.tsx` (grep for actual path) | REWRITE | Read `session_id` from path param only (no `public_key` in URL). GET /v1/auth/sessions/{id} to fetch `cli_public_key` + `token_name`. Display "Approve CLI login for **{token_name}**?". On Approve: mint JWT via Clerk → generate `(dash_priv, dash_pub)` → derive shared via ECDH × cli_public_key → HKDF → AES-256-GCM encrypt(jwt) → generate 6-digit code → hash with sha256(code \|\| session_id) → PATCH /approve → display the plaintext code with "Type this into your CLI" hint. When a previous session exists for the same Clerk user, surface "This will replace your previous CLI session on **{previous_token_name}**" alongside the Approve button. |
| `ui/packages/app/lib/auth/cli-flow.ts` (NEW or EDIT — grep first) | CREATE or EDIT | Pure crypto helpers: `generateEphemeralKeypair()`, `deriveSharedKey(privateKey, publicKey)`, `encryptJwt(jwt, key)`, `hashVerificationCode(code, sessionId)`. All `crypto.subtle`-based. Unit-tested in isolation. |
| `ui/packages/app/tests/e2e/acceptance/cli-acceptance/lifecycle-after-login.spec.ts` (or successor) | EDIT | Assert verification-code happy path + wrong-code reject + device-label visibility + already-authenticated dashboard message. |

### OpenAPI

| File | Action | Why |
|------|--------|-----|
| `public/openapi/paths/authentication.yaml` | REWRITE | Source of truth for the `/v1/auth/sessions*` paths. **Delete the existing `patch_auth_session` operation (plaintext token shape) entirely** (Q3 directive — never shipped to production, no deprecation window needed). Add: `PATCH /v1/auth/sessions/{session_id}/approve`, `POST /v1/auth/sessions/{session_id}/verify`, `DELETE /v1/auth/sessions/{session_id}`, `DELETE /v1/auth/sessions/all`. Update `POST /v1/auth/sessions` request to require `public_key` + `token_name`. Narrow `GET /v1/auth/sessions/{id}` response: drop the `token` field; add the new state-machine enum (`pending` / `verification_pending` / `consumed` / `expired` / `aborted`); add `cli_public_key` + `token_name` + `expires_at_ms` fields. Add per-error schemas for `VerificationFailed`, `SessionConsumed`, `SessionAborted`, `SessionExpired`, `NotApproved`, `RateLimited`. |
| `public/openapi.json` | REGENERATE | Compiled artifact (~4363 lines today). Regenerate via `make check-openapi` (bundles YAML → JSON + Redocly lint + error-schema + URL-shape checks per Makefile). Must regenerate clean from the YAML sources; any drift between source and compiled artifact is a Continuous Integration (CI) failure. |
| `public/openapi/components/schemas.yaml` (if shared schemas land) | EDIT | New shared request/response shapes (`SessionApproveRequest`, `SessionVerifyRequest`, `SessionVerifyResponse`, `SessionStateEnum`) consumed by multiple operations. Inline the schemas in `authentication.yaml` if they are operation-local; promote to `components/schemas.yaml` only if reused. |
| `public/openapi/AGENTS.md` | EDIT (only if rules drift) | Update if the spec authoring rules (file-naming, operation-id conventions) need clarification for the new auth endpoints. No-op if the existing conventions already cover them. |

### Playbooks (Vault provisioning + e2e fixtures)

Bootstrap and preflight playbooks gain new vault items per the *Bootstrap playbook updates* section below. All edits mirror the existing `encryption-master-key` shape (one vault item per environment, `credential` field carries the value, `openssl rand -hex 32` generates it).

| File | Action | Why |
|------|--------|-----|
| `playbooks/001_bootstrap/001_playbook.md` | EDIT | §1.3a (Generate Encryption Master Key) is extended with §1.3b (Generate Auth Pepper Keys) for `AUTH_SESSION_CODE_PEPPER` + `AUDIT_LOG_PEPPER` and §1.3c (Provision E2E Fixture Email Identities) for `e2e-fixture-email/regular` + `e2e-fixture-email/admin`. The §2.0 agent-step vault inventory table gains rows for the three new items. The §1.3 hand-off message gets one extra bullet pointing to §1.3b + §1.3c. |
| `playbooks/002_preflight/001_playbook.md` | EDIT | DEV-vault inventory (around line 66) and PROD-vault inventory (around line 42) each gain three rows: `auth-session-code-pepper`, `audit-log-pepper`, `e2e-fixture-email`. Consumer column for the peppers names `zombied` (loaded at boot via `src/state/vault.zig`); for `e2e-fixture-email` names the Playwright + Vitest e2e suites under `ui/packages/app/tests/e2e/`. |
| `playbooks/001_bootstrap/03_auth_pepper_provision.sh` (NEW) | CREATE | Optional automation script mirroring `02_vercel_env.sh`'s shape. Runs `openssl rand -hex 32` for each pepper per environment, idempotently upserts into `$VAULT_DEV` / `$VAULT_PROD` via `op item create` (skips if already present). `--check` mode reads-only and exits 1 if items missing. Reduces human error in the bootstrap; the playbook prose path stays valid for manual provisioning. |

### Docs

| File | Action | Why |
|------|--------|-----|
| `docs/AUTH.md` | EDIT | Flow 1 sequence diagram + field shapes updated. New "Security properties by layer" table. Explicit "what's protected vs what isn't." Boundary diagram. Replay-semantics + endpoint-trust-boundaries subsections (see §7). Explicit redirect for autonomous-agent auth → M75_xxx. Explicit "Relationship to `zmb_t_` and agent keys" subsection so future readers do not conflate the surfaces. **Explicit "Flow 2 is unchanged"** confirmation so future readers do not assume the dashboard signup/login path is in scope. |

---

## Session Schema

Replaces the implicit shape currently in `src/auth/sessions.zig`:

```zig
const SessionStatus = enum {
    pending,
    verification_pending,
    consumed,
    expired,
    aborted,
};

const SessionState = struct {
    session_id: []const u8,                  // UUIDv7
    status: SessionStatus,

    // Authorization material — set at creation
    cli_public_key: []const u8,              // base64url P-256 SubjectPublicKeyInfo
    token_name: []const u8,                  // operator-facing label

    // Confidentiality material — set on PATCH /approve
    dashboard_public_key: ?[]const u8,       // base64url P-256
    ciphertext: ?[]const u8,                 // base64url AES-256-GCM output
    nonce: ?[]const u8,                      // base64url 96-bit
    verification_code_hmac: ?[32]u8,         // HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || code) — Fix #1

    // Rate-limit + audit metadata
    verification_attempts: u8,               // capped at 5
    created_at_ms: i64,
    expires_at_ms: i64,                      // created_at + 5 min
    approved_at_ms: ?i64,
    consumed_at_ms: ?i64,
    aborted_reason: ?[]const u8,             // "rate_limit_exceeded" | "explicit_cancel" | "replaced"

    // Clerk user attached at PATCH /approve for the audit row
    clerk_user_id: ?[]const u8,

    // Consume-idempotency window (Fix 1): 60s replay window for the same fingerprint
    consumed_client_fingerprint: ?[32]u8,    // sha256(remote_addr || user_agent || session_id)
    consume_payload_expires_at_ms: ?i64,     // consumed_at_ms + 60_000; null until consume
};
```

`token` does not appear on this struct. The server never holds the plaintext JWT.

---

## Bootstrap playbook updates (Vault provisioning)

The Fix #1 pepper (`AUTH_SESSION_CODE_PEPPER`), Fix #4 pepper (`AUDIT_LOG_PEPPER`), and the e2e fixture-email identities all live in 1Password vaults and MUST be provisioned by the bootstrap playbook so the API process can boot and the e2e tests can authenticate. Mirrors the existing `encryption-master-key` discipline (one item per environment, never reuse across DEV/PROD, never on disk).

### §1.3b — Generate Auth Pepper Keys (new bootstrap step, slots after §1.3a)

Generate two 32-byte CSPRNG keys per environment for the auth-session pepper and the audit-log pepper. Run **once per environment** (never reuse across DEV/PROD).

```bash
# DEV — session-code HMAC pepper (Fix #1)
openssl rand -hex 32   # → store in op://$VAULT_DEV/auth-session-code-pepper/credential

# DEV — audit-log HMAC pepper (Fix #4)
openssl rand -hex 32   # → store in op://$VAULT_DEV/audit-log-pepper/credential

# PROD — session-code HMAC pepper
openssl rand -hex 32   # → store in op://$VAULT_PROD/auth-session-code-pepper/credential

# PROD — audit-log HMAC pepper
openssl rand -hex 32   # → store in op://$VAULT_PROD/audit-log-pepper/credential
```

**Rotation:** rotating `auth-session-code-pepper` invalidates every in-flight CLI login session (their HMACs no longer match). Operationally cheap because sessions are 5-min-TTL — drain old sessions on rotation by waiting 5+ minutes between provisioning the new pepper and cutting over. Rotating `audit-log-pepper` breaks cross-event correlation for past sessions but does not affect security. For v2.0, both rotations are manual + Captain-approved.

### §1.3c — Provision E2E Fixture Email Identities (new bootstrap step)

Two long-lived Clerk identities are needed for the e2e auth-flow suite (Playwright + Vitest under `ui/packages/app/tests/e2e/`). The identities are pre-provisioned in Clerk's DEV instance (the e2e tests run against DEV); credentials stored in Vault so tests can read them without hardcoding.

```bash
# 1. In Clerk DEV dashboard, create two test users:
#    - regular@usezombie.dev (regular tenant member role)
#    - admin@usezombie.dev (tenant admin role)
# 2. Generate a strong password for each (openssl rand -base64 24 or similar).
# 3. Store both as separate 1Password items:
#    op://$VAULT_DEV/e2e-fixture-email/regular  → fields: email, password
#    op://$VAULT_DEV/e2e-fixture-email/admin    → fields: email, password
```

**PROD vault**: NOT required. The e2e suite runs against DEV only. If a future spec adds prod-canary smoke tests, that spec authors the prod entries separately.

**Why this lives in bootstrap, not the auth spec proper:** the e2e fixtures are bootstrap infrastructure (the equivalent of the existing `encryption-master-key` item). The spec authoring discipline says infra prerequisites live in playbooks; the spec's job is to reference them.

### §2.0 agent-step vault inventory additions

The §2.1 table in `playbooks/001_bootstrap/001_playbook.md` gains three rows (and `playbooks/002_preflight/001_playbook.md` adds the same entries to both the DEV and PROD vault tables):

| Item | Field(s) | Value source | Consumer |
|---|---|---|---|
| `auth-session-code-pepper` | `credential` (32-byte hex) | `openssl rand -hex 32` per §1.3b | `zombied` — loaded at boot via `src/state/vault.zig` as `AUTH_SESSION_CODE_PEPPER`. Process fails fast if missing. |
| `audit-log-pepper` | `credential` (32-byte hex) | `openssl rand -hex 32` per §1.3b | `zombied` — loaded at boot as `AUDIT_LOG_PEPPER`. Process fails fast if missing. |
| `e2e-fixture-email/regular` | `email`, `password` | Clerk DEV user creation per §1.3c | Playwright e2e suites; Vitest integration tests under `ui/packages/app/tests/e2e/`. |
| `e2e-fixture-email/admin` | `email`, `password` | Clerk DEV user creation per §1.3c | Same — admin-role scenarios. |

### §1.3 hand-off message extension

The hand-off message at the end of §1.3 in `001_playbook.md` gains one extra item:

> "…Store them in 1Password vaults `ZMB_CD_PROD` / `ZMB_CD_DEV` per `playbooks/001_bootstrap/001_playbook.md §2.0`, **including the new entries from §1.3b (auth peppers) and §1.3c (e2e fixture emails)**, then run `./playbooks/002_preflight/001_gate.sh` and proceed with `playbooks/002_preflight/001_playbook.md`."

### Optional automation: `playbooks/001_bootstrap/03_auth_pepper_provision.sh`

Mirrors `02_vercel_env.sh`'s shape: idempotent upsert of pepper items into both vaults via `op item create`; `--check` mode validates presence and exits 1 on missing items. Reduces drift if the bootstrap is replayed. Optional — manual provisioning via §1.3b is equally valid.

```bash
# Pseudo-shape (full script lands in this spec's implementation):
./playbooks/001_bootstrap/03_auth_pepper_provision.sh           # apply, idempotent
./playbooks/001_bootstrap/03_auth_pepper_provision.sh --check   # exit 1 on missing
```

---

## UI access (dashboard → API)

The dashboard reaches the API via the same-origin proxy pattern already configured in `ui/packages/app/next.config.ts`:

```ts
async rewrites() {
  const backend = process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.usezombie.com";
  return [{ source: "/backend/:path*", destination: `${backend}/:path*` }];
}
```

**No changes to `next.config.ts` are required by this spec.** The existing `/backend/:path*` rewrite already covers every new auth-session endpoint:

| Dashboard call (browser) | Proxied to (Next.js → zombied) | Purpose |
|---|---|---|
| `GET /backend/v1/auth/sessions/{id}` | `GET /v1/auth/sessions/{id}` | Fetch `cli_public_key` + `token_name` + state for the `/cli-auth/{session_id}` page |
| `PATCH /backend/v1/auth/sessions/{id}/approve` | `PATCH /v1/auth/sessions/{id}/approve` | Dashboard submits ECDH-encrypted payload after user clicks Approve |
| `DELETE /backend/v1/auth/sessions/{id}` | `DELETE /v1/auth/sessions/{id}` | Dashboard cancel button (UX guardrail; not v2.0-required but in the API contract for the future sessions surface) |
| `DELETE /backend/v1/auth/sessions/all` | `DELETE /v1/auth/sessions/all` | Future "abort all in-flight logins" button on a sessions surface; out of v2.0 dashboard UI but the route exists |

**Bearer token**: every dashboard call carries `Authorization: Bearer <user-jwt>` (Token B) minted via `useAuth().getToken({template:"api"})` per the existing Flow 2 pattern. The new `/cli-auth/{session_id}` page is no different from any other dashboard page in this respect.

**Page authoring**: `ui/packages/app/app/cli-auth/[session_id]/page.tsx` is the only new dashboard route. Implementation details are in §3 (Dashboard handler). The pure crypto helpers live in `ui/packages/app/lib/auth/cli-flow.ts` (covered in Files Changed → UI).

**Server Components vs Client Components**: the `/cli-auth/{session_id}` page is a **Client Component** (uses `crypto.subtle`, reads browser-only fragments, calls `useAuth()` from Clerk's React SDK). The shell page can be a Server Component that renders a Client Component island; implementing agent decides at PLAN time based on whether the shell needs server-side props.

**No SSE / EventSource** on this flow — the dashboard handler is a single user-click action (Approve), not a long-lived stream. The existing SSE Route Handler pattern (used for `/v1/zombies/{id}/events/stream`) is not touched.

---

## Sections (implementation slices)

### §1 — Wire protocol (Zig + handler surface)

Implement the four endpoints below. Each has a fixed shape; all enums and JSON keys are named constants (RULE UFS).

**`POST /v1/auth/sessions`** (no auth — anyone can create a session, rate-limited per IP):

```
Request:  { "public_key": "<base64url-P-256-SPKI>",
            "token_name": "<≤64 chars>" }
Response: 201 { "session_id": "<uuidv7>" }
Errors:   429 RateLimited (with Retry-After header)
          400 InvalidPublicKey (malformed base64url or wrong curve)
          400 InvalidTokenName (over 64 chars, control characters)
```

**`GET /v1/auth/sessions/{id}`** (no auth — anyone with the ID can poll for status; intentionally not gated by Clerk so the unauthenticated CLI poll works):

```
Response shapes:
  200 { "status": "pending",
        "cli_public_key": "<base64url>",
        "token_name": "<label>",
        "expires_at_ms": <i64> }
  200 { "status": "verification_pending",
        "cli_public_key": "<base64url>",
        "token_name": "<label>",
        "expires_at_ms": <i64> }
  410 { "status": "expired" }
  410 { "status": "aborted", "reason": "<rate_limit_exceeded|explicit_cancel|replaced>" }
  410 { "status": "consumed" }      // session terminal-success, ciphertext already released
  429 RateLimited
```

GET NEVER returns `ciphertext`, `nonce`, `dashboard_public_key`, `verification_code`, or `verification_code_hmac`. (Invariant 1.)

**`PATCH /v1/auth/sessions/{id}/approve`** (requires Clerk JWT in Authorization header; the JWT's `sub` becomes the session's `clerk_user_id`):

```
Request:  { "dashboard_public_key": "<base64url-P-256-SPKI>",
            "ciphertext": "<base64url>",
            "nonce": "<base64url-12-bytes>",
            "verification_code": "<6-digit-plaintext>" }
Response: 200 (empty body)
Errors:   401 (Clerk JWT missing/invalid)
          409 Conflict (session already in verification_pending — single-write invariant)
          410 (session in expired/aborted/consumed terminal state)
          400 InvalidShape (any field missing or wrong size; code not 6 digits)
          429 RateLimited (per-Clerk-user)
```

Server: validate shape; validate state == `pending`; compute `verification_code_hmac = HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || verification_code)`; atomically transition to `verification_pending`, persist `dashboard_public_key`, `ciphertext`, `nonce`, `verification_code_hmac` (the HMAC, **never the plaintext code**), `approved_at_ms`, `clerk_user_id`. Discard the plaintext code from memory after the HMAC compute. Emit `auth.session.approved` audit event (does NOT include the code or HMAC — only `session_id_hash`/prefix per Fix #4).

**Wire-shape note:** the plaintext verification code travels over TLS to a Clerk-authenticated endpoint, exists momentarily in the API process to compute the HMAC, and is then discarded. This is the standard one-time-password (OTP) verification shape. The alternative (dashboard pre-hashes locally and sends the hash) does not work for a pepper-keyed HMAC — only zombied has the pepper.

**`POST /v1/auth/sessions/{id}/verify`** (no auth — the verification code IS the auth):

```
Request:  { "verification_code": "<6-digits>" }
Response: 200 { "dashboard_public_key": "<base64url>",
                "ciphertext": "<base64url>",
                "nonce": "<base64url>" }
Errors:   400 VerificationFailed (wrong code; verification_attempts incremented)
          410 SessionConsumed (consumed-fingerprint mismatch OR consume-window elapsed)
          410 SessionAborted (5 failed attempts OR explicit cancel OR replaced)
          410 SessionExpired
          410 NotApproved (state != verification_pending and not in consume-idempotency window)
          429 RateLimited
```

Server (Lua-scripted single round-trip):

1. Read session blob; fail fast with the appropriate 410 if state is terminal AND outside the consume-idempotency window.
2. **Consume-idempotency check first** (per Fix 1): if state == `consumed` AND `now_ms < consume_payload_expires_at_ms` AND `request_fingerprint == consumed_client_fingerprint` → return 200 with the cached payload (`{dashboard_public_key, ciphertext, nonce}`). Emit `auth.session.consumed_replay` audit event. Done.
3. Otherwise if state == `consumed` → return 410 `SessionConsumed`.
4. If state != `verification_pending` → 410 `NotApproved` / `SessionExpired` / `SessionAborted` per state.
5. Compute `verification_code_hmac = HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || code)`; constant-time compare against stored `verification_code_hmac`. On mismatch: increment `verification_attempts`; if ≥5, transition to `aborted` with `reason="rate_limit_exceeded"`; emit `auth.session.verify_failed`; return 400.
6. On match: emit `auth.session.verified` (with `attempt = verification_attempts + 1`). In the same Redis write, transition `verification_pending → consumed`, persist `consumed_at_ms = now_ms`, `consume_payload_expires_at_ms = now_ms + 60_000`, `consumed_client_fingerprint = sha256(remote_addr || user_agent || session_id)`. Retain `{dashboard_public_key, ciphertext, nonce}` in the blob for the 60s window. Emit `auth.session.consumed`. Return 200 with the payload.
7. Set/refresh Redis EXPIRE on the session key to `max(remaining_session_ttl, 60s)` so the consume-payload survives the original 5-min TTL if necessary.

The verify endpoint MUST be Lua-atomic (single `EVAL` script). Implementing this in two Redis calls (read-then-write) is a race window where two simultaneous correct-code POSTs both succeed; the test `test_verify_atomic_under_concurrent_correct_codes` pins this.

**`DELETE /v1/auth/sessions/{id}`** (Clerk-authenticated; only the session's `clerk_user_id` can delete):

```
Response: 204
Errors:   401 (Clerk JWT missing/invalid)
          403 (Clerk user doesn't match clerk_user_id)
          404 (no such session)
```

Server: transition to `aborted` with `reason="explicit_cancel"`. Used by the dashboard's "cancel this login" button and by the future sessions surface.

**`DELETE /v1/auth/sessions/all`** (D32 — Clerk-authenticated):

```
Response: 204 { "aborted_count": <n> }
```

Server: enumerate sessions with this `clerk_user_id` in `pending` or `verification_pending`, transition each to `aborted` with `reason="explicit_cancel"`. **Note: this does NOT revoke any already-minted JWTs.** Clerk revocation is a separate problem; an active CLI continues to work until its JWT expires (~15 min). This endpoint clears *in-flight login sessions* only.

The dashboard "abort all pending logins" button calls this. The CLI command surface deliberately does NOT expose this as `zombiectl logout --all` because that name implies revocation of active credentials — see *Logout command rename (Fix 5 from ChatGPT review)* below.

**Server-side invariant: every successful login transitions through `pending → verification_pending → consumed`**, never `pending → consumed`. Enforced by the handler's state-machine check; tested by `test_no_pending_to_consumed_transition`.

**No `PATCH /v1/auth/sessions/{id}` (plaintext shape) ships in this milestone.** Per Q3, the legacy plaintext-token PATCH operation that exists in `public/openapi.json` today is deleted entirely from `authentication.yaml` and from `src/http/handlers/auth/sessions.zig`. No deprecation shim, no Sunset header, no `UpgradeRequired` response. Any client hitting the old path receives the standard router 404 `route not found`. This is RULE NLG-perfect because the operation was not in production traffic.

### §2 — CLI login handler (Effect-TS or async)

New login handler that:

1. Generates an ECDH P-256 keypair via `crypto.subtle.generateKey` (Node.js ≥20 standard library).
2. Determines the default `token_name` via `defaultTokenName()` (platform family — not hostname); allows override via `--token-name <label>`.
3. **D20 idempotency check:** detects an existing local credential. If present, prompts "You're already logged in as **{previous_token_name}**. Replace?" (Y/n). `--force` skips. `--no-input` aborts unless `--force` is set.
4. **D25 argv leak warning:** if `process.argv` contains `--token <value>`, emit stderr warning and recommend `ZMB_TOKEN` env-var path. Token still honored (this is a guardrail, not a block); login continues.
5. POSTs `{ public_key, token_name }` to `/v1/auth/sessions`; receives `session_id`.
6. Prints the URL to the user: `https://app.usezombie.com/cli-auth/{session_id}` (no public_key in URL).
7. Starts the poll loop:
   - **D22 countdown:** updates the spinner line per tick with `Session expires in MM:SS — open the link in your browser…`. Switches to `Session expires in 0:0X — finish login soon` when ≤10s remain. Suppressed in `--no-input`.
   - **D29 exp-backoff:** initial delay 1s, ×1.5 per attempt up to 5s cap, ±20% jitter. Honors `Retry-After` header from 429 responses.
   - **D30 single-blip tolerance:** one transient failure per poll loop is recoverable; logged via analytics `attempt` callback; continues. A second transient counts as `NetworkError`.
8. On `status=verification_pending`, prompt the user for the 6-digit verification code displayed on the dashboard. `--no-input` mode aborts here with a clear "verification code required; re-run interactively" error.
9. POST `/v1/auth/sessions/{id}/verify` with the entered code. Wrong code → `VerificationFailedError` with one retry offered (interactive only); second wrong code aborts with the same error. Server-side 410 `SessionAborted` after 5 attempts propagates as `SessionAbortedError`.
10. On success response, derive shared secret = `cli_priv × dashboard_public_key`; HKDF-SHA256 (info `"m74-002-v1"`) → 32-byte AES-256-GCM key; decrypt the ciphertext + nonce to recover the JWT. Decrypt failure → `DecryptError`.
11. Persist `{ token, token_name }` to `credentials.json` (mode 0o600).
12. **D24 /me ping:** call `GET /v1/me` with the new token. Failure → `MeValidationError` with "token saved but failed validation — try `zombiectl login` again." Distinct exit code from `VerificationFailedError`.
13. **D23 fail-loud hydration:** call `hydrateWorkspacesAfterLogin`. On failure, emit single-line stderr warning `warn: post-login workspace hydration failed (<err.code or "network">) — run "zombiectl workspace list" to retry`. Exit code stays 0.
14. Emit success: `logged in as {token_name}`. Existing exit-code pin tests continue to pass.

Memory hygiene per the *Memory hygiene* section above.

### §3 — Dashboard handler

`/cli-auth/{session_id}` is reachable only with the `session_id` path parameter. The page:

1. Validates the Clerk session (existing `clerkMiddleware()` redirect-to-sign-in if missing).
2. GETs `/v1/auth/sessions/{id}` to fetch `cli_public_key` and `token_name`. If `status` is already `verification_pending` / `consumed` / `expired` / `aborted`, surface "this session is no longer pending approval" and stop (do not re-PATCH).
3. Renders "Approve CLI login for **{token_name}**?" with Approve / Cancel buttons.
4. If the active Clerk user has another session in `pending` or `verification_pending` for a different `cli_public_key`, surfaces "This will replace your previous CLI session on **{previous_token_name}**" alongside Approve. Approve in this case calls `DELETE /v1/auth/sessions/{previous_id}` first (transitions previous to `aborted` with `reason="replaced"`).
5. On Approve:
   - Mint Clerk JWT via `getToken({template: "api"})`.
   - Generate `(dash_priv, dash_pub)` via `crypto.subtle.generateKey`.
   - Derive shared = `crypto.subtle.deriveBits({name:"ECDH", public: cli_public_key}, dash_priv, 256)`.
   - Pass through `crypto.subtle.deriveKey({name:"HKDF", info: utf8("m74-002-v1"), salt: empty, hash:"SHA-256"}, sharedSecret, {name:"AES-GCM", length:256}, false, ["encrypt"])`.
   - Generate 96-bit nonce via `crypto.getRandomValues(new Uint8Array(12))`.
   - Encrypt JWT: `crypto.subtle.encrypt({name:"AES-GCM", iv: nonce}, key, utf8(jwt))`.
   - Generate 6-digit code via `crypto.getRandomValues(new Uint32Array(1))` mod 1_000_000, zero-padded to 6 digits.
   - PATCH `/v1/auth/sessions/{id}/approve` with `{dashboard_public_key, ciphertext, nonce, verification_code}` — **plaintext code over TLS to a Clerk-authenticated endpoint**. The API server computes the keyed HMAC server-side using the Vault-loaded pepper; the dashboard never sees the pepper and does NOT hash locally. (Earlier revisions had the dashboard hash with sha256-and-session_id-salt; Fix #1 from ChatGPT review 3 moved hashing server-side because only the API has the pepper.)
6. On 200, display the plaintext verification code prominently with the prose "Type this 6-digit code into your CLI window to complete login." Provide a Copy button (low-friction; the user can paste into the CLI prompt).
7. On 409 / 410, surface a clear "this session is no longer accepting approval" message.

Memory hygiene: discard `dash_priv` and shared key after PATCH /approve resolves. Never persist crypto material.

### §4 — Error taxonomy (`AUTH_PRESET` + Effect-TS error variants)

Extend `zombiectl/src/lib/error-map-presets.ts`. `AUTH_PRESET` gains:

| Internal trigger | Public code | User prose |
|---|---|---|
| 404 from server polling unknown session | `InvalidSession` | "Login session not recognized — start over with `zombiectl login`." |
| 410 `{status: expired}` from server | `ExpiredSession` | "Login session expired. Start over with `zombiectl login`." |
| `fetch` throws `TypeError` / `ECONNREFUSED` / `ENOTFOUND` / `ETIMEDOUT` (after blip budget) | `NetworkError` | "Can't reach the server. Check connection and retry." |
| 429 from any auth endpoint | `RateLimited` | "Server rate-limited the login flow. Backing off — this is transient." |
| Client-side `timeoutSec` exhausted | `Timeout` | "Login took too long. Start over with `zombiectl login`." |
| SIGINT during poll or prompt | `Interrupted` | "Login cancelled." |
| 400 from POST /verify (wrong code) | `VerificationFailed` | "Verification code didn't match. Check the code shown in your browser and try again." |
| AES-GCM decrypt throws | `DecryptError` | "Session integrity check failed — try `zombiectl login` again." |
| 410 `{status: aborted}` from server (5 failed attempts or replaced) | `SessionAborted` | "Login session aborted (too many wrong codes, or replaced by a newer session). Start over." |
| 410 `{status: consumed}` from server | `SessionConsumed` | "Login session already consumed. Start over with `zombiectl login`." |
| GET /v1/me fails after token write | `MeValidation` | "Token saved but failed validation — try `zombiectl login` again." |

Effect-TS error variants (per Q7): one tagged-class per row above, in `zombiectl/src/errors/auth.ts`. The dispatcher formatter must be exhaustive; missing a case fails compile-time per M74_001 §1. **No `UpgradeRequiredError` variant** — the deprecated PATCH path does not ship (Q3 directive).

### §5 — CLI command surface additions

| Dim | Surface | Behaviour |
|---|---|---|
| D20 | `zombiectl login` (idempotency check) | Detects existing credential; prompts to replace; `--force` skips; `--no-input` aborts without `--force`. |
| D21 | `zombiectl login --token-name <label>` | Override the platform-default label. Persisted to `credentials.json` and sent on POST /sessions. |
| D24 | `zombiectl login` (post-write /me ping) | Validates the token actually works against the API before reporting success. Fails fast on mint-time mismatches. |
| D25 | `zombiectl <any> --token <value>` | Stderr warning: "warning: --token leaks into shell history and process lists; prefer `ZMB_TOKEN`/`ZOMBIE_TOKEN`." Token still honored. |
| D26 | Auth-token resolution | TTY: `ZMB_TOKEN` > `ZOMBIE_TOKEN` > `credentials.json`. Non-TTY (scripted): `credentials.json` > `ZMB_TOKEN` > `ZOMBIE_TOKEN`. Pinned in `program/auth-token.ts`. Resolution is **shape-agnostic** — `ZMB_TOKEN` may hold either a Clerk JWT or a `zmb_t_` tenant API key; the Bearer middleware routes by prefix and accepts both. |
| D26b | `zombiectl login` env-var awareness | Before kicking off the browser flow, detect whether `ZMB_TOKEN` or `ZOMBIE_TOKEN` is set in the environment. If set, emit a clear notice: *"`ZMB_TOKEN` is set in your environment — on interactive shells it takes precedence over `credentials.json`. `zombiectl login` only replaces `credentials.json`; your `ZMB_TOKEN` is unaffected."* Then prompt to continue or abort (Y/n). `--force` skips the prompt; `--no-input` aborts unless `--force` is set. **This is a UX guardrail, not a security control** — env vars are out-of-band and the login flow cannot mutate them. Pins the documented "the login flow does not touch env-var-set tokens" contract from the `zmb_t_` coexistence section. |
| D32 | `zombiectl logout` (renamed semantics) | **Replaces `zombiectl logout --all` to avoid misleading framing** (ChatGPT review Fix 5 — "users interpret `logout --all` as `invalidate credentials`, not `delete pending sessions`"). Two explicit commands replace it: (1) `zombiectl logout` deletes the local `credentials.json` and aborts the operator's own *in-flight pending login sessions* via `DELETE /v1/auth/sessions/all`. Help text: *"Removes local credentials and aborts any in-flight `zombiectl login` flows you have pending. Does NOT revoke your active session token — JWTs are short-lived and expire on their own (~15 min). Does NOT touch `ZMB_TOKEN`/`ZOMBIE_TOKEN` environment variables."* (2) `zombiectl auth status --json` documents that JWT revocation is not available client-side. Future spec (M75_xxx or a Clerk admin-API spec) lands real revocation. **Until then, the misleading `--all` flag is rejected with a clear error** pointing to the new help text. RULE NLG: the flag is removed in entirety (no `legacy_*` alias), not silently aliased — pre-v2.0.0. |

### §6 — Login UX hardening (carried forward from M71_001 P2 §1-§5)

| Dim | Surface | Behaviour |
|---|---|---|
| D22 | `pollUntilComplete` (per-tick countdown) | Per-poll update of the in-place spinner line to `Session expires in MM:SS — open the link in your browser…`; switches to `Session expires in 0:0X — finish login soon` at ≤10s. Suppressed in `--no-input`. Deadline computed from `expires_at_ms` returned by GET /sessions. |
| D23 | `persistAndHydrate` (fail-loud hydration) | Narrow catch around `hydrateWorkspacesAfterLogin`; on failure emit single-line stderr `warn: post-login workspace hydration failed (<err.code or "network">) — run "zombiectl workspace list" to retry`. Exit code stays 0 — the existing pin tests in `test/login.unit.test.ts` are binding. |
| D28 | `AUTH_PRESET` six-code split | Per §4 table above. JSON-mode callers and the acceptance suite assert on the taxonomy. Unmapped errors continue to surface as the generic auth fallback (backwards compat). |
| D29 | Poll loop exp-backoff with jitter | Start at 1s (`pollMs` default), grow ×1.5 per attempt up to 5s cap, ±20% jitter per tick. Honor server `Retry-After` (seconds or HTTP-date) on 429 responses — that beats local backoff. Use the existing `backoffDelay()` helper from `src/lib/http.ts` (RULE UFS — one named exp-backoff helper per package). |
| D30 | Single-blip tolerance | Carry `transientCount` in the `pollUntilComplete` local state. One transient failure per poll loop is logged via the analytics `attempt` callback and recoverable; the second counts as `NetworkError`. The 1-blip budget is intentionally conservative; bigger budgets mask real outages. |

### §7 — AUTH.md update

`docs/AUTH.md` gains:

1. **Security properties by layer** table from above (durable in AUTH.md so future readers see the crypto context together).
2. **Updated Flow 1 sequence diagram** with the new endpoints, state machine, and ECDH path.
3. **What the dashboard sees vs what the wire sees** subsection with the boundary diagram.
4. **What this flow does NOT protect against** subsection mirroring this spec's Non-goals (especially endpoint compromise on either side).
5. **Endpoint trust boundaries** table:

   | Endpoint | Trusted actor | Auth |
   |---|---|---|
   | POST /v1/auth/sessions | Unauthenticated CLI | Rate-limited per IP |
   | GET /v1/auth/sessions/{id} | Unauthenticated CLI poll | Rate-limited per session |
   | PATCH /v1/auth/sessions/{id}/approve | Dashboard JS process | Clerk JWT (api template) |
   | POST /v1/auth/sessions/{id}/verify | CLI with verification code | None (the code IS the auth) |
   | DELETE /v1/auth/sessions/{id} | Dashboard JS process | Clerk JWT, must match session's `clerk_user_id` |
   | DELETE /v1/auth/sessions/all | Dashboard JS process | Clerk JWT |

6. **Replay semantics** subsection covering single-use code, single-read ciphertext, 60s same-fingerprint replay window for consume-idempotency, atomic consume, monotonic state machine.
7. **Autonomous-agent redirect**: short paragraph stating M74_002 is human-mediated; agents go to M75_xxx.
8. **Sensitive-data classification table** including the new row: `session_id` = "ephemeral capability, treat as password-reset token. Appears only in the primary CLI-generated verification URL and the API route paths that consume it; never in logs at info/warn/error, never in analytics or metrics labels, never in secondary URLs, never in error bodies routed to non-trusted surfaces."
9. **High-risk future hardening areas** subsection listing dashboard-JS-compromise (XSS / extension / supply-chain), real JWT revocation, and the verification-code entropy uplift. Each entry names the responsible future milestone (or "spec not yet authored") so future security-review passes find them without re-reasoning.
10. **Deployment requirements** subsection: Redis required for the session store; HTTPS-only at the load balancer; HSTS header on every response; NTP-synced clocks across pods within ≤1s drift; `AUTH_SESSION_CODE_PEPPER` and `AUDIT_LOG_PEPPER` provisioned in Vault; `.auth_audit` sink routed to a restricted destination distinct from customer-visible logs.
11. **Human-led-only invariant** (Fix #5): explicit prose mirroring the autonomous-agents section's hard rule — M74_002 is for human-led flows only; CI / cron / K8s / hosted-agent / scheduled use is a spec violation; redirect to M75_xxx.

---

## Interfaces

See §1 for wire shapes. Locked contracts:

- `POST /v1/auth/sessions` request body shape — additive (`public_key`, `token_name` required; old binaries without them fall through to the deprecated PATCH path).
- `GET /v1/auth/sessions/{id}` response shape — narrows: only `status` + `cli_public_key` + `token_name` + `expires_at_ms` ever returned. **Never** `ciphertext` / `nonce` / `dashboard_public_key` / `verification_code*`. (Invariant 1.)
- `PATCH /v1/auth/sessions/{id}/approve` (NEW) — request shape per §1.
- `POST /v1/auth/sessions/{id}/verify` (NEW) — request + response shapes per §1.
- `DELETE /v1/auth/sessions/{id}` (NEW) — Clerk-authenticated.
- `DELETE /v1/auth/sessions/all` (NEW) — Clerk-authenticated; D32.
- Deprecated `PATCH /v1/auth/sessions/{id}` (plaintext shape) — **DELETED entirely from this milestone per Q3**. The operation that exists in `public/openapi.json` today is removed from the YAML sources and the Zig handler. Any client hitting the old path gets a router 404. No deprecation header, no Sunset header, no `UpgradeRequired` response.
- `zombiectl auth status --json` output gains `token_name` field (additive).
- `credentials.json` schema gains `token_name` (non-optional; defaulted by login).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Wrong verification code (≤4 attempts) | User mistypes or attacker has wrong code | Server 400 `VerificationFailed`; client offers one retry (interactive only); second wrong code aborts. |
| Wrong verification code (5th attempt) | Attempt cap | Server transitions session to `aborted` with `reason="rate_limit_exceeded"`; subsequent verify returns 410 `SessionAborted`. CLI surfaces `SessionAbortedError`. |
| Decryption failure | Ciphertext tampered with mid-flight or key mismatch | `DecryptError` → exit code distinct from `VerificationFailedError`. Surfaces "Session integrity check failed — try `zombiectl login` again." Memory hygiene: discard buffers. |
| Old `zombiectl` binary with PATCH plaintext | Pre-M74_002 binary in the wild hits the new handler | **Router 404 — no compat shim ships (Q3 directive).** The plaintext PATCH operation never existed in production traffic; treating any client that targets it as a 404 is honest. Dev/staging engineers running stale builds upgrade their local `zombiectl`. |
| Browser closed mid-flow | User closes the tab before Approve | CLI poll's D22 countdown reaches zero, transitions to `Timeout`. |
| Already-logged-in user runs `zombiectl login` | D20 detection fires | CLI prompts "Replace existing session for **{token_name}**?" unless `--force`. Dashboard surfaces same on the Approve screen via the replace-detection. |
| Clerk session no longer valid when dashboard mints | User's dashboard session expired between page load and Approve click | Standard Clerk re-auth flow on dashboard; CLI poll times out and surfaces "Session expired." |
| Server returns 503 once during poll | Transient server / network blip | D30 single-blip tolerance: log via analytics `attempt`, continue. |
| Server returns 503 twice in same poll | Sustained outage | D28 propagates as `NetworkError`; exit 1. |
| Server returns 429 with `Retry-After: N` | Rate limit | D29 honors `Retry-After`; next attempt counts as fresh; AUTH_PRESET `RateLimited`. |
| SIGINT mid-poll or mid-prompt | Operator hit Ctrl-C | AUTH_PRESET `Interrupted`; exit 130 (POSIX); existing handler. |
| Wall-clock `timeoutSec` exhausted | User stalled in browser past `--timeout` | AUTH_PRESET `Timeout`; exit 1; clear the D22 countdown line. |
| `hydrateWorkspacesAfterLogin` rejects | Server unreachable post-login, or workspace endpoint 401 | D23 stderr warning; exit code stays 0; `credentials.json` was already saved. |
| `/me` ping fails after token write | Token mint-time mismatch or transient server | `MeValidationError`; exit 1 distinct from `VerificationFailedError`; `credentials.json` is deleted to avoid leaving a half-valid local state. |
| `--no-input` + verification code needed | Scripted invocation with no human to type code | Clear error "verification code required; re-run interactively or pre-authenticate via `ZMB_TOKEN`." Exit 1. |
| Concurrent dashboard PATCH /approve | Browser tab refreshed and re-Approve clicked | Server returns 409 Conflict; dashboard surfaces "this session is already approved; check your terminal." |
| Replace flow: previous session in flight on another machine | User logs in fresh while another `zombiectl login` is mid-poll on another machine | Previous session transitions to `aborted` with `reason="replaced"`; the polling CLI surfaces `SessionAbortedError` with that reason. |
| `--token-name` value too long / control chars | Bad user input | 400 `InvalidTokenName` from server; CLI surfaces "token name must be ≤64 chars and printable ASCII." |
| Argv leak warning + `--token` value | D25 path | Stderr warning; token still honored; recommend env-var path. |
| `Date.now()` clock skew during D22 countdown | Operator's clock jumps | Countdown shows the delta vs. session start; if it goes negative, prose flips to `Session expires in 0:00`. Real timeout is server-side; client display is informational. |

---

## Invariants

1. **GET /v1/auth/sessions/{id} NEVER returns ciphertext, nonce, dashboard_public_key, or verification_code*.** Enforced at the handler in `src/http/handlers/auth/sessions.zig`. Tested by `test_get_response_shape_never_carries_ciphertext`.
2. **The server never persists the plaintext verification code.** Enforced by inspection of `SessionState` (no `verification_code` field, only `verification_code_hmac`). The plaintext code arrives on PATCH /approve, lives long enough to compute the HMAC, then is discarded from memory. Tested by `test_session_row_has_no_plaintext_code` + `test_audit_events_carry_no_plaintext_code`.
3. **No plaintext PATCH operation exists in the compiled OpenAPI or in the Zig handler.** Per Q3, the legacy plaintext-token PATCH shape is deleted in entirety. Tested by `test_legacy_plaintext_patch_returns_404` and `test_openapi_has_no_legacy_patch_operation`.
4. **POST /verify is atomic: ciphertext release and transition to `consumed` happen in one write.** Enforced by handler transaction; tested by `test_verify_atomic_consume_blocks_replay`.
5. **A verification code is required for every successful login.** Enforced by the CLI handler (no path through the Effect that recovers a JWT without entering the code) AND by the server (POST /verify is the only ciphertext-release endpoint). Tested by `test_no_pending_to_consumed_transition`.
6. **`token_name` is persisted with every minted credential.** Enforced by `credentials.json` schema — `token_name` is non-optional in the persisted shape (defaulted by `defaultTokenName()`).
7. **No TypeScript error variant in this spec is unhandled by the shared dispatcher formatter.** Enforced by TypeScript exhaustiveness on the formatter switch (Q7).
8. **No `as any` / `!` / `@ts-expect-error` introduced** in the diff. Enforced by lint.
9. **State machine is monotonic.** No backward transitions. Enforced by handler; tested by `test_state_machine_monotonic`.
10. **Verify attempts cap is 5 per session.** Enforced by handler; tested by `test_5th_wrong_code_aborts_session`.
11. **`commandLogin` exit codes match `test/login.unit.test.ts` exactly.** Pre-existing pin tests are binding. 0 = success (including hydration-failed per D23), 1 = login failed, 130 = SIGINT.
12. **`AUTH_PRESET` retains every existing key** — §4 only adds keys. Tested by `test_auth_preset_keys_superset`.
13. **An *honest* API server never holds the decryption key.** Enforced by inspection: `cli_priv` never crosses any wire; `dash_priv` is generated in-browser, used once, discarded after PATCH /approve. **An active malicious API server CAN obtain decryption capability** by substituting `cli_public_key` (the textbook unauthenticated-Diffie-Hellman MITM); v2.0 does not close this — see *what this flow does NOT protect against* row 6 and the v2.1 future-improvement entry. Tested by audit at code-review time.
14. **Session storage is Redis-backed.** No in-memory session map exists in the API process post-rewrite. Tested by `test_session_store_redis_required` (boot fails without `REDIS_URL`) and `test_redis_sessionstore_survives_pod_restart`.
15. **POST /verify is a single Lua-EVAL atomic operation.** No read-then-write Redis sequences anywhere on the verify path. Tested by `test_verify_atomic_under_concurrent_correct_codes`.
16. **`session_id` never appears in the `.auth` log scope at info/warn/error level.** Only `.auth_audit` scope and `.auth` debug/trace (redacted to 8-hex-prefix) contain the ID. Tested by `test_session_id_never_logged_unredacted`.
17. **All `/v1/auth/*` responses carry the HSTS header.** Tested by `test_hsts_header_present_on_auth_responses`.
18. **Server-side expiry decisions use a 30-second grace window over `expires_at_ms`.** Client `expires_at_ms` is informational only and never gates security. Tested by `test_server_grace_window_30s` and `test_client_countdown_does_not_gate_security`.
19. **`zombiectl logout --all` is rejected** (RULE NLG-clean — flag removed in entirety; clear error pointing to the new help text). Tested by `test_logout_command_rejects_all_flag`.
20. **M74_002 is human-led only.** Spec-text invariant per Fix #5: a human MUST be present at flow time to complete browser approval AND terminal verification. Unattended use (CI, cron, K8s, hosted agents, scheduled background) is a spec violation and must redirect to M75_xxx Agent Identity. This is enforced by documentation discipline, not by code. AUTH.md carries the same hard wording.

---

## Test Specification

### Server-side (Zig + integration)

| Test | Asserts |
|------|---------|
| `test_post_sessions_returns_session_id` | Happy POST returns 201 with a UUIDv7 and stores `cli_public_key` + `token_name`. |
| `test_post_sessions_rate_limit_per_ip` | 11th POST from same IP within a minute returns 429 with `Retry-After`. |
| `test_post_sessions_rejects_invalid_pubkey` | Malformed base64url / wrong curve / wrong length → 400 `InvalidPublicKey`. |
| `test_post_sessions_rejects_long_token_name` | >64 chars or control chars → 400 `InvalidTokenName`. |
| `test_get_response_shape_never_carries_ciphertext` | After PATCH /approve, GET response keys are exactly `{status, cli_public_key, token_name, expires_at_ms}`. Pins Invariant 1. |
| `test_get_returns_pending_then_verification_pending` | State transitions visible to the CLI poll. |
| `test_get_returns_410_on_terminal_states` | `expired` / `aborted` / `consumed` each return 410 with status + reason. |
| `test_patch_approve_happy_path` | Valid request transitions `pending → verification_pending`, persists all four fields + `approved_at_ms` + `clerk_user_id`, emits `auth.session.approved` audit. |
| `test_patch_approve_requires_clerk_jwt` | Missing/invalid Authorization → 401. |
| `test_patch_approve_409_on_double_call` | PATCH against already-`verification_pending` session → 409 Conflict. |
| `test_patch_approve_410_on_terminal_state` | Session in expired/aborted/consumed → 410. |
| `test_patch_approve_rejects_short_nonce` | Nonce not exactly 12 bytes → 400. |
| `test_patch_approve_per_user_rate_limit` | 21st approve from same Clerk user in an hour → 429. |
| `test_verify_atomic_consume_blocks_replay` | First correct POST /verify returns ciphertext + transitions to `consumed` in one Lua-scripted write. Second POST /verify from a **different** fingerprint → 410 `SessionConsumed`. Pins Invariant 4. |
| `test_verify_idempotent_replay_within_window` | First correct POST /verify returns ciphertext. Second POST /verify within 60s **from the same fingerprint** returns the same payload (200). `auth.session.consumed_replay` audit event fired. Pins Fix 1. |
| `test_verify_replay_rejected_outside_window` | Wait 61s after first successful POST /verify; second call from same fingerprint → 410 `SessionConsumed` (payload wiped). |
| `test_verify_replay_rejected_from_different_fingerprint` | First correct POST /verify; second call within 60s from different IP/User-Agent → 410 `SessionConsumed`. |
| `test_verify_atomic_under_concurrent_correct_codes` | Fire 2 simultaneous POST /verify with the same correct code from same source; exactly one returns 200, the other returns 200 too (idempotent replay) — but the session is in `consumed` state once, not twice. Lua-EVAL atomicity pinned. |
| `test_redis_sessionstore_survives_pod_restart` | Create session on pod A; restart pod A; pod B picks up the session from Redis and serves the GET /poll correctly. |
| `test_redis_sessionstore_concurrent_pods` | Approve on pod A; poll on pod B; verify on pod C — full flow works across all three. |
| `test_session_id_never_logged_unredacted` | Capture all `.auth`-scope log output during a full happy-path flow; assert no log line contains a 36-char UUIDv7 hex sequence matching the live session_id. The `.auth_audit` scope is allowed; `.auth` is not. Pins Fix 4. |
| `test_audit_event_shape_session_created` | After POST /sessions, the `.auth_audit` log sink captures one JSON line with fields `{event, ts, session_id_hash, session_id_prefix, token_name, ip, user_agent, request_id}`. **No raw `session_id` field** in default mode. Pins Fix 3 + Fix 4. |
| `test_audit_event_shape_session_verify_failed` | After a wrong code POST, capture one audit line with `{event:"auth.session.verify_failed", session_id_hash, session_id_prefix, attempt, ip, user_agent, reason:"invalid_code", request_id}`. No plaintext code, no HMAC value of code. |
| `test_audit_event_shape_session_consumed_replay` | After replay-within-window, capture one audit line with `{event:"auth.session.consumed_replay", session_id_hash, session_id_prefix, consumed_client_fingerprint, replay_within_ms, request_id}`. |
| `test_audit_session_id_hash_correlation` | Two events for the same session (created + approved) produce identical `session_id_hash` values; events for different sessions produce different hashes. Confirms cross-event correlation works through the pseudonymization layer. |
| `test_audit_incident_mode_warns_on_boot` | Boot with `AUTH_AUDIT_INCLUDE_FULL_IDS=true` → `.auth` scope emits a WARN-level startup line `"AUTH_AUDIT_INCLUDE_FULL_IDS=true …"`. Audit events then carry the raw `session_id` field. |
| `test_audit_events_carry_no_plaintext_code` | Full happy-path + sad-path log capture; assert no audit event JSON contains the plaintext 6-digit code. |
| `test_audit_log_pepper_required_on_boot` | Boot zombied without `AUDIT_LOG_PEPPER` → fail fast (same shape as `AUTH_SESSION_CODE_PEPPER` + `REDIS_URL` requirements). |
| `test_server_grace_window_30s` | At `expires_at_ms + 15_000`, GET /poll returns `verification_pending` (within grace). At `expires_at_ms + 31_000`, GET /poll returns `expired`. Pins Fix 5. |
| `test_client_countdown_does_not_gate_security` | Set CLI clock 60s ahead of server; CLI countdown displays "0:00 expiring now" but actual GET /poll returns `verification_pending` (server-authoritative, within grace). User can still complete login. |
| `test_session_store_redis_required` | Boot API process with no `REDIS_URL` configured → fail-fast on startup with clear error pointing to deploy README. No fallback to in-memory. Pins Fix 2. |
| `test_session_gc_redis_ttl_evicts` | Create session; wait 5 min 30 s (TTL + buffer); assert `EXISTS auth:session:{id}` returns 0. |
| `test_session_gc_secondary_sweep` | Inject a session blob with TTL accidentally cleared; run the background sweep; assert the blob is pruned and a metric increment fires. |
| `test_hsts_header_present_on_auth_responses` | Every response to `/v1/auth/*` includes `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload`. Pins Fix 6. |
| `test_logout_command_rejects_all_flag` | `zombiectl logout --all` returns exit code 1 with a clear message pointing to the new help text; no API call made. RULE NLG: the flag is rejected, not silently aliased. |
| `test_logout_clears_local_and_aborts_pending` | `zombiectl logout` deletes `credentials.json` AND calls `DELETE /v1/auth/sessions/all` to abort the operator's in-flight pending sessions. Does NOT attempt JWT revocation (no Clerk admin API call). |
| `test_verify_wrong_code_increments_attempts` | 4 wrong attempts increment counter; session stays in `verification_pending`; 4 responses are 400 `VerificationFailed`. |
| `test_5th_wrong_code_aborts_session` | 5th wrong code → session → `aborted` with `reason="rate_limit_exceeded"`; subsequent verify → 410 `SessionAborted`. Pins Invariant 10. |
| `test_verify_constant_time_compare` | Side-channel resistance: compare uses `crypto.timingSafeEqual` (audit-level assertion + grep). |
| `test_verify_410_on_not_approved` | POST /verify against `pending` session (no PATCH yet) → 410 `NotApproved`. |
| `test_session_row_has_no_plaintext_code` | After PATCH /approve, inspect session row; assert no field equals the original plaintext code (only the hash). Pins Invariant 2. |
| `test_session_row_never_holds_plaintext_jwt` | Inspect session store after PATCH /approve; assert no field equals the JWT plaintext. Pins Attack B's protection. |
| `test_session_expiry_after_5_min` | Session created at T0; GET at T+5min+1s → 410 `expired`. |
| `test_delete_session_aborts` | DELETE → state → `aborted` with `reason="explicit_cancel"`. |
| `test_delete_session_requires_owning_user` | Different Clerk user → 403. |
| `test_delete_sessions_all_aborts_user_sessions` | DELETE /all → all pending/verification_pending sessions for this user → `aborted`. |
| `test_no_pending_to_consumed_transition` | State-machine audit — there is no codepath that transitions `pending → consumed` directly. Pins Invariant 5. |
| `test_state_machine_monotonic` | Construct sessions in each non-terminal state; assert every mutation either stays-or-progresses (no backward edges). Pins Invariant 9. |
| `test_legacy_plaintext_patch_returns_404` | `PATCH /v1/auth/sessions/{id}` with a plaintext `{status, token}` body → HTTP 404 from the router (no handler exists for this shape). Confirms Q3 directive. |
| `test_openapi_has_no_legacy_patch_operation` | `jq` against `public/openapi.json` confirms `paths["/v1/auth/sessions/{session_id}"].patch` is absent. The only PATCH under `/v1/auth/sessions/*` is `.../approve`. |

### CLI-side (TypeScript + zombiectl)

| Test | Asserts |
|------|---------|
| `test_ecdh_round_trip_unit` | CLI keypair + dashboard keypair → derive shared secret on both sides → AES-256-GCM encrypt-decrypt of a JWT round-trips byte-identically. |
| `test_hmac_verification_code_unit` (Zig) | `hmacVerificationCode(pepper, "session-id-x", "123456")` deterministic; identical pepper + session_id + code produce identical 32-byte HMAC output; different pepper produces different HMAC (validates the pepper is actually keyed in). |
| `test_session_pepper_required_on_boot` | Boot zombied without `AUTH_SESSION_CODE_PEPPER` env → fail fast with clear error. Same fail-fast shape as `test_session_store_redis_required`. |
| `test_session_pepper_loaded_from_vault` | Boot zombied with Vault-mocked `op://ops/ZMB_LOCAL_DEV/AUTH_SESSION_CODE_PEPPER` → process loads it, computes a known-vector HMAC matching expected output. |
| `test_login_happy_path_e2e` | Full flow: `zombiectl login` → browser Approve → user types verification code → credential persisted with `token_name` set + /me ping succeeds. |
| `test_login_wrong_verification_code` | User enters wrong code → handler fails with `VerificationFailedError` + documented exit code; no credential persisted. Server-side: matching Attack A path is blocked. |
| `test_login_5_wrong_codes_surfaces_session_aborted` | 5 wrong attempts → server aborts → CLI surfaces `SessionAbortedError`. |
| `test_login_expired_session` | Session expires before Approve → CLI surfaces "Session expired" + exits without persisting. |
| `test_login_already_authenticated_prompts` (D20) | Existing credential present → CLI prompts to replace; `--force` skips. |
| `test_login_already_authenticated_dashboard_surfaces` | Existing session for the same user → dashboard shows "Replacing previous session on **{token_name}**." |
| `test_login_token_name_default` (D21) | No `--token-name` → credential's `token_name` ∈ {`macos-cli`, `linux-cli`, `windows-cli`, `freebsd-cli`} based on `process.platform`. Never the hostname. |
| `test_login_token_name_override` (D21) | `--token-name production-laptop` → credential's `token_name === "production-laptop"`. |
| `test_login_me_ping_failure` (D24) | Login completes but `/me` returns 401 → `MeValidationError`; `credentials.json` deleted; exit code distinct from `VerificationFailedError`. |
| `test_argv_redact_warns_on_token_flag` (D25) | Invoking `zombiectl <cmd> --token <value>` emits stderr line matching `/warning: --token leaks/`. Token still honored. |
| `test_auth_token_resolution_tty_priority` (D26) | TTY-mocked: `ZMB_TOKEN` set → that wins over `credentials.json`. |
| `test_auth_token_resolution_non_tty_priority` (D26) | Non-TTY: `credentials.json` wins over `ZMB_TOKEN`. |
| `test_auth_token_resolution_accepts_zmb_t_prefix` (D26) | `ZMB_TOKEN=zmb_t_<hex>` (tenant API key) flows through as Bearer transparently; downstream calls succeed without going through `zombiectl login`. |
| `test_login_warns_when_env_token_set` (D26b) | `ZMB_TOKEN` set in environment → `zombiectl login` emits the env-var-awareness notice on stderr; prompt requires Y to continue; `--force` skips. |
| `test_login_aborts_in_no_input_when_env_token_set` (D26b) | `--no-input` + `ZMB_TOKEN` set + no `--force` → login aborts with exit code 1 and a clear "env-var token detected; pass `--force` to override" message. |
| `test_logout_all_does_not_unset_env_token` (D32) | `zombiectl logout --all` with `ZMB_TOKEN` set → `credentials.json` deleted, server sessions aborted, but `process.env.ZMB_TOKEN` is unchanged (the CLI cannot mutate the parent shell anyway; assertion documents intent). |
| `test_logout_all_calls_server_then_deletes_local` (D32) | `zombiectl logout --all` calls `DELETE /v1/auth/sessions/all` then unlinks `credentials.json`. |
| `test_d22_countdown_ticks_per_poll` | Per-poll write to stdout matches `/Session expires in \d+:\d{2}/`; transitions to single-digit second prose at `< 10s`. |
| `test_d22_countdown_suppressed_in_no_input` | `--no-input` produces zero countdown writes; only the final success line. |
| `test_d23_hydration_failure_emits_stderr_warning_exit_0` | When `hydrateWorkspacesAfterLogin` rejects, stderr matches `/warn: post-login workspace hydration failed/`; exit code is 0; `credentials.json` exists with 0o600 mode. |
| `test_d23_hydration_success_emits_no_warning` | Happy path — no `warn:` line on stderr. |
| `test_d28_invalid_session_maps_to_InvalidSession` | 404 from server mid-poll → JSON envelope `error.code === "InvalidSession"`. |
| `test_d28_expired_session_maps_to_ExpiredSession` | 410 expired → `"ExpiredSession"`. |
| `test_d28_network_error_maps_to_NetworkError` | `fetch` throws → `"NetworkError"` (after blip budget). |
| `test_d28_rate_limited_maps_to_RateLimited` | 429 → `"RateLimited"`. |
| `test_d28_timeout_maps_to_Timeout` | `timeoutSec` exhausted → `"Timeout"`. |
| `test_d28_interrupted_maps_to_Interrupted` | SIGINT propagated → `"Interrupted"`, exit 130. |
| `test_d28_unknown_error_falls_through_to_generic` | Unmapped error retains existing fallback prose; backwards compat. |
| `test_d29_first_poll_immediate` | First mock-fetch call fires at t≈0 (no backoff). |
| `test_d29_subsequent_polls_use_exp_backoff_with_jitter` | Inter-poll delays grow ×1.5 up to 5s cap; each delay within ±20% of nominal. |
| `test_d29_retry_after_honored` | 429 with `Retry-After: 3` → next poll fires at t+3s (overrides local backoff). |
| `test_d30_single_503_survives_login_completes` | Inject one 503 mid-poll; login completes; analytics `attempt` callback fires with `attempt: 2`. |
| `test_d30_double_503_surfaces_NetworkError` | Inject two 503s back-to-back; login fails with `NetworkError`. |
| `test_invariant_existing_pin_tests_still_pass` | All pre-existing `test/login.unit.test.ts` rows pass byte-for-byte. |
| `test_invariant_auth_preset_keys_superset` | Exported `AUTH_PRESET` contains every pre-existing key plus the codes from §4. |
| `test_authmd_security_layers_table_present` | `grep -c 'Security properties by layer\|verification code.*authorization binding\|ECDH.*ciphertext-only' docs/AUTH.md` ≥ 3. |
| `test_authmd_autonomous_agent_redirect_present` | `grep -c 'M75_xxx\|autonomous agent' docs/AUTH.md` ≥ 1. |

### UI-side (TypeScript + Playwright)

| Test | Asserts |
|------|---------|
| `test_cli_auth_page_displays_token_name` | `/cli-auth/{id}` after GET shows "Approve CLI login for {token_name}?". |
| `test_cli_auth_page_url_has_no_public_key_param` | Generated verify URL contains only `session_id` in path; no `public_key` query param. |
| `test_cli_auth_approve_patches_correct_shape` | On Approve click, mocked PATCH /approve receives `{dashboard_public_key, ciphertext, nonce, verification_code}` (plaintext code per Fix #1 — API computes HMAC server-side); no `token` field. |
| `test_cli_auth_displays_verification_code_post_approve` | After PATCH success, page shows 6-digit code prominently with "Type this into your CLI" prose. |
| `test_cli_auth_handles_409_gracefully` | Double-PATCH (refresh-and-reapprove) surfaces "this session is already approved" not a raw error. |
| `test_cli_auth_replace_previous_surface` | Previous session for same user → "This will replace your previous CLI session on {previous_token_name}" copy renders alongside Approve. |
| `lifecycle-after-login.spec.ts` (Playwright extension) | Full e2e: zombiectl login + dashboard Approve + verification-code entry → exit 0 + token_name visible in `zombiectl auth status`. **Uses `op://$VAULT_DEV/e2e-fixture-email/regular`** loaded at suite setup. |
| `test_flow2_signin_unchanged` (Playwright regression) | Existing dashboard sign-in flow (Clerk OAuth round-trip, `__session` cookie, dashboard page render, `useAuth().getToken({template:"api"})` mint) works byte-identical to pre-M74_002 baseline. Confirms Flow 2 is untouched. |
| `test_next_config_rewrites_unchanged` | `grep -c '/backend/:path' ui/packages/app/next.config.ts` returns 1; no new rewrite stanzas added; the existing rewrite covers all new auth endpoints transparently. |
| `test_dashboard_uses_admin_fixture_for_clerk_admin_scenarios` | Scenarios requiring admin role (e.g. dashboard "abort all sessions" surface, if it lands) use `op://$VAULT_DEV/e2e-fixture-email/admin`. |

---

## Acceptance Criteria

- [ ] `make test` green (Zig unit + zombiectl + UI + app).
- [ ] `make test-integration` green (DB + Redis + session handler + Clerk-test-mode).
- [ ] `make lint` green.
- [ ] `(cd zombiectl && bun run typecheck && bun run lint)` clean.
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`.
- [ ] Playwright `lifecycle-after-login.spec.ts` passes including all new assertions.
- [ ] `gitleaks detect` clean.
- [ ] `docs/AUTH.md` Flow 1 reflects the new sequence; the Security-properties-by-layer table is present; autonomous-agent redirect to M75_xxx present; endpoint-trust-boundaries table present.
- [ ] No file added over 350 lines (RULE FLL).
- [ ] `grep -nE '"token":' src/http/handlers/auth/sessions.zig | grep -v 'token_name'` returns zero matches in the **new** handler bodies (the deprecated handler retains it until the cut).
- [ ] `grep -c "verification_code_hmac" src/auth/sessions.zig` ≥ 1 AND `grep -nE 'verification_code[^_]' src/auth/sessions.zig` returns zero matches (plaintext code field absent).
- [ ] `AUTH_SESSION_CODE_PEPPER` provisioned in Vault for every environment (`op://ops/ZMB_CD_PROD/AUTH_SESSION_CODE_PEPPER`, `ZMB_CD_DEV`, `ZMB_LOCAL_DEV`); zombied fails fast on boot without it. Documented in deploy README.
- [ ] No plaintext code appears in any audit event (`.auth_audit` scope grep confirms).
- [ ] `git diff origin/main..HEAD -- 'zombiectl/**/*.ts' | grep -E "as any|@ts-expect-error|: !" | wc -l` == 0.
- [ ] All Captain decisions Q1-Q7 reflected in the implementation; spec text matches the pinned answers.
- [ ] `public/openapi/paths/authentication.yaml` describes the new endpoints + state-machine enum; legacy plaintext PATCH operation marked `deprecated: true` (or deleted at the cut milestone).
- [ ] `public/openapi.json` regenerated; `make check-openapi` (or equivalent) clean — no drift between source YAML and compiled JSON.
- [ ] `jq '.paths["/v1/auth/sessions/{session_id}"].get.responses["200"].content."application/json".schema.properties | has("token")' public/openapi.json` returns `false` (GET response no longer carries plaintext `token`).
- [ ] Session storage is Redis-backed; the old in-memory `SessionStore` is deleted in entirety from `src/auth/sessions.zig`. `grep -n 'StringHashMap.*Session\|max_sessions: usize' src/auth/sessions.zig` returns zero matches.
- [ ] Boot the API with `REDIS_URL` unset → process exits with non-zero status + clear error. Tested by `test_session_store_redis_required`.
- [ ] Consume-idempotency window verified end-to-end: replay within 60s from same fingerprint succeeds; outside window OR different fingerprint → 410.
- [ ] Audit events fire with the exact field shapes in the *Audit events* section (`session_id_hash` + `session_id_prefix` in default mode, raw `session_id` only under `AUTH_AUDIT_INCLUDE_FULL_IDS=true`); verified via integration log-capture tests.
- [ ] `AUDIT_LOG_PEPPER` provisioned in Vault for every environment; zombied fails fast on boot without it.
- [ ] No audit event in any test capture contains the plaintext verification code or the `verification_code_hmac` raw bytes.
- [ ] `playbooks/001_bootstrap/001_playbook.md` includes §1.3b (Auth Pepper Keys) + §1.3c (E2E Fixture Email Identities); the §2.0 vault inventory lists `auth-session-code-pepper`, `audit-log-pepper`, `e2e-fixture-email/regular`, `e2e-fixture-email/admin`.
- [ ] `playbooks/002_preflight/001_playbook.md` DEV-vault + PROD-vault tables list the same three new items (e2e-fixture rows only in DEV; peppers in both).
- [ ] `ui/packages/app/next.config.ts` is **unchanged** by this spec (the existing `/backend/:path*` rewrite covers the new endpoints); grep confirms.
- [ ] `ui/packages/app/app/cli-auth/[session_id]/page.tsx` is the only new dashboard route added; no other dashboard pages modified.
- [ ] Flow 2 regression: a smoke test runs against the existing dashboard sign-in / dashboard / sign-out path and confirms zero behavioral change.
- [ ] `TRUSTED_PROXY_IPS` env documented in deploy README; `trusted_client_ip.zig` middleware lands; consume-idempotency fingerprint + per-IP rate-limit buckets confirmed via test to use the derived IP, not the raw TCP peer.
- [ ] Spec wording: `session_id` allowed in the primary `cli-auth/{session_id}` URL and API route paths; forbidden in logs / analytics / metrics / secondary URLs / error bodies. Grep confirms the contradictory "never embedded in user-pasteable URLs" phrasing is gone.
- [ ] `session_id` redaction enforced: a full log capture of a happy-path flow shows zero unredacted session_id occurrences in the `.auth` scope.
- [ ] Server-side expiry uses a 30-second grace window; CLI countdown is informational only.
- [ ] HSTS header present on every `/v1/auth/*` response (via load-balancer or `security_headers.zig` middleware).
- [ ] `zombiectl logout --all` is rejected with a clear error pointing at the renamed semantics; the misleading flag is removed in entirety.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: tests
make test && make test-integration

# E2: lint
make lint && (cd zombiectl && bun run typecheck && bun run lint)

# E3: cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux

# E4: e2e
cd ui/packages/app && bun run test:e2e lifecycle-after-login

# E5: AUTH.md updated
grep -cE 'Security properties by layer|verification code.*authorization binding|ECDH.*ciphertext-only|M75_xxx' docs/AUTH.md

# E6: PATCH /approve handler rejects plaintext token (manual integration check via curl)
curl -X PATCH https://api.usezombie.local/v1/auth/sessions/<id>/approve \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <clerk-jwt>' \
  -d '{"dashboard_public_key":"...","ciphertext":"...","nonce":"...","verification_code":"123456","token":"abc"}'
# expect HTTP 400 (unexpected field) OR field silently ignored — test asserts ciphertext is the only secret carrier

# E7: GET response shape narrow
curl -s https://api.usezombie.local/v1/auth/sessions/<id> | jq 'keys'
# expect ["status","cli_public_key","token_name","expires_at_ms"] only

# E8: gitleaks
gitleaks detect

# E9: session row never holds plaintext JWT or plaintext code
# (introspection from integration test — see test_session_row_never_holds_plaintext_jwt + test_session_row_has_no_plaintext_code)

# E10: AUTH_PRESET completeness
grep -cE "InvalidSession|ExpiredSession|NetworkError|RateLimited|Timeout|Interrupted|VerificationFailed|DecryptError|SessionAborted|SessionConsumed|MeValidation" \
  zombiectl/src/lib/error-map-presets.ts
# expect ≥ 11  (UpgradeRequired removed per Q3)

# E11: argv-leak warning fires
zombiectl auth status --token fake-value 2>&1 | grep -c "warning: --token leaks"
# expect ≥ 1

# E12: token_name default is platform-family, not hostname
zombiectl auth status --json | jq -r '.token_name' | grep -E '^(macos|linux|windows|freebsd)-cli$'
# expect a match (unless user overrode with --token-name)

# E13: HARNESS VERIFY
make harness-verify

# E14: OpenAPI source/compiled drift
make check-openapi && git diff --exit-code public/openapi.json
# expect zero diff (compiled artifact matches sources)

# E15: GET response shape no longer carries plaintext token
jq '.paths["/v1/auth/sessions/{session_id}"].get.responses["200"].content."application/json".schema.properties | has("token")' public/openapi.json
# expect: false

# E16: PATCH /approve operation exists in compiled OpenAPI
jq '.paths["/v1/auth/sessions/{session_id}/approve"].patch.operationId' public/openapi.json
# expect: "approve_auth_session" (or whatever the chosen operationId is)

# E17: legacy plaintext PATCH operation absent (Q3 directive)
jq '.paths["/v1/auth/sessions/{session_id}"].patch' public/openapi.json
# expect: null  (operation deleted entirely; no deprecation shim ships)

# E18: in-memory SessionStore deleted
grep -n 'StringHashMap.*Session\|max_sessions: usize' src/auth/sessions.zig
# expect: zero matches

# E19: API refuses to boot without REDIS_URL
REDIS_URL= ./zig-out/bin/zombied serve 2>&1 | grep -c "REDIS_URL required for session store"
# expect: ≥ 1

# E20: session_id never appears unredacted in the .auth log scope
make test-integration | grep -c 'session_id="[0-9a-f]\{24,\}"'
# expect: 0  (all redacted to `…(len=N)` form)

# E21: HSTS header on auth responses
curl -sI https://api.usezombie.local/v1/auth/sessions -X POST -d '{}' \
  | grep -ci 'strict-transport-security'
# expect: ≥ 1

# E22: logout --all is rejected
zombiectl logout --all 2>&1 | grep -ci "removed; use 'zombiectl logout'"
# expect: ≥ 1

# E23: pepper vault items provisioned (DEV)
op item get auth-session-code-pepper --vault "$VAULT_DEV" --fields credential >/dev/null && echo PASS || echo FAIL
op item get audit-log-pepper          --vault "$VAULT_DEV" --fields credential >/dev/null && echo PASS || echo FAIL

# E24: pepper vault items provisioned (PROD)
op item get auth-session-code-pepper --vault "$VAULT_PROD" --fields credential >/dev/null && echo PASS || echo FAIL
op item get audit-log-pepper          --vault "$VAULT_PROD" --fields credential >/dev/null && echo PASS || echo FAIL

# E25: e2e fixture identities provisioned (DEV only)
op item get e2e-fixture-email --vault "$VAULT_DEV" >/dev/null && echo PASS || echo FAIL
# (Item should carry both regular and admin sub-fields or be split into two items per §1.3c — pick one shape and stay consistent.)

# E26: playbook §1.3b + §1.3c present
grep -cE '^### 1\.3b — Generate Auth Pepper Keys|^### 1\.3c — Provision E2E Fixture' playbooks/001_bootstrap/001_playbook.md
# expect: 2

# E27: preflight vault inventory updated
grep -cE 'auth-session-code-pepper|audit-log-pepper|e2e-fixture-email' playbooks/002_preflight/001_playbook.md
# expect: ≥ 5  (peppers in both DEV+PROD tables = 4 rows + e2e in DEV = 1 row)

# E28: Next.js rewrite unchanged (no new entries)
grep -c '/backend/:path\*' ui/packages/app/next.config.ts
# expect: 1  (the existing rewrite covers all new auth endpoints transparently)

# E29: trusted-client-ip middleware present + TRUSTED_PROXY_IPS env documented
test -f src/http/middleware/trusted_client_ip.zig && echo PASS || echo FAIL
grep -c 'TRUSTED_PROXY_IPS' docs/AUTH.md
# expect: ≥ 1  (deploy assumption documented)

# E30: session_id wording contradiction resolved (outside this spec's own meta-references)
grep -rli 'never embedded in user-visible URLs the user might paste\|never embedded in user-pasteable URLs' docs/AUTH.md src/ ui/ zombiectl/ 2>/dev/null | wc -l
# expect: 0  (Must-change A landed; the spec body itself retains the phrase only inside this eval line + the acceptance criterion + the Discovery historical reference)
```

---

## Dead Code Sweep

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| Plaintext `token` field in any auth handler | `grep -nE '"token":' src/http/handlers/auth/sessions.zig` | Matches only `token_name` (no plaintext token anywhere — the legacy PATCH handler is deleted in entirety per Q3). |
| Old `commandLogin` (pre-Effect or pre-this-rewrite) | `grep -rn 'commandLogin' zombiectl/src/` | Zero matches OR routes through the new login Effect/function only. |
| Pre-existing M68 §13 D27 stages that this spec replaces (none — this spec extends them) | N/A | All five named stages still present in `commands/core.ts`. |
| Old `verification_code` plaintext field in `SessionState` | `grep -nE 'verification_code[^_]' src/auth/sessions.zig` | Zero matches. |
| Legacy plaintext PATCH operation in OpenAPI | `jq '.paths["/v1/auth/sessions/{session_id}"].patch' public/openapi.json` | `null` (operation absent). Pins Q3 directive. |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What | Required output |
|------|-------|------|-----------------|
| Before any implementation | (this spec) | ChatGPT/Codex independent review of the protocol section. Captain has scheduled this; the spec is sized to be reviewable end-to-end. | Captain-acked sign-off; any protocol changes from the review land before CHORE(open). |
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage on the new error variants + ECDH round-trip + verification-code paths + atomic-consume replay + state-machine monotonicity. | Clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial pass against `docs/AUTH.md` Security-properties table + Threat Model + Replay Protections. Does each line of code trace to a listed property? Does the implementation close each listed threat? | Findings dispositioned. |
| After `gh pr create` | `/review-pr` | Re-runs against the immutable diff; catches regressions where any code drifts back toward "ECDH authenticates the CLI" framing OR re-introduces plaintext code storage OR loses the atomic consume. | Comments addressed. |
| After every push | `kishore-babysit-prs` | Greptile poll loop. | Two consecutive empty polls. |

---

## Discovery (consult log)

**May 17, 2026 — first rewrite.** Captain challenged the prior framing with: *"What attacker capability did we actually remove?"* Cross-LLM review (Claude + ChatGPT) confirmed the prior spec conflated transport confidentiality with CLI authentication. First rewrite scoped each property explicitly.

**May 17, 2026 — second rewrite (this version).** A follow-up ChatGPT review surfaced concrete protocol fixes:

- The session row was implicitly storing the plaintext verification code. Fix: `sha256(code || session_id)` storage.
- The GET polling endpoint was returning ciphertext + verification metadata, mixing polling with auth-proof retrieval. Fix: dedicated `POST /v1/auth/sessions/{id}/verify` endpoint; GET stays pure status.
- The CLI public key was being passed via URL query parameter (`?public_key=...`), leaking into browser history, telemetry, analytics, referrers, screenshots, and logs. Fix: store on session row at POST /sessions; dashboard fetches via GET.
- The session state machine was too weak (`pending → complete`). Fix: explicit `pending → verification_pending → consumed` with terminal `expired`/`aborted` states, monotonic transitions, atomic verified-consume.
- Replay protections were implicit. Fix: explicit invariants — single-use code, single-read ciphertext, atomic consume, no backward transitions.
- Rate limits were absent. Fix: pinned per-IP / per-Clerk-user / per-session limits.
- Cryptographic primitives were hand-wavey. Fix: explicit pinning of curve (P-256), KDF (HKDF-SHA256), AEAD (AES-256-GCM), nonce (96-bit random), tag (128-bit), code TTL (5 min), attempt cap (5).
- `token_name` defaulted to hostname-username, leaking workstation naming conventions. Fix: platform-family default (`macos-cli`, etc.); operator overrides via `--token-name`.
- Memory hygiene intent was not documented. Fix: intent-documented section.
- Autonomous-agent question: M74_002 is NOT general agent auth. Confirmed M75_xxx (separate milestone) owns persistent machine identity with a different trust model (persistent keypair + signed challenges).

**Captain also consolidated** in the same session: the auth-flow surface had been spread across M74_001 §3 (Effect-TS migration of auth command), M71_001 P2 §1-§5 (login UX hardening) plus M71_001 P2's deferred D20/D21/D24/D25/D26/D32, and this spec's protocol work. All three live here now to ship as one coherent surface; M74_001 trims to substrate-only; M71_001 P2 keeps only §6-§11 (dashboard/website UX, non-auth).

**May 17, 2026 — third rewrite (this version).** Third ChatGPT review on the consolidated spec returned `APPROVED WITH MANDATORY FIXES`. Verdict: "no longer crypto cargo cult — serious reviewable architecture." Seven mandatory fixes incorporated before any implementation:

- **Fix 1 — Verify idempotency window.** Added consume-idempotency semantics: 60-second replay window from same client fingerprint (`sha256(remote_addr || user_agent || session_id)`). Same fingerprint retry → same payload; different fingerprint or out-of-window → 410 `SessionConsumed`. Closes the "consume succeeded, response lost, client retried" failure mode. New tests: `test_verify_idempotent_replay_within_window`, `test_verify_replay_rejected_outside_window`, `test_verify_replay_rejected_from_different_fingerprint`, `test_verify_atomic_under_concurrent_correct_codes`.
- **Fix 2 — Centralized session store.** Deleted the in-memory `SessionStore` (`max_sessions: usize = 64`, single-instance only) in entirety. Rewrote on the existing Redis pool from M69_004. API process fails fast on startup if `REDIS_URL` is unset. Multi-pod deployment topology now safe — approve on pod A + poll on pod B + verify on pod C all hit the same store.
- **Fix 3 — Structured audit events.** Added explicit JSON shapes for every auth-relevant event (`auth.session.created`, `.approved`, `.verify_failed`, `.verified`, `.consumed`, `.consumed_replay`, `.aborted`, `.expired`, `.ratelimit.exceeded`). Routed to a dedicated `.auth_audit` Zig log scope so deploy-side log routing can fan it to a separate sink.
- **Fix 4 — `session_id` log redaction.** Classified `session_id` as a sensitive ephemeral capability (equivalent to a password-reset token). Added `redactSessionId()` helper; `.auth`-scope info/warn/error must use it; `.auth_audit` is the only place the full ID appears; metrics/traces never carry it as a label. New test `test_session_id_never_logged_unredacted` scans log capture for full-ID matches and fails the build.
- **Fix 5 — Clock-skew handling + `logout --all` rename.** Server-authoritative expiry with a 30s grace window; CLI countdown is informational only and cannot gate security. Renamed `zombiectl logout --all` (semantically misleading — users read it as "revoke active credentials" but it actually only aborted in-flight pending sessions) to `zombiectl logout` (clear local + abort in-flight pending sessions; documented explicitly that JWT revocation is NOT done). The `--all` flag is rejected with a clear error pointing at the new semantics.
- **Fix 6 — TLS / HSTS / transport.** Pinned HTTPS-only (HTTP → 308 to HTTPS); `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload` on every `/v1/auth/*` response (via load balancer or a new `security_headers.zig` middleware if absent); TLS 1.2 minimum; no secure-cookie handling on the API side (zombied never sets cookies — only the dashboard does, on its own origin).
- **Fix 7 — Session garbage collection.** Primary mechanism: Redis `EXPIRE` (TTL-driven, automatic). Secondary defensive sweep every 60s per pod (covers TTL-inadvertently-cleared blobs; no-op in steady state). Hard per-IP and per-Clerk-user caps enforced at creation via the rate-limit table (Fix 2's centralized counters). `maxmemory-policy allkeys-lfu` recommendation documented for the deploy.

Additionally:

- **Verification-code entropy.** Acknowledged ChatGPT's "6 digits is borderline" — explicitly future-improvement (8 alphanumeric `X4K9-TQ` shape) in *Out of Scope → Future improvements*; not v2.0 blocker because the 5-attempt cap + 5-min TTL caps brute-force success at 0.0005% per session-lifetime.
- **Clerk-JWT-as-transport.** Acknowledged ChatGPT's "do not fossilize" — explicitly future-improvement in *Out of Scope → Future improvements*: an API-minted scoped access token derived from Clerk auth is the right long-term shape; v2.0 ships with raw Clerk JWT transport for delivery speed.
- **Dashboard-JS-compromise as high-risk hardening area.** Already documented honestly in the Threat Model "what this flow does NOT protect against" table; AUTH.md gains a new "high-risk future hardening areas" subsection listing this explicitly so future SOC2 / security-review passes find it without re-reasoning from first principles.

**May 18, 2026 — fourth rewrite (this version).** Fourth ChatGPT review returned `CONDITIONAL GREENLIGHT` with five mandatory fixes. Captain's per-fix dispositions (May 17-18):

- **Fix #1 (HMAC pepper for verification code) — APPLIED.** Replaced `sha256(code || session_id)` with `HMAC-SHA256(AUTH_SESSION_CODE_PEPPER, session_id || code)`. New required env loaded from Vault at zombied boot; dashboard no longer hashes locally (only the API has the pepper); plaintext code travels on PATCH /approve over TLS to a Clerk-authenticated endpoint, lives momentarily in process memory, and is discarded after HMAC compute. Closes offline brute force from a Redis dump alone.
- **Fix #2 (active API/proxy MITM closure) — DEFERRED TO v2.1.** v2.0 explicitly does NOT close key-substitution MITM. The spec text was softened end-to-end: every "API never holds decrypt capability" claim becomes "honest API never holds decrypt capability"; new row 6 added to *what this flow does NOT protect against*; new Attack G walkthrough added to *Concrete attacker walkthroughs*; Invariant 13 softened. v2.1 closure detailed in *Out of Scope → Future improvements* (URL fragment binding for `cli_public_key` + HKDF info string binds both pubkeys + session_id). Tracked top priority for v2.1.
- **Fix #3 (deprecated plaintext PATCH compatibility) — REMOVED ENTIRELY.** Captain directive: "the plaintext PATCH never shipped in production; treat it as if it never existed." Q3 pinned answer rewritten; `acceptPatchPlaintextDeprecated` handler deleted from spec; OpenAPI `patch_auth_session` operation deleted from `authentication.yaml`; `UpgradeRequiredError` variant deleted; `auth.session.deprecated_patch_used` audit event deleted; deprecated-path tests deleted; Failure Modes row "old binary with PATCH plaintext" rewritten to "router 404"; Dead Code Sweep row updated. The spec's security claim ("plaintext JWT removed") is unconditionally true on day one. RULE NLG-perfect — there is no legacy path because there was never anything in production to be legacy to.
- **Fix #4 (`.auth_audit` pseudonymization) — APPLIED.** Audit events default to `session_id_hash = HMAC-SHA256(AUDIT_LOG_PEPPER, session_id)` + `session_id_prefix = first8(session_id)`; raw `session_id` only under `AUTH_AUDIT_INCLUDE_FULL_IDS=true` (env-gated, dev/staging-only, startup WARN). Restricted-routing requirement documented: `.auth_audit` sink MUST NOT route to customer-visible logs and MUST have tighter access controls than `.auth`. This is deploy-side discipline, not enforced by code; AUTH.md + deploy README carry the requirement. Mirrors the discipline already applied to `zmb_t_` tenant API keys (only the SHA-256 hash is persisted in `core.api_keys`).
- **Fix #5 (human-led-only hard wording) — APPLIED.** Added explicit prose to *Relationship to autonomous agents*, AUTH.md, and Invariant 20: M74_002 supports only human-led agents; CI / cron / K8s / hosted-agent / scheduled use is a spec violation; redirect to M75_xxx. Enforced by discipline + documentation, not by code (no programmatic way to detect "real human present"). Closes the future-drift hazard where someone rationalizes unattended use as "a kind of local agent."

Captain provided an operational clarification during Fix #4 review: audit recording is server-side (zombied); zombiectl emits only product analytics (M71_001 P1) and is structurally unfit for security audit because it doesn't see Clerk user, cross-tenant attempts, or brute-force across sessions. Both peppers (`AUTH_SESSION_CODE_PEPPER`, `AUDIT_LOG_PEPPER`) live in zombied process memory after Vault load; never crosses to CLI or dashboard.

**May 18, 2026 — fifth review pass (ChatGPT final).** Verdict: `GREENLIGHT: yes`. Attack G remains the explicit accepted risk for v2.0; the spec's documentation of that scope-out is what makes the greenlight defensible. Two non-blocking wording / implementation clarifications landed:

- **Must-change A (wording contradiction on session_id URLs).** Previous text said `session_id` must "never be embedded in user-visible URLs the user might paste" — but the flow itself requires `https://app.usezombie.com/cli-auth/{session_id}`. Reworded throughout: `session_id` appears **only** in the primary CLI-generated verification URL and the API route paths that consume it; forbidden in logs, analytics, metrics labels, secondary URLs, error bodies, and copied diagnostics. Both Invariant 6 and AUTH.md row 8 updated.
- **Must-change B (trusted client IP extraction pinned).** Previous fingerprint definition used `request.remote_addr` without specifying how it's derived behind a load balancer. Bad on two axes: (a) load-balancer IP collapses all CLIs into one fingerprint; (b) trusting unsigned `X-Forwarded-For` lets a client forge the IP. New "Trusted client IP extraction" subsection pins: `TRUSTED_PROXY_IPS` env allowlist; `X-Forwarded-For` accepted ONLY when the TCP peer is a configured trusted proxy; ignored otherwise. New middleware `src/http/middleware/trusted_client_ip.zig` runs before rate-limiting and any handler reading `remote_addr`. Five new tests pin the matrix.

Beyond these two, **ChatGPT explicitly recommended NOT changing**: v2.1 key-substitution closure (stays deferred per Captain's Fix #2 decision), autonomous-agent auth (M75_xxx owns this), API-minted scoped tokens (future improvement, do not fossilize Clerk-JWT-as-transport but do not bite it off here), 6-digit code entropy uplift (rate-limit cap is sufficient for v2.0), dashboard sessions/revoke UI (out of scope, future spec). The spec is sized to ship; expanding it again would make it unreviewable. Going to implementation.

**Future agents:** the question *"what attacker capability did we actually remove?"* should become mandatory in every security-spec review. If the answer is vague or amounts to "we added cryptography," the protocol is probably cargo-cult crypto and the spec needs a Threat Model rewrite before any implementation.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Tests (E1) | `make test && make test-integration` | _pending_ | |
| Lint (E2) | `make lint && (cd zombiectl && bun run lint)` | _pending_ | |
| Cross-compile (E3) | `zig build` x2 targets | _pending_ | |
| e2e (E4) | Playwright | _pending_ | |
| AUTH.md (E5) | grep for security-layers table + autonomous-agent redirect | _pending_ | |
| PATCH /approve shape (E6) | curl | _pending_ | |
| GET shape narrow (E7) | curl + jq | _pending_ | |
| gitleaks (E8) | `gitleaks detect` | _pending_ | |
| Session row plaintext-free (E9) | integration introspection | _pending_ | |
| AUTH_PRESET completeness (E10) | grep | _pending_ | |
| Argv-leak warning (E11) | runtime check | _pending_ | |
| token_name default (E12) | runtime check | _pending_ | |
| Harness (E13) | `make harness-verify` | _pending_ | |

---

## Out of Scope

- **Persistent device or agent identity.** Authoring a `zombiectl` device keypair, signing server-issued challenges, server-side agent inventory, scoped credentials, revocation — all **M75_xxx CLI Agent Identity** (to be authored). M74_002 intentionally does not include device identity; see *Relationship to autonomous agents*.
- **Hardware-backed authentication.** TPM / Secure Enclave / WebAuthn / passkey-based CLI auth is a separate downstream milestone.
- **Defense against compromised dashboard or compromised CLI host.** See Non-goals — these compromises defeat any flow this spec could produce.
- **Clerk session revocation.** Still a Clerk admin-API problem, not ours. `zombiectl logout --all` aborts in-flight login sessions but does NOT revoke already-minted JWTs.
- **`zmb_t_` API-key auth (Flow 3).** Separate surface; not affected by this spec.
- **Dashboard sessions surface.** Listing active CLI sessions on the dashboard with revoke buttons is a UX follow-up; this spec scopes mint-time only.
- **CLI binary upgrade prompt.** "Your `zombiectl` is out of date" is part of the Q3 dual-stack path's deprecation notice, but a richer in-CLI upgrade prompt is a separate UX consideration.
- **Effect-TS substrate itself.** M74_001 owns the dispatcher + layer + non-auth command migrations. If M74_001 lands first, the M74_002 login Effect targets it directly; if not, M74_002 ships in the current async shape and M74_001 sweeps it later.
- **Server-side session persistence.** Sessions stay in-memory in `src/auth/sessions.zig` for v2.0. Cross-instance session sharing (Redis / Postgres) is a separate decision tied to horizontal-scaling needs.
- **M71_001 P2 §6-§11 dashboard / website UX work** (TriggerPanel multi-card switch, provider-guidance table, GuidedTriggerCard / CronCard, OnboardingFlow, Hero CTA). Stays in M71_001 P2.
- **PostHog event-schema changes** for the auth-flow events. The existing `cli_command_finished` shape carries everything needed; new audit events are server-side (`auth.session.*`), not analytics.
- **AUTH.md Flow 2 / Flow 3 / webhook auth sections.** Out of scope; only Flow 1 changes.

### Future improvements (acknowledged in ChatGPT review; explicitly NOT v2.0)

These are recorded so future agents can find them without re-discovering the design tension; **not in scope for M74_002 implementation**.

- **Active API / proxy key-substitution closure (planned for v2.1).** v2.0 leaves Attack G (active key-substitution MITM) open by design — closure requires dashboard + CLI changes that are scoped out of M74_002 per Captain decision (May 17, 2026, ChatGPT review 3). v2.1 closes the gap via two pinned mechanisms:
  - **URL fragment binding.** The CLI opens the verify URL as `https://app.usezombie.com/cli-auth/{session_id}#cli_public_key=<base64url>&fp=<fingerprint>`. URL fragments are not sent to the server — they exist only in the browser. The dashboard reads `cli_public_key` from the fragment, GETs the session metadata from the API for `token_name` + state, compares the fragment's `cli_public_key` to the API-returned value, and aborts with `"session key mismatch"` on any divergence. The fragment value is authoritative; the API value is verification.
  - **HKDF transcript binding.** The HKDF `info` parameter changes from `"m74-002-v1"` to `"m74-002-v2" || session_id || cli_public_key || dashboard_public_key`. Any pubkey substitution by an active attacker produces a different derived AES key on the dashboard side than on the CLI side; decryption fails on the CLI with a hard `DecryptError`, the JWT is never recovered, and the substitution attempt is loud (not silent).
  - **Together these close Attack G.** Active API/proxy attackers can still observe ciphertext (already-existing TLS-inspection capability) but cannot decrypt without substituting the public key, and substitution is detected by either mechanism alone.
  - **Why v2.1 not v2.0:** Captain directive. The closure adds dashboard JS complexity, CLI URL-construction change, HKDF version bump, and a small migration coordination — outside M74_002's already-large scope. v2.0 lands the verification-code + ECDH + Redis store + pseudonymized audit; v2.1 lands the active-MITM closure. Tracked at top priority in the v2.1 milestone (to be authored).
- **Verification-code entropy uplift.** Move from 6 digits (1M entries) to 8 alphanumeric characters in a TOTP-style segmented format (e.g. `X4K9-TQ`). ~37× entropy improvement; human-typability preserved. v2.0 ships 6 digits because the 5-attempt cap + 5-min TTL already caps brute-force success at 0.0005% per session-lifetime — uplift is hygiene, not correctness. Track as a follow-up spec.
- **API-minted scoped access tokens instead of Clerk-JWT transport.** Long-term, the dashboard should not act as a Clerk-JWT broker — the API should mint its own scoped, short-lived access tokens (derived from a verified Clerk session) and the dashboard hands those to the CLI. Decouples the CLI's bearer shape from Clerk's JWT shape; lets the API revoke tokens server-side; supports per-CLI-install scopes. v2.0 ships raw Clerk JWT transport for delivery speed; do not fossilize this choice.
- **Dashboard-JS-compromise hardening.** Sub-resource integrity (SRI) on the dashboard bundle, content security policy (CSP) hardening, dependency-supply-chain pinning. v2.0 ships the existing dashboard with no changes; future spec lands the hardening. Acknowledged as a real attack surface in the Threat Model and AUTH.md "high-risk hardening areas" subsection.
- **Pub/sub on session state changes.** Currently the CLI polls every 1-5s for state transitions. A Redis pub/sub channel `auth:session:{id}:state` could let the CLI subscribe and receive an instant push on `verification_pending` and `consumed`. Trade-off: extra connection cost per CLI; nice-to-have UX, not behavior. Track separately.
- **Real JWT revocation.** Currently `zombiectl logout` deletes local credentials and aborts in-flight pending logins but does NOT revoke the active JWT (Clerk admin API call would be needed; rate-limited and not free). M75_xxx Agent Identity OR a separate "Clerk revocation integration" spec is the right home.
