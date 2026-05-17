# Handoff — Spec out: Supabase-style hardening for `zombiectl login` + `/cli-auth/{session_id}`

You are picking up from another agent that was scoping M68 §13 (zombiectl login
redesign) and identified a separate, larger problem worth a dedicated spec: the
current `zombiectl login` ↔ dashboard `/cli-auth/{session_id}` handshake is
considerably less hardened than the upstream Supabase CLI equivalent. Your job
is **to author that spec, not implement anything.**

## Captain identifier

The Captain is Kishore. Address as Captain / Skipper / Boss.

## Prior-agent feedback the Captain wants you to know

The Captain pushed back on the prior agent's audit twice in a row for being
sloppy:

1. The prior agent first claimed §13 needed a new `core.cli_tokens` table.
   **Wrong** — `core.api_keys` already exists (`schema/015_api_keys.sql`) with
   `key_name`, `revoked_at`, `last_used_at`, `active`.
2. The prior agent then claimed the CLI token *is* a `zmb_t_` row in
   `core.api_keys`. **Also wrong** — `docs/AUTH.md` Flow 1 makes clear that
   `zombiectl login` produces a **Clerk-issued JWT** (Token B,
   `aud=https://api.usezombie.com`), not a `zmb_t_` key. `core.api_keys` is
   Flow 3 (service-to-service), an entirely separate surface.
3. The prior agent then claimed the dashboard `/cli-auth/{session_id}` page
   does not exist and `zombiectl login` "404s today". **Also wrong** — the
   acceptance suite (`lifecycle-after-login.spec.js`) clicks
   `[data-testid="cli-auth-approve"]` on that page; the page exists. Captain
   explicitly called this blabber.

**Lesson the Captain wants encoded into your work:** read AUTH.md fully and
grep the actual code before writing any spec claim. Don't extrapolate. Don't
guess. If your spec asserts that something is missing, prove it with a path
+ grep that returns empty.

## What you are spec'ing

A standalone spec — separate from M68, separate from §13 of the M68 spec —
that hardens the `zombiectl login` ↔ dashboard `/cli-auth/{session_id}`
device-flow handshake to match the security posture of upstream Supabase CLI.

Reference implementation:
`~/Projects/oss/cli/apps/cli/src/next/commands/login/login.handler.ts` (228L,
Effect-TS, ECDH transport, verification code, token_name). You may also
inspect the rest of `~/Projects/oss/cli/apps/cli/src/next/commands/login/`
(login.command.ts, login.errors.ts, login.e2e.test.ts,
login.integration.test.ts) for the full pattern.

The hardenings the prior agent identified (verify each yourself before
committing them to the spec):

| # | Hardening | What Supabase does | What `zombiectl` does today |
|---|---|---|---|
| 1 | ECDH transport for the token | CLI gens keypair pre-browser-open; public_key in URL params; dashboard encrypts the issued token with CLI's public_key; CLI decrypts locally with private key | JWT travels plaintext in PATCH body (`PATCH /v1/auth/sessions/{id} { token: <jwt> }`); confidentiality relies on TLS alone |
| 2 | Out-of-band verification code | Dashboard displays a short verification code post-approve; user types code into CLI; CLI sends `(session_id, code)` to retrieve the encrypted token | Single "Approve" click; anyone who captures `session_id` (URL leak / browser-history sync / shoulder-surf) can race a hijack before the legitimate user clicks Approve |
| 3 | Token name (device label) | CLI defaults `token_name = hostname-username`; passed in URL params; dashboard labels the minted credential | No labeling concept — Clerk JWTs are stateless, can't carry a per-device label, and the auth-session row is in-memory anyway |
| 4 | Already-logged-in detection | CLI checks for existing token, prompts "Log in with a different account?" before kicking off the browser flow | Overwrites silently (M68 §13 D20 addresses this CLI-side via `--force`; the dashboard side does not surface "this CLI is replacing an existing session") |

## Why this is a separate spec, not part of M68

