<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_002: CLI ↔ dashboard auth handshake hardening (ECDH + verification code + device label)

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 002
**Date:** May 17, 2026
**Status:** PENDING
**Priority:** P1 — security boundary. Closes a session-id hijack window in the current `zombiectl login` ↔ `/cli-auth/{session_id}` device flow and brings the surface to Supabase CLI's hardening level.
**Categories:** API, CLI, UI
**Batch:** B2 — depends on M74_001 substrate landing.
**Branch:** feat/m74-002-cli-auth-handshake-hardening (to be created on CHORE(open))
**Depends on:** M74_001 (Effect-TS substrate in `zombiectl`). The new login handler is implemented on the Effect-TS dispatcher; landing this before M74_001 means rewriting the handler twice.
**Provenance:** human-written from `HANDOFF_SUPABASE_HARDENING_SPEC.md` (Captain ask, May 17, 2026).

**Canonical architecture:** `docs/AUTH.md` Flow 1 (CLI device flow). Updated as part of this spec.

---

## Implementing agent — read these first

1. `docs/AUTH.md` end-to-end. **Flow 1** is the primary target; "Two tokens at a glance", "Backend validation", and "Test infrastructure — e2e fixture mint" inform constraints. Read this before writing any spec claim — prior agents hit Captain rejections for getting Flow 1 vs Flow 3 confused.
2. [`supabase/cli`'s `apps/cli/src/next/commands/login/login.handler.ts`](https://github.com/supabase/cli/blob/main/apps/cli/src/next/commands/login/login.handler.ts) (228L) end-to-end. The pattern to port — ECDH keypair, public_key in URL params, ciphertext PATCH body, verification-code prompt, token_name device label. Captain has explicitly told prior agents to "use and leverage" this file's patterns.
3. `zombiectl/src/commands/auth.{js,ts}` — the current `commandLogin` (`~70-211`) and `commandLogout` (`~213-220`). M74_001 migrates these to Effect-TS; M74_002 adds the hardening on the migrated handler.
4. `src/auth/sessions.zig` + `src/http/handlers/auth/sessions.zig` — the SessionStore (in-memory) and the `PATCH`/`POST`/`GET` handler surface. Today the session row carries `{ status, token }`. ECDH transport requires adding `{ public_key, nonce, ciphertext }` plus optional `verification_code`.
5. The dashboard `/cli-auth/{session_id}` page in `ui/packages/app/app/`. Find it with `grep -rln 'cli-auth-approve' ui/packages/app/` (acceptance test pins the testid). Read it end-to-end before drafting §3.

---

## Applicable Rules

- `docs/AUTH.md` — applies to every change in this spec. Flow 1 sequence diagrams + field shapes update as part of the work.
- `docs/REST_API_DESIGN_GUIDELINES.md` — applies to the session-handler shape changes.
- `docs/greptile-learnings/RULES.md` — RULE NLG forbids `legacy_` / `V2` framing. Migration window for old `zombiectl` binaries needs a different mechanism (see Failure Modes + Q3).
- `docs/ZIG_RULES.md` — applies to `src/auth/sessions.zig` + `src/http/handlers/auth/sessions.zig`. PG-drain lifecycle if any new DB-backed work lands; default to in-memory unless drift demands persistence.
- `docs/SCHEMA_CONVENTIONS.md` — N/A (sessions stay in-memory in v2 per `src/auth/sessions.zig`'s current shape).

---

## Overview

**Goal (testable):** `zombiectl login` rejects a wrong verification code with a typed `VerificationFailedError` and a documented exit code; a successful login fetches the token via ECDH-decrypt of the dashboard's PATCH ciphertext (never in plaintext); the minted credential carries a device label visible in `zombiectl auth status` and (if Q1 resolves to dashboard-visible) on the dashboard sessions surface; an already-authenticated CLI prompts to replace the session before kicking off a fresh browser flow.

**Problem:** The current `zombiectl login` device flow is materially weaker than the upstream Supabase CLI equivalent on four axes (each is a real threat, not theatre — see HANDOFF for the threat-model paragraphs):

1. **Token travels plaintext in the PATCH body** (`PATCH /v1/auth/sessions/{id} { token: <jwt> }`); confidentiality relies on TLS alone. Supabase's CLI encrypts the token with an ECDH-derived key the CLI generated pre-browser-open; the dashboard never sees plaintext.
2. **No out-of-band verification.** Anyone who captures `session_id` (URL leak, browser-history sync, shoulder-surf) can race a hijack before the legitimate user clicks Approve. Supabase displays a short verification code post-Approve; the user types it into the CLI; the CLI sends `(session_id, code)` to retrieve the encrypted token. Race closed.
3. **No device label.** Clerk JWTs are stateless and the in-memory session row has no `token_name` concept. Operators cannot tell which CLI on which machine has an active session.
4. **No already-logged-in detection on the dashboard side.** M68 §13 D20 added `--force` to the CLI; the dashboard still silently replaces an existing session without surfacing "this CLI is replacing your previous device."

**Solution summary:** Extend the session schema with `{ public_key, nonce, ciphertext, verification_code, token_name }`. Move the new `zombiectl login` handler onto M74_001's Effect-TS substrate so the four hardenings compose cleanly. Update `/cli-auth/{session_id}` to read the URL params, display the verification code + device label, and ECDH-encrypt before PATCH. Update `docs/AUTH.md` Flow 1 to reflect the new sequence diagram + field shapes. Land an e2e test that asserts wrong-code rejection.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/auth/sessions.zig` | EDIT | Extend `SessionState` with `public_key`, `nonce`, `ciphertext`, `verification_code`, `token_name`. |
| `src/http/handlers/auth/sessions.zig` | EDIT | `POST /v1/auth/sessions` accepts the new fields; `GET /v1/auth/sessions/{id}` returns ciphertext+nonce when approved; `PATCH` writes ciphertext, not plaintext. |
| `zombiectl/src/commands/auth.ts` | EDIT | New `login.handler.ts`-style Effect (ECDH keypair gen → open browser with pubkey + token_name in URL → poll → verification-code prompt → ECDH-decrypt → persist). Already-logged-in detection prompts before kicking off. |
| `zombiectl/src/lib/credentials.ts` | EDIT | Persist optional `token_name` alongside the JWT for `zombiectl auth status` display. |
| `ui/packages/app/app/cli-auth/[session_id]/page.tsx` (or wherever it lives — grep first) | EDIT | Read `public_key` + `token_name` from URL params; display verification code post-Approve; ECDH-encrypt before PATCH; surface "Replacing previous session" when a token already exists. |
| `ui/packages/app/tests/e2e/acceptance/cli-acceptance/lifecycle-after-login.spec.ts` (or successor) | EDIT | Assert verification-code happy path + wrong-code reject + device-label visibility. |
| `docs/AUTH.md` | EDIT | Flow 1 sequence diagram + field shapes updated; new "verification code" + "ECDH transport" subsections. |
| `zombiectl/src/errors/auth.ts` (created in M74_001) | EDIT | Add `VerificationFailedError`, `DecryptError`, `SessionHijackSuspectedError` variants. |
| `zombiectl/test/auth/login.unit.test.ts` (new) | CREATE | ECDH round-trip + verification-code happy/sad paths. |

---

## Sections (implementation slices)

### §1 — Wire protocol

Extend `SessionState` in `src/auth/sessions.zig` and the handler surface in `src/http/handlers/auth/sessions.zig` to carry the new fields. New shape of the JSON body for `POST /v1/auth/sessions`:

```
{ "public_key": "<base64-encoded P-256>", "token_name": "<hostname>-<username>" }
```

`GET /v1/auth/sessions/{id}` (CLI poll) returns:

```
{ "status": "pending" | "approved" | "expired",
  "verification_code": "<6-digit>"  // only present when status=approved
  "nonce": "<base64>"                // only present when status=approved
  "ciphertext": "<base64>"           // only present when status=approved (encrypted token)
}
```

`PATCH /v1/auth/sessions/{id}` (dashboard, post-Approve) accepts:

```
{ "status": "approved",
  "nonce": "<base64>",
  "ciphertext": "<base64>",        // dashboard-encrypted token using CLI's public_key
  "verification_code": "<6-digit>" // dashboard-generated; surfaced to user post-Approve
}
```

AUTH.md update lands in the same diff.

### §2 — CLI handler

The new login handler is an Effect-TS Effect (the substrate from M74_001) that:

1. Generates an ECDH P-256 keypair via `crypto.subtle.generateKey` (Node 20+ stdlib — see Q4).
2. Determines the default `token_name` (`hostname-username`); allows override via `--token-name`.
3. POSTs `{ public_key, token_name }` to `/v1/auth/sessions`; receives `session_id`.
4. Detects an existing local token; prompts `"You're already logged in as <foo>. Replace?"` unless `--force`.
5. Opens the browser to `${dashboard}/cli-auth/{session_id}?public_key=...&token_name=...`.
6. Polls `GET /v1/auth/sessions/{id}` (exponential backoff per M68 §13 D24).
7. On `status=approved`, prompts the user for the 6-digit verification code displayed on the dashboard.
8. POSTs the entered code (or includes it on the next GET) — fail with `VerificationFailedError` on mismatch.
9. ECDH-decrypts the ciphertext to recover the JWT.
10. Persists `{ token, token_name }` to credentials.json.

### §3 — Dashboard handler

`/cli-auth/{session_id}` reads `public_key` + `token_name` from URL params. Surfaces the device label on the Approve screen ("This will log in **{token_name}**"). On Approve:

1. Mint the Clerk JWT for the active session.
2. Generate a 6-digit `verification_code`.
3. Generate an ECDH ephemeral keypair; derive the shared secret with the CLI's `public_key`; AES-256-GCM-encrypt the JWT with a `nonce`.
4. PATCH the session with `{ status: "approved", nonce, ciphertext, verification_code }`.
5. Display the verification code to the user with a "Type this into the CLI" hint.

When the dashboard detects an existing session for the active user, surface "This will replace your previous CLI session on **{previous_token_name}**" alongside the Approve button.

### §4 — Test coverage

Extend `lifecycle-after-login.spec.ts` (or successor) to cover: verification-code happy path; wrong-code reject; expired-session reject; replaced-session warning; device-label visibility in `zombiectl auth status` output. New unit test `zombiectl/test/auth/login.unit.test.ts` asserts the ECDH round-trip math + the dispatcher's error mapping for each new error variant.

### §5 — AUTH.md update

Flow 1's sequence diagram + every payload shape updated. Two new subsections: "Verification code" (the OOB confirmation flow + threat model) and "ECDH transport" (the keypair lifecycle + AES-GCM parameters). The threat model paragraphs from the HANDOFF land verbatim — not security theatre, real hijack-window-in-seconds language.

---

## Interfaces

See §1 for the wire shapes. Locked contracts:

- `POST /v1/auth/sessions` request body shape (additive — `public_key` and `token_name` are new fields; backward-compat depends on Q3).
- `GET /v1/auth/sessions/{id}` response shape (additive — three new optional fields when `status=approved`).
- `PATCH /v1/auth/sessions/{id}` request body shape (replaces `token` with `nonce + ciphertext + verification_code`).
- `zombiectl auth status --json` output gains `token_name` field (additive).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Wrong verification code | User mistypes or hijacker has wrong code | `VerificationFailedError` → exit code documented in M74_001's error taxonomy. CLI offers one retry; second wrong code aborts. |
| Decryption failure | Ciphertext tampered with mid-flight or key mismatch | `DecryptError` → exit code distinct from `VerificationFailedError`. Surfaces "Session integrity check failed — try `zombiectl login` again" message. |
| Old `zombiectl` binary with PATCH plaintext | A pre-M74_002 binary in the wild hits the new handler. | Per Q3: either keep both PATCH shapes for a deprecation window OR hard-flip with a clear "upgrade required" error. RULE NLG forbids `legacy_` naming pre-2.0.0 — surface to Captain. |
| Browser closed mid-flow | User closes the tab before Approve | CLI poll eventually times out; surfaces "Session expired. Re-run `zombiectl login`." Existing M68 §13 D24 backoff applies. |
| Already-logged-in user runs `zombiectl login` | Replacement detection fires. | CLI prompts "Replace existing session for <token_name>?" unless `--force`. Dashboard surfaces the same on the Approve screen. |
| Clerk session no longer valid when dashboard mints | User's dashboard session expired between page load and Approve click | Standard Clerk re-auth flow on the dashboard side; CLI poll times out and surfaces "Session expired." |

---

## Invariants

1. **Token never crosses the wire in plaintext after this spec.** Enforced by the PATCH handler's shape — the route rejects a body with a top-level `token` field (only `ciphertext`+`nonce` accepted). Compile-time + runtime check in `src/http/handlers/auth/sessions.zig`.
2. **A verification code is required for every successful login.** Enforced by the CLI handler — there is no path through the Effect that recovers a token without entering the code.
3. **Device label is persisted with every minted credential.** Enforced by `credentials.json` schema — `token_name` is non-optional in the persisted shape (defaulted to `hostname-username` if the user passes nothing).
4. **No new TypeScript error variant in this spec is unhandled by M74_001's dispatcher formatter.** Enforced by TypeScript exhaustiveness on the formatter switch.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_ecdh_round_trip_unit` | CLI keypair + dashboard keypair → derive shared secret on both sides → AES-256-GCM encrypt-decrypt of a JWT round-trips byte-identically. |
| `test_login_happy_path_e2e` | Full flow: `zombiectl login` → browser Approve → user types verification code → credential persisted with `token_name` set. |
| `test_login_wrong_verification_code` | User enters wrong code → handler fails with `VerificationFailedError` and the documented exit code; no credential persisted. |
| `test_login_expired_session` | Session expires before dashboard Approve → CLI surfaces "Session expired" and exits without persisting. |
| `test_login_already_authenticated_prompts` | Existing credential present → CLI prompts to replace; `--force` skips the prompt. |
| `test_login_already_authenticated_dashboard_surfaces` | Existing session for the same user → dashboard shows "Replacing previous session on <token_name>." |
| `test_auth_status_shows_token_name` | `zombiectl auth status` output includes the `token_name` of the active credential. |
| `test_patch_rejects_plaintext_token` | Server returns 400 when PATCH body contains `{ token: <…> }` instead of `{ nonce, ciphertext, verification_code }`. |
| `test_authmd_flow1_diagram_present` | `grep -c 'verification_code\|ECDH\|public_key' docs/AUTH.md` returns ≥ 3 (each concept named). |

---

## Acceptance Criteria

- [ ] `make test` green (Zig unit + zombiectl + UI + app).
- [ ] `make test-integration` green (DB + Redis; session handler).
- [ ] `make lint` green.
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`.
- [ ] Playwright e2e `lifecycle-after-login.spec.ts` passes including all new assertions.
- [ ] `gitleaks detect` clean.
- [ ] `docs/AUTH.md` Flow 1 reflects the new sequence; manual diff review.
- [ ] No file over 350 lines added.
- [ ] `grep -n 'token:' src/http/handlers/auth/sessions.zig | grep -v 'token_name'` returns zero matches in the PATCH handler body (no plaintext token field).

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: tests
make test && make test-integration

# E2: lint
make lint

# E3: cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux

# E4: e2e
cd ui/packages/app && bun run test:e2e lifecycle-after-login

# E5: AUTH.md updated
grep -c 'verification_code\|ECDH\|public_key' docs/AUTH.md

# E6: PATCH handler rejects plaintext token (manual integration check via curl)
curl -X PATCH https://api.usezombie.local/v1/auth/sessions/<id> \
  -H 'Content-Type: application/json' \
  -d '{"status":"approved","token":"abc"}'
# expect 400 with a "use ciphertext + nonce" error

# E7: gitleaks
gitleaks detect
```

---

## Dead Code Sweep

If Q3 resolves to "hard-flip, no compat shim":

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| Plaintext `token` field in PATCH handler | `grep -n '"token"' src/http/handlers/auth/sessions.zig` | Only `token_name` matches |
| Old `commandLogin` (pre-Effect) | `grep -rn 'commandLogin' zombiectl/src/` | Zero matches (M74_001 migration deletes the legacy entry point) |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage on the new error variants + ECDH round-trip. | Clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial pass against `docs/AUTH.md` + the new threat-model paragraphs. Does the wire shape actually close the hijack window? | Findings dispositioned. |
| After `gh pr create` | `/review-pr` | Re-runs against the immutable diff; catches plaintext-token regressions. | Comments addressed. |

---

## Discovery (consult log)

Empty at creation. Open questions in §Open Questions need Captain decisions at plan-eng-review.

---

## Open Questions (Captain decides)

- **Q1: Token labeling for stateless Clerk JWTs.** The CLI receives a Clerk-issued JWT, not a server-side row. Options: (a) save the label client-side only in `credentials.json` (visible in `zombiectl auth status` only — no server visibility); (b) dashboard writes a separate audit row in `zombied` recording "device X logged in at T" without touching the JWT itself; (c) defer #3 until CLI auth switches to `zmb_t_` keys. Surface trade-offs; Captain picks.
- **Q2: Verification-code direction.** Supabase shows the code on the dashboard, CLI prompts. Alternative: CLI prints the code, dashboard prompts (closer to Google / AWS SSO). Both prevent session-id hijack; one is more familiar. Captain picks.
- **Q3: Backward-compat during migration.** Old `zombiectl` binaries in the wild won't speak ECDH. Backend keeps both PATCH shapes for a deprecation window, OR hard-flip and force `zombiectl upgrade`? Note: pre-v2.0 (`cat VERSION` < 2.0.0) means RULE NLG forbids `legacy_` naming — surface this constraint to Captain.
- **Q4: ECDH key-gen library.** Node.js stdlib `crypto.subtle` (no extra dep, available Node 20+) vs `tweetnacl` vs `@noble/curves`. Browser is forced into `crypto.subtle`. CLI is a choice. Recommendation: `crypto.subtle` both sides — same API surface, zero extra deps. Captain confirms.
- **Q5: Effect-TS error variant naming.** `VerificationFailedError` vs `Auth.VerificationFailed` (Effect's tagged-class convention). Defer to M74_001's chosen taxonomy convention.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Tests (E1) | `make test && make test-integration` | | |
| Lint (E2) | `make lint` | | |
| Cross-compile (E3) | `zig build` | | |
| e2e (E4) | Playwright | | |
| AUTH.md (E5) | grep | | |

---

## Out of Scope

- **M68 §13 CLI-side dimensions (D20-D32 except where renumbered).** Those landed in M68. This spec is the deeper auth-flow hardening, not the polish.
- **Clerk session revocation.** Still a Clerk admin-API problem, not ours.
- **`zmb_t_` API-key auth (Flow 3).** Separate surface; not affected by this spec.
- **Dashboard sessions surface.** Listing active CLI sessions on the dashboard with revoke buttons is a UX follow-up; this spec scopes mint-time hardening.
- **CLI binary upgrade prompt.** "Your `zombiectl` is out of date" is a separate UX consideration; surface as a follow-up if Q3 picks hard-flip.
- **Effect-TS migration itself.** Sibling workstream M74_001 owns the substrate; this spec depends on it.
