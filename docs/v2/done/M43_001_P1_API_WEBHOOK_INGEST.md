# M43_001: Webhook Ingest — GitHub Actions in v1, Generic Receiver Pattern

**Prototype:** v2.0.0
**Milestone:** M43
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** DONE
**Priority:** P1 — launch-blocking. The wedge IS GitHub Actions CD-failure responder; without webhook ingest the wedge has no entry point. The chat-only fallback (M42 steer) is the secondary interaction, not the primary.
**Categories:** API
**Batch:** B1 — depends on M42 (event stream + envelope), parallel with M40, M41, M44, M45.
**Branch:** feat/m43-001-webhook-ingest (separate PR, based on chore/m43-review-amendments per PR #272 stacking decision May 1, 2026)
**Depends on:** M42_001 (writes to `zombie:{id}:events` with the M42 envelope shape). M45_001 (webhook secret stored as a structured credential under `${secrets.github.webhook_secret}`). M40_001 (the per-zombie thread that consumes the event must exist).

**Canonical architecture:** `docs/architecture/user_flow.md` (webhook input + GH Actions trigger walkthrough), `docs/architecture/data_flow.md` §B (TRIGGER — three callers, ONE ingress).

---

## Cross-spec amendments (Apr 30, 2026 — pre-implementation review pass)

The original spec below was written assuming greenfield. M28_001 (DONE) had already shipped most of the auth substrate. M45_001 (DONE) reshaped credential storage. The following decisions reconcile this spec with what's already in tree.

**A1 — URL: `POST /v1/webhooks/{zombie_id}/github`.** Reuse the existing `/v1/webhooks/{zombie_id}/...` namespace (5 webhook routes already there: clerk, svix, approval, grant_approval, the generic per-zombie receiver). Workspace prefix `/v1/workspaces/{ws}/...` is wrong for webhook URLs — webhooks are signature-authed, not session-authed, so the workspace is the auth scope of the *secret* (vault lookup), not of the URL.

**A2 — Reuse `webhook_sig.zig` middleware (M28).** The handler is auth-free: dedupe → parse → filter → normalize → XADD → 202 (per A8 — dedupe FIRST after the middleware's signature verify, before any expensive parse/filter work). All HMAC verification happens upstream in middleware. GitHub's HMAC scheme is already registered in `src/zombie/webhook_verify.zig` `PROVIDER_REGISTRY` (`{name="github", sig_header="x-hub-signature-256", prefix="sha256="}`).

**A3 — Drop the proposed `webhooks/common.zig`.** No HMAC primitive lives in the handler tree; `src/auth/middleware/webhook_sig.zig` is the source of truth. Re-implementing HMAC in two places is RULE NLR-violating debt.

**A4 — Envelope shape: flat `request={…}`, not nested `request: { message, metadata }`.** Per the canonical envelope at `src/zombie/event_envelope.zig` and `docs/architecture/user_flow.md` line 113. The `request_json` field is opaque JSON bytes; its top-level keys are `{run_url, head_sha, conclusion, repo, attempt, run_id, head_branch, workflow_name, received_at}`.

**A5 — Drop the pre-M45 single-string-secret fallback** (RULE NLG no legacy framing pre-v2.0.0). M45 is DONE; there is one vault shape.

**A6 — Webhook secrets are workspace-scoped via M45 credentials.**
- Resolver: `vault.loadJson(workspace_id, name=<trigger.source>)` where `name` defaults to the `trigger.source` value (e.g. `"github"`). Pull the `webhook_secret` field from the parsed JSON.
- Frontmatter override: optional `x-usezombie.trigger.credential_name: github-prod` for workspaces with multiple GH integrations needing different secrets.
- Operator UX: `zombiectl credential add github --data='{"webhook_secret":"<S>","api_token":"<PAT>"}'` once per workspace per provider; all zombies in the workspace using `trigger.source: github` share the same secret.
- The `serve_webhook_lookup.zig` resolver migrates from the per-zombie `config_json.signature.secret_ref` pointer pattern to the workspace-credential-by-name pattern.

**A7 — Remove the dead `webhook_secret_ref` column** (RULE NLR touch-it-fix-it). The column on `core.zombies` has zero writers in tree; the only readers are the legacy AgentMail URL-embedded-secret path (per M28's matrix). Removal is in M43's scope:
- `schema/007_core_zombies.sql` — drop the column line + comment (pre-v2.0 Schema Guard: edit-in-place, no `ALTER TABLE`).
- `src/cmd/serve_webhook_lookup.zig` — drop the `webhook_secret_ref` SELECT path; resolver simplifies to workspace-credential lookup.
- `src/http/handlers/webhooks/zombie.zig` — drop `webhook_secret_ref` from row struct + SELECT; rewrite header comment to point at workspace credentials.
- AgentMail's URL-embedded-secret path is removed with the column. Per M28_003, AgentMail's migration trajectory was already toward `/v1/webhooks/svix/{zombie_id}`.

**A8 — Dedupe ordering: dedupe AFTER zombie validation + action filter, BEFORE XADD.** Key shape: `webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery}` `EX 259200` (72 h — covers GitHub's documented maximum retry window for the same `X-GitHub-Delivery` UUID). Dedupe-hit response: `{ "deduped": true }` (no `original_event_id` — operator-debuggable info is in the events stream). The `gh:` namespace prefix prevents collision with body-`event_id`-based dedupe keys for the same zombie.

The original draft placed dedupe right after signature verification ("dedupe FIRST"). Greptile P1 flagged the resulting silent-data-loss vector: a delivery that hits a paused or nonexistent zombie 4xx's, the dedupe slot is already claimed for 72 h, and an operator-triggered redelivery (GitHub UI "Recent Deliveries → Redeliver" button) within the 72 h window would silently return `{deduped: true}` instead of being processed once the zombie is unpaused. Moving dedupe to after the zombie + filter validations means only events we are *actually* about to XADD consume a slot. The 4xx and 200-ignored paths are idempotent + cheap (single SELECT + JSON parse), so re-running them on operator redelivery is safe and correct.

**A9 — Error codes** (RULE EMS — reuse existing `UZ-WH-NNN` family from `src/errors/`):
- Bad signature → `UZ-WH-010` (existing).
- Missing workspace credential or missing `webhook_secret` field → new `UZ-WH-020` *webhook_credential_not_configured*.
- Payload >1 MiB → new `UZ-WH-030` *webhook_payload_too_large*.

**A10 — File layout (PUB GATE + FILE SHAPE strict):**
- `src/http/handlers/webhooks/github.zig` — handler, conventional layout (matches `webhooks/clerk.zig` pattern), one pub fn `invokeGithubWebhook`.
- `src/zombie/webhook/normalizer/github.zig` — pure transformation, conventional layout, one pub fn `normalize(alloc, raw_body) ![]u8`. Operations-over-value (no state to bind).
- `src/zombie/webhook/normalizer/github_test.zig` — colocated unit tests, RULE TST-NAM clean (no milestone IDs in names).
- `src/http/handlers/webhooks/github_integration_test.zig` — colocated integration test (project convention; not under top-level `tests/`).
- Fixtures: `samples/fixtures/webhook_github/{workflow_run_failure.json, workflow_run_success.json}` (no `m43-` prefix).

**A11 — Workspace IDOR is naturally resolved.** No explicit binding check needed: the resolver reads `core.zombies.workspace_id` for the given `{zombie_id}` from the URL, then loads workspace credential by that workspace. An attacker forging URLs cannot bypass HMAC; an attacker who has another workspace's secret has compromised that workspace's vault, which is a higher-tier breach. Document the invariant; no extra code.

**A12 — Spec invariants & out-of-scope updated:**
- Out: 60s secret cache (security-implications; defer to a follow-up perf spec only after measurement).
- Out: Manual smoke against staging (defer to M49 install-skill validation, parallel to M48b's pattern).
- Out: high-entropy secret generation and "show once" UX (M49 owns the install-skill UX; M43 assumes the secret exists in vault).

**A13 — Slack workspace-global surface removed in this same PR (no separate spec).**

Per operator decision during the review walkthrough, the dead Slack-app inbound layer was deleted as part of this same review-amendment PR rather than scheduled as a follow-up spec. The investigation confirmed:

- Zero UI/CLI consumers for `/v1/slack/install` or `/v1/slack/callback`.
- Outbound Slack (`chat.postMessage` from zombies) uses each operator's *workspace* credential `slack`.bot_token — unaffected.
- M47 approval inbox is in-dashboard, not Slack-button-driven — `/v1/slack/interactions` was strictly orphan.
- The platform-ops wedge has zombies posting *to* Slack, never receiving from Slack — `/v1/slack/events` had no live use case.

**What was deleted:** `SLACK_SIGNING_SECRET` env read, `auth/middleware/slack_signature.zig`, `auth/middleware/oauth_state.zig` (Slack-only — referenced `slack:oauth:nonce:` Redis keys), the four `/v1/slack/*` routes + path matches + route-table + invoke entries, `src/http/handlers/slack/{events,interactions,oauth,oauth_client}.zig`, `public/openapi/paths/slack.yaml` + root.yaml refs, the four `route_manifest.zig` entries, and the Slack carve-outs in `scripts/check_openapi_url_shape.py`. Net deletion: ~2100 lines.

**What stayed:** `webhook_verify.PROVIDER_REGISTRY.SLACK` (used by the generic per-zombie webhook receiver if any zombie ever declares `trigger.source: slack`). Outbound Slack via `chat.postMessage`.

**Also deleted in this PR:** `/v1/github/callback` (path match in `src/http/router.zig`, the `github_callback` `Route` enum variant, `src/http/handlers/auth/github_callback.zig`, `src/auth/github.zig` (412 lines), the OpenAPI entry, the route-manifest entry). `zombiectl workspace add` no longer emits `install_url` or opens the GitHub App page — the App-installation flow is being replaced by per-workspace credential entry under M45 (`zombiectl credential add github`). When/if the GitHub App OAuth flow is reintroduced, it ships as a fresh design.

Verified clean: `zig build`, `make test`, `make openapi` (route-manifest ↔ openapi parity), `make lint`, orphan-grep for the removed symbols.

When "Slack-as-input" becomes a real product need (e.g. a `usezombie-slack-bot` skill, or per-zombie Slack subscriptions), it ships as a fresh design rather than reviving the workspace-global path.

The original spec text below describes the *problem and intent* correctly; the *implementation shape* is superseded by the amendments above where they conflict.

---

## Implementing agent — read these first

1. `docs/architecture/user_flow.md` §8.5 — the GH Actions trigger walkthrough is the worked example.
2. M42's spec (sibling) for the EventEnvelope shape — DO NOT invent a new envelope.
3. GitHub's webhook docs: HMAC SHA-256 signature in `X-Hub-Signature-256` header, payload signed with workspace's webhook secret.
4. `src/auth/middleware/webhook_sig.zig` — the canonical HMAC verification middleware. All webhook signature checks live here; handlers are auth-free (per A2/A3). The provider registry is in `src/zombie/webhook_verify.zig` (`PROVIDER_REGISTRY`); GitHub is already registered.
5. `samples/platform-ops/SKILL.md` — what the zombie expects to reason over when actor=webhook:github (the SKILL.md prose teaches it to fetch GH run logs first).

---

## Overview

**Goal (testable):** A configured GitHub repo posts a `workflow_run` event with `conclusion=failure` to `POST /v1/webhooks/{zombie_id}/github`. The receiver verifies the `X-Hub-Signature-256` HMAC against the workspace's stored `github.webhook_secret` (HMAC verification is upstream in `auth/middleware/webhook_sig.zig`; the handler is auth-free — see A2). On valid signature: normalize the payload into an EventEnvelope (M42 shape) with `actor=webhook:github`, `event_type=webhook`, and a flat `request_json` whose top-level keys are `{run_url, head_sha, conclusion, repo, attempt, run_id, head_branch, workflow_name, received_at}` (per A4 — no nested `request.message` / `request.metadata` wrapper). XADD to `zombie:{id}:events`. Return 202 within 100ms. The zombie's per-zombie thread (M40) picks up the event, processes it via M42's processEvent + M41's executor session. The agent reasons, fetches GH run logs via `http_request` (the GH API token is in the vault as `${secrets.github.api_token}`), correlates, posts to Slack.

**Problem:** No HTTP receiver exists today for any external webhook. M42 owns the event stream + history but not ingest. The GH Actions wedge is structurally unbuildable until this lands.

**Solution summary:** A single ingest endpoint per source. v1 ships GitHub-specific: `POST /v1/webhooks/{zombie_id}/github` (per A1). The receiver is split: `auth/middleware/webhook_sig.zig` does HMAC + vault lookup; the handler is auth-free and just runs dedupe → parse → filter → normalize → XADD → 202. All semantics (which event types matter, what the agent does with them) live in the SKILL.md — not in the receiver. Generic enough to add `/v1/webhooks/{zombie_id}/gitlab`, `.../bitbucket` later by mirroring the receiver pattern.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/webhooks/github.zig` | NEW | GH-specific receiver: dedupe → parse → filter → normalize → XADD → 202 (per A2/A8 — HMAC + vault lookup happen upstream in `auth/middleware/webhook_sig.zig`; the handler is auth-free and dedupes before any parse/filter work) |
| `src/http/router.zig` | EXTEND | Register `/v1/webhooks/{zombie_id}/github` (A1 — namespace shared with clerk/svix/approval/grant_approval/zombie) |
| `src/zombie/webhook/normalizer/github.zig` | NEW | GH `workflow_run` payload → EventEnvelope normalizer (A10 layout) |
| `src/zombie/webhook/normalizer/github_test.zig` | NEW | Colocated unit tests (A10, RULE TST-NAM clean) |
| `src/http/handlers/webhooks/github_integration_test.zig` | NEW | Colocated integration test (A10 — project convention; not under top-level `tests/`) |
| `schema/007_core_zombies.sql` | EDIT | Drop dead `webhook_secret_ref` column (A7, pre-v2.0 Schema Guard edit-in-place) |
| `src/cmd/serve_webhook_lookup.zig` | EDIT | Drop `webhook_secret_ref` SELECT path; resolver simplifies to workspace-credential lookup (A6/A7) |
| `src/http/handlers/webhooks/zombie.zig` | EDIT | Drop `webhook_secret_ref` from row struct + SELECT; comment points at workspace credentials (A7) |
| `samples/platform-ops/TRIGGER.md` (or merged frontmatter — wait for M46) | EXTEND | Add `trigger.webhook.github: enabled` flag (or equivalent under `x-usezombie:`) |
| `samples/platform-ops/SKILL.md` | EXTEND | Add a paragraph teaching the agent: when actor=webhook:github, first fetch GH run logs via `http_request GET https://api.github.com/repos/{request_json.repo}/actions/runs/{request_json.run_id}/logs` (placeholder shape matches A4's flat envelope; see §6 for the full prose) |
| `samples/fixtures/webhook_github/workflow_run_failure.json` | NEW | Real GH webhook payload structure (anonymized) |
| `samples/fixtures/webhook_github/workflow_run_success.json` | NEW | Used to test "ignore success" filtering |

---

## Sections (implementation slices)

### §1 — End-to-end algorithm (middleware + handler)

`POST /v1/webhooks/{zombie_id}/github`

Per A2, HMAC verification + vault lookup happen in `src/auth/middleware/webhook_sig.zig` *upstream* of the handler. The handler is auth-free. Steps below are split accordingly so an implementer doesn't add HMAC code to `src/http/handlers/webhooks/github.zig`.

**Middleware: `src/auth/middleware/webhook_sig.zig`** (already exists; A3 forbids re-implementing HMAC):

```
M1. Read raw request body (signature is over raw bytes — no JSON parse yet)
M2. Read X-Hub-Signature-256 header → expected = "sha256=" + hex(HMAC-SHA256(secret, body))
M3. Resolve workspace_id by reading `core.zombies.workspace_id` for the `{zombie_id}` from the URL (A11)
    → load workspace credential `github` via `vault.loadJson(workspace_id, "github")`
    → pull `webhook_secret` field (A6)
M4. Compute actual_sig = sha256_hmac(webhook_secret, body)
M5. constant_time_compare(actual, expected); on fail → 401 Unauthorized (UZ-WH-010)
    On missing credential / missing field → 401 (UZ-WH-020 webhook_credential_not_configured, A9)
    On body > 1 MiB → 413 BEFORE signature verify (UZ-WH-030 webhook_payload_too_large, A9)
```

**Handler: `src/http/handlers/webhooks/github.zig`** (NEW — A2's `dedupe → parse → filter → normalize → XADD → 202`):

```
H1. Dedupe FIRST (per A8 — before any parse/filter work):
    `SET NX webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery} <placeholder> EX 259200`.
    If key already existed → return 200 OK `{"deduped": true}` and stop.
H2. Parse JSON body; verify event type is workflow_run
H3. Filter: only conclusion=failure (success/cancelled ignored, return 204 No Content with reason)
H4. Normalize → EventEnvelope (§2)
H5. XADD zombie:{id}:events
H6. Return 202 Accepted with event_id
```

**Implementation default**: `constant_time_compare` via std.crypto in Zig (`std.crypto.utils.timingSafeEql`). Never use string equality. Lives in middleware, not the handler.

**Implementation default**: target latency end-to-end <100ms; the bottleneck is the middleware's vault lookup. Resolve the webhook_secret per request — no in-process cache. A short-TTL cache was deliberately ruled out of scope by A12 (security implications + needs measurement first); revisit only via a follow-up perf spec backed by a real bottleneck measurement.

### §2 — Payload normalization

GitHub `workflow_run.failure` event has ~80 fields; agent only needs ~6. Normalizer extracts:

```
EventEnvelope {
  event_id: <auto from Redis XADD>,
  zombie_id, workspace_id,
  actor: "webhook:github",
  event_type: "webhook",
  request_json: {                       // flat per A4 — no nested message/metadata wrapper
    run_url: <html_url>,
    head_sha: <sha>,
    conclusion: "failure",
    repo: "<owner>/<name>",
    attempt: <attempt>,
    run_id: <id>,
    head_branch: <branch>,
    workflow_name: <name>,
    received_at: <RFC3339>,
  },
  created_at: <now>,
}
```

`request_json` is opaque JSON bytes carrying the structured fields the agent reasons over. Per the canonical envelope at `src/zombie/event_envelope.zig` (A4); no `message` summary field is constructed by the receiver.

### §3 — Replay and idempotency

GitHub retries on 5xx with exponential backoff. To handle replays without duplicate processing:

- The receiver computes a `delivery_id` from `X-GitHub-Delivery` header (a UUID GH provides)
- Use `delivery_id` as a stable component of a hash → write to a Redis key `webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery}` with EX 259200 (72h). The `{zombie_id}` scopes the key so two zombies in the same workspace subscribed to the same repo don't share dedupe state.
- If `SET NX` fails, return 200 OK with `{"deduped": true}` — don't 4xx GH (they'd retry). No `original_event_id`; operator-debuggable info lives in the events stream.

**Implementation default**: 72h dedupe window covers GitHub's documented maximum retry window for the same `X-GitHub-Delivery` UUID. After that, treat as new.

### §4 — Workspace-scoped webhook secret resolution

The webhook secret is **workspace-scoped** (per A6 — not per-zombie, not global). Stored in the vault as a structured credential named `github` with field `webhook_secret`; one `github` credential per workspace covers every zombie in that workspace whose `trigger.source: github`. M43 assumes the secret exists in vault by the time webhook traffic arrives — secret generation + the "show once" UX are owned by M49's install-skill (per A12 — out of M43's scope).

**Implementation default**: M45 is DONE — there is one vault shape. Resolve via `vault.loadJson(workspace_id, name="github")` and pull the `webhook_secret` field (A5 + A6). No pre-M45 single-string fallback; RULE NLG forbids the legacy framing pre-v2.0.0. Optional `x-usezombie.trigger.credential_name` frontmatter override exists for the rare multi-integration case (per A6).

### §5 — Event-type filtering

GH webhooks can carry many event types (`push`, `pull_request`, `workflow_run`, `deployment`, etc.). v1 only handles `workflow_run` with `conclusion=failure`. Other event types: 204 No Content with `{"ignored": "<reason>"}`. Generic receiver pattern — adding `pull_request` later means another handler in the same path.

### §6 — Sample SKILL.md prose extension

Add to `samples/platform-ops/SKILL.md`:

> **When the trigger is a GitHub Actions failure webhook** (`actor=webhook:github` in the event):
> 1. Read `request_json.run_url` and `request_json.repo` from the event.
> 2. Call `http_request GET https://api.github.com/repos/{request_json.repo}/actions/runs/{request_json.run_id}/logs` with header `Authorization: Bearer ${secrets.github.api_token}`. Parse the failed step.
> 3. Cross-reference recent commits (last 5 on `request_json.head_branch`) via `http_request GET https://api.github.com/repos/{request_json.repo}/commits?sha={request_json.head_sha}&per_page=5` — look for migration files, config changes, dependency bumps.
> 4. Correlate with fly + upstash health (see existing prose).
> 5. Post a Slack diagnosis with the run URL, the failing step name, the most likely root cause from the cross-reference, and a remediation suggestion.

This section is what makes the wedge real. Without it, the receiver lands events the agent doesn't know how to act on.

---

## Interfaces

```
HTTP:
  POST /v1/webhooks/{zombie_id}/github
       headers: X-Hub-Signature-256, X-GitHub-Event, X-GitHub-Delivery
       body: raw GitHub webhook payload
       → 202 Accepted { event_id }
       → 200 OK { deduped: true }
       → 204 No Content { ignored: "<event-type>" }   for non-workflow_run or non-failure
       → 401 Unauthorized                              on signature mismatch
       → 413 Payload Too Large                         for >1MB body

Vault credential shape (M45 structured):
  github = {
    api_token: "<PAT with read scope on Actions>",
    webhook_secret: "<HMAC secret matching repo's webhook config>",
  }

Redis dedupe key:
  webhook:dedup:{zombie_id}:gh:<X-GitHub-Delivery>  EX 259200
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Bad HMAC | Wrong secret in vault, or attacker | 401 with `UZ-WH-010` (A9), no XADD, log `webhook_signature_mismatch` |
| Missing workspace credential or missing `webhook_secret` field | Operator hasn't run `zombiectl credential add github` for this workspace | 401 with `UZ-WH-020 webhook_credential_not_configured` (A9) so the log surfaces it loudly |
| Payload >1 MiB | Adversarial or unusual workflow | 413 with `UZ-WH-030 webhook_payload_too_large` (A9) immediately, no signature verify |
| GH retries on 5xx | Our handler errored | Dedupe key prevents duplicate processing on retry |
| `workflow_run` with conclusion=success | Normal GH traffic | 204 with reason; don't burn agent budget |
| `delivery_id` reuse (rare) | GH bug or replay attack | Dedupe key catches; 200 OK with `deduped: true` |

---

## Invariants

1. **Signature verified before any handler work.** XADD, JSON parse, and dedupe never run on unsigned/badly-signed payloads. (Vault lookup is itself part of HMAC verification — the secret has to be loaded to compute the comparison; it's middleware-internal, not handler-visible.)
2. **Constant-time comparison.** Never string-compare HMACs.
3. **Dedupe is keyed by `X-GitHub-Delivery`**, not by payload hash, since GH guarantees delivery_id stability across retries.
4. **The receiver is dumb.** It does not interpret the payload beyond filtering on event type + conclusion. All product logic lives in SKILL.md.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_valid_signature_xadds_event` | POST signed `workflow_run.failure` payload → 202 + event lands on `zombie:{id}:events` with correct envelope |
| `test_bad_signature_401` | POST with tampered body → 401, no XADD |
| `test_replay_dedupe` | POST same `X-GitHub-Delivery` twice → first 202, second 200 with `deduped: true`, exactly 1 event in stream |
| `test_workflow_run_success_ignored` | POST `workflow_run.success` → 204, no XADD |
| `test_other_event_type_ignored` | POST `push` event → 204, no XADD |
| `test_payload_too_large` | POST 2MB body → 413 before signature verify |
| `test_normalizer_extracts_request_json` | Real GH payload (fixture) → assert flat `request_json` top-level keys match A4 |
| `test_e2e_zombie_processes_webhook` | Full path: POST → XADD → worker thread (M40) picks up → processEvent (M42) → executor.startStage (M41) → assertion: `core.zombie_events` row with `actor=webhook:github`, `status=processed` |

Fixtures in `samples/fixtures/webhook_github/` (A10 — no `m43-` prefix).

---

## Acceptance Criteria

- [x] Endpoint live at `POST /v1/webhooks/{zombie_id}/github` with HMAC verified upstream by `webhook_sig`.
- [x] Resolver migrated to workspace credentials (`vault.loadJson(zombie:<source>) → webhook_secret`) per A6.
- [x] `webhook_secret_ref` column dropped per A7; URL-embedded-secret path torn out (matcher, middleware, registry chain) per RULE NLR.
- [x] `UZ-WH-020` + `UZ-WH-030` error codes registered.
- [x] GitHub `workflow_run` normalizer with unit-test coverage of failure / success / malformed / missing-keys / default-attempt cases.
- [x] OpenAPI parity at `/v1/webhooks/{zombie_id}/github`; URL-shape carve-out justified.
- [x] `make lint` + `zig build test` green.
- [x] Cross-compile clean: x86_64-linux + aarch64-linux.
- [ ] `make memleak` clean (run during CHORE(close)).
- [ ] Manual end-to-end smoke against staging — intentionally deferred to M49 install-skill validation per A12 (don't re-add without amending A12).

---

## Discovery

- The URL-embedded-secret legacy path (matcher branch, `WebhookRoute.secret`, `AuthCtx.webhook_provided_secret`, middleware Strategy 1, the entire `webhook_url_secret.zig` middleware, the registry chain + private accessor) was orphaned by removing `webhook_secret_ref`. Cleaning it in the same PR per RULE NLR — the user explicitly authorized "no legacy sprawl, full clean" during PLAN.
- The two-segment URL form `/v1/webhooks/{zombie_id}/{X}` would have collided with the new `/github` action route had it stayed; simplifying `matchWebhookRoute` to single-segment was a correctness fix on top of the cleanup.
- The dedicated `/github` URL means the source-derived signature scheme can be confidently treated as `webhook_verify.GITHUB` even before the body is parsed; the existing detect-by-config_json path stays for the generic `/v1/webhooks/{zombie_id}` receiver.
- **Filtered-event response status: 200 OK + `{"ignored": "<reason>"}` body** (revised from §5's draft "204 No Content with body"). RFC 9110 §6.4.5 forbids a message body on 204 — some CDNs and HTTP/2 proxies strip 204+body silently, others normalize to 502, both of which would mask the operator-visible diagnostic shown in GitHub's "Recent Deliveries" dashboard. The 2xx semantics for GitHub's retry behavior are unchanged; only the operator-debuggability of the `ignored` reason improves.

---

## Files Changed (final)

NEW: `src/http/handlers/webhooks/github.zig`, `src/zombie/webhook/normalizer/github.zig`, `src/zombie/webhook/normalizer/github_test.zig`, `samples/fixtures/webhook_github/{workflow_run_failure,workflow_run_success}.json` (with sibling copies under `src/zombie/webhook/normalizer/` for `@embedFile` boundary).

DROPPED: `src/auth/middleware/webhook_url_secret.zig`, `core.zombies.webhook_secret_ref` column.

EDIT: `src/cmd/serve_webhook_lookup.zig`, `src/http/handlers/webhooks/zombie.zig`, `src/http/router.zig`, `src/http/route_matchers.zig`, `src/http/route_table.zig`, `src/http/route_table_invoke.zig`, `src/http/route_manifest.zig`, `src/http/server.zig`, `src/http/test_harness.zig`, `src/http/webhook_test_fixtures.zig`, `src/http/webhook_http_integration_test.zig`, `src/http/handlers/cross_workspace_idor_test.zig`, `src/http/route_matchers_test.zig`, `src/http/router_test.zig`, `src/auth/middleware/webhook_sig.zig`, `src/auth/middleware/webhook_sig_test.zig`, `src/auth/middleware/auth_ctx.zig`, `src/auth/middleware/mod.zig`, `src/auth/tests.zig`, `src/cmd/serve.zig`, `src/errors/error_registry.zig`, `src/errors/error_entries.zig`, `src/errors/error_registry_test.zig`, `src/main.zig`, `schema/007_core_zombies.sql`, `public/openapi/paths/webhooks.yaml`, `public/openapi/root.yaml`, `scripts/check_openapi_url_shape.py`, `samples/platform-ops/SKILL.md`.

The original Files Changed table above (under §Files Changed) reflected the spec's pre-amendment plan; the actual scope is wider per A7's URL-embedded-secret teardown decision.