The Captain has decided M68 §13 stays CLI-side and ships the simpler
hardenings (idempotency, TTL countdown, exp-backoff polling, error taxonomy,
fail-loud hydration, argv-leak warning, decomposition, etc. — the D20–D32
list, modulo D21/D32 which were reframed). The deeper auth-flow hardenings
above need their own surface because:

- They touch **three repos/packages**: zombiectl (CLI), `ui/packages/app/`
  (dashboard `/cli-auth/{session_id}` page), zombied (`src/auth/sessions.zig`
  + `src/http/handlers/auth/sessions.zig` to carry pubkey/nonce/ciphertext +
  optional verification_code).
- The Clerk JWT model interacts non-trivially with ECDH and labels (#3 may
  not be implementable at all without changing what the CLI persists — see
  open question below).
- AUTH.md itself will need an update to reflect the new sequence diagram +
  field shapes.

## Inputs you must read before drafting

1. **`docs/AUTH.md` end-to-end.** Flow 1 (CLI device flow) is the primary
   target. Sections "Two tokens at a glance", "Backend validation", and
   "Test infrastructure — e2e fixture mint" inform constraints.
2. **`docs/TEMPLATE.md`** — the canonical spec template you must follow.
   Use `kishore-spec-new` to create the spec file (don't hand-roll the
   filename or frontmatter).
3. **`~/Projects/oss/cli/apps/cli/src/next/commands/login/login.handler.ts`**
   end-to-end (228L). Captain has explicitly told the prior agent to "use
   and leverage" patterns from this file. Note the Effect-TS shape — you'll
   port the *patterns*, not the *runtime*; zombiectl is plain Node.js.
4. **`zombiectl/src/commands/core.js`** — `commandLogin` (≈ lines 70–211)
   and `commandLogout` (≈ lines 213–220).
5. **`zombiectl/src/lib/credentials.js`** — credentials.json shape.
6. **The actual dashboard `/cli-auth/{session_id}` page in
   `ui/packages/app/app/`.** Prior agent's grep missed it; find it. Could
   be under a route group, could use a different folder name. Try
   `grep -rln 'cli-auth-approve' ui/packages/app/` first. If the page truly
   doesn't exist as a Next page file but the test passes against a deployed
   URL, document that finding precisely (with the grep output) — don't
   guess.
7. **`src/auth/sessions.zig`** + **`src/http/handlers/auth/sessions.zig`** —
   the SessionStore (in-memory) and the PATCH/POST/GET handler surface.
   Today the session carries `{ status, token }`. ECDH transport requires
   adding `{ public_key, nonce, ciphertext }` and the optional
   verification_code.
8. **`ui/packages/app/tests/e2e/acceptance/cli-acceptance/lifecycle-after-login.spec.js`**
   (or wherever it lives — grep for the filename) — pins the current
   contract.

## What the spec must produce

Use `kishore-spec-new` to create the spec under `docs/v2/pending/`. The spec
must include (per `docs/TEMPLATE.md`):

1. **Problem statement** — the four hardenings above, *each with a real
   threat model paragraph*. Not hand-wavy security theater. For #1: TLS
   alone vs ECDH at-rest-in-logs argument. For #2: the actual session-id
   hijack window, sized in seconds. For #3: ops-dashboard "which device
   is which" pain. For #4: silent token replacement and credential loss.
2. **Out-of-scope** — explicitly: this spec does NOT cover the M68 §13
   CLI-side dimensions (D20–D32 except where renumbered). Also out of
   scope: Clerk session revocation (still a Clerk admin-API problem, not
   ours).
3. **Acceptance criteria** — verifiable commands per dimension. "Works
   correctly" is not a criterion; "`zombiectl login` rejects a wrong
   verification code with exit code N and error tag VERIFICATION_FAILED"
   is.
4. **Dimensions / Sections / Workstreams** mapping to the four hardenings.
   Suggested initial cut (you decide the final shape):
   - §1 Wire protocol — extend session schema for ECDH (pubkey, nonce,
     ciphertext, verification_code); backend handler updates; AUTH.md
     update.
   - §2 CLI side — ECDH keypair gen; verification-code prompt; decrypt
     response; `token_name` URL param.
   - §3 Dashboard side — read URL params; display verification code +
     device label; ECDH-encrypt before PATCH; already-logged-in detection
     surface.
   - §4 Test coverage — extend `lifecycle-after-login.spec.js` (or its
     successor) to assert verification-code happy path + wrong-code reject;
     unit tests for ECDH round-trip; PR Discovery note for the rotated
     session schema.
5. **Open questions you must surface** (do NOT silently resolve these —
   ask Captain):
   - **Q1: Token labeling for stateless Clerk JWTs.** The CLI receives a
     Clerk-issued JWT, not a server-side row. There is nothing to label.
     Options: (a) save the label client-side only in credentials.json (for
     `zombiectl auth status` display only — no server visibility); (b)
     have the dashboard write a separate audit row in zombied recording
     "device X logged in at T" without touching the JWT itself; (c) defer
     #3 until/unless we switch CLI auth to `zmb_t_` keys. Surface the
     trade-offs; let Captain pick.
   - **Q2: Verification-code UX choice.** Supabase shows the code on the
     dashboard and asks the CLI to prompt for it. An alternative is the
     reverse — CLI prints the code, dashboard prompts. The second is more
     usual for hardware-keyed device flows (Google, AWS SSO). Both prevent
     session-id hijack; one is more familiar than the other. Captain
     should pick.
   - **Q3: Backward-compat during migration.** Old `zombiectl` binaries in
     the wild won't speak ECDH. Does the backend keep both PATCH shapes
     for a deprecation window, or hard-flip and force `zombiectl upgrade`?
     Note: pre-v2.0 (`cat VERSION` < 2.0.0) means RULE NLG forbids
     "legacy" naming or compat shims — surface this constraint to Captain.
   - **Q4: ECDH key gen library choice.** Node.js stdlib `crypto.subtle`
     vs `tweetnacl` vs `@noble/curves`. Browser side (dashboard) is
     basically forced into `crypto.subtle` (no extra dep ok). CLI side
     is a choice. Recommend; don't decide alone.
6. **Failure-mode + invariant tables** per TEMPLATE.md.
7. **Test specification** — per Dimension, the explicit test case file +
   name. No code without a test.

## What you do NOT do

- Do not write any code. This is a spec authoring task only.
- Do not commit anything other than the spec file. The spec lives in
  `docs/v2/pending/` and gets `Status: PENDING` per template.
- Do not start implementation. Captain explicitly wants the spec first,
  then a separate implementation decision.
- Do not touch `M68_001_*.md` (active worktree's spec). This is a
  *sibling* spec.
- Do not edit AUTH.md. The spec will describe the AUTH.md edits required,
  but the edits themselves land with the implementation, not the spec.
- Do not edit any harness/gate/hook to silence anything. If you hit a
  gate while writing markdown, the harness is right and the markdown is
  wrong. Stop and surface to Captain.

## Branch / worktree

This spec authoring should happen on a **new worktree** off `main`:

```bash
cd /Users/kishore/Projects/usezombie
git checkout main
git pull --ff-only
git branch feat/m69-cli-auth-handshake-hardening main   # or whichever Mxx Captain assigns
git worktree add ../usezombie-m69-cli-auth-handshake-hardening feat/m69-cli-auth-handshake-hardening
cd ../usezombie-m69-cli-auth-handshake-hardening
```

Then invoke `kishore-spec-new`. The current M68 worktree
(`feat/m68-trigger-dx-and-free-trial`) is mid-flight and must not be
touched.

## Definition of done for *this handoff*

A single new spec file under `docs/v2/pending/M{N}_{NNN}_*.md`, committed
on its own feature branch off main, no code changes, all the open
questions listed and unresolved (Q1–Q4 above, plus any new ones you find),
the `Status: PENDING` field set, ready for Captain's review and
plan-ceo-review / plan-eng-review / plan-design-review pass before
implementation.

When you finish: open a PR for the spec-only branch (no implementation),
title `spec(M{N}): cli-auth handshake hardening — ECDH + verification code
+ device label`. Captain reviews, then decides scope + ordering.
