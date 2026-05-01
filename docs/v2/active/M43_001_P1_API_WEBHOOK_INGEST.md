# M43_001: Webhook Ingest — GitHub Actions in v1, Generic Receiver Pattern

**Prototype:** v2.0.0
**Milestone:** M43
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — launch-blocking. The wedge IS GitHub Actions CD-failure responder; without webhook ingest the wedge has no entry point. The chat-only fallback (M42 steer) is the secondary interaction, not the primary.
**Categories:** API
**Batch:** B1 — depends on M42 (event stream + envelope), parallel with M40, M41, M44, M45.
**Branch:** chore/m43-review-amendments (folded into PR #272 alongside the review amendments + Slack/GitHub-App removal per operator authorization Apr 30, 2026)
**Depends on:** M42_001 (writes to `zombie:{id}:events` with the M42 envelope shape). M45_001 (webhook secret stored as a structured credential under `${secrets.github.webhook_secret}`). M40_001 (the per-zombie thread that consumes the event must exist).

**Canonical architecture:** `docs/architecture/user_flow.md` (webhook input + GH Actions trigger walkthrough), `docs/architecture/data_flow.md` §B (TRIGGER — three callers, ONE ingress).

---

## Cross-spec amendments (Apr 30, 2026 — pre-implementation review pass)

The original spec below was written assuming greenfield. M28_001 (DONE) had already shipped most of the auth substrate. M45_001 (DONE) reshaped credential storage. The following decisions reconcile this spec with what's already in tree.

**A1 — URL: `POST /v1/webhooks/{zombie_id}/github`.** Reuse the existing `/v1/webhooks/{zombie_id}/...` namespace (5 webhook routes already there: clerk, svix, approval, grant_approval, the generic per-zombie receiver). Workspace prefix `/v1/workspaces/{ws}/...` is wrong for webhook URLs — webhooks are signature-authed, not session-authed, so the workspace is the auth scope of the *secret* (vault lookup), not of the URL.

**A2 — Reuse `webhook_sig.zig` middleware (M28).** The handler is auth-free: parse → filter → dedupe → normalize → XADD → 202. All HMAC verification happens upstream in middleware. GitHub's HMAC scheme is already registered in `src/zombie/webhook_verify.zig` `PROVIDER_REGISTRY` (`{name="github", sig_header="x-hub-signature-256", prefix="sha256="}`).

**A3 — Drop the proposed `webhooks/common.zig`.** No HMAC primitive lives in the handler tree; `src/auth/middleware/webhook_sig.zig` is the source of truth. Re-implementing HMAC in two places is RULE NLR-violating debt.

**A4 — Envelope shape: flat `request={…}`, not nested `request: { message, metadata }`.** Per the canonical envelope at `src/zombie/event_envelope.zig` and `docs/architecture/user_flow.md` line 113. The `request_json` field is opaque JSON bytes; its top-level keys are `{run_url, head_sha, conclusion, ref, repo, attempt, run_id, head_branch, workflow_name, received_at}`.

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

**A8 — Dedupe ordering: dedupe FIRST after signature verify, before any filtering.** Key shape: `webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery}` `EX 259200` (72 h — covers GitHub's documented maximum retry window for the same `X-GitHub-Delivery` UUID). Dedupe-hit response: `{ "deduped": true }` (no `original_event_id` — operator-debuggable info is in the events stream). The `gh:` namespace prefix prevents collision with body-`event_id`-based dedupe keys for the same zombie.

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

**What stayed:** `webhook_verify.PROVIDER_REGISTRY.SLACK` (used by the generic per-zombie webhook receiver if any zombie ever declares `trigger.source: slack`). Outbound Slack via `chat.postMessage`. `/v1/github/callback` (vendor-pinned by GitHub App manifest, alive and used by `zombiectl workspace add` for App-installation binding — different concern from Slack inbound).

Verified clean: `zig build`, `make test`, `make openapi` (route-manifest ↔ openapi parity), `make lint`, orphan-grep for the removed symbols.

When "Slack-as-input" becomes a real product need (e.g. a `usezombie-slack-bot` skill, or per-zombie Slack subscriptions), it ships as a fresh design rather than reviving the workspace-global path.

The original spec text below describes the *problem and intent* correctly; the *implementation shape* is superseded by the amendments above where they conflict.

---

## Implementing agent — read these first

1. `docs/architecture/` §8.5 — the GH Actions trigger walkthrough is the worked example.
2. M42's spec (sibling) for the EventEnvelope shape — DO NOT invent a new envelope.
3. GitHub's webhook docs: HMAC SHA-256 signature in `X-Hub-Signature-256` header, payload signed with workspace's webhook secret.
4. `src/http/handlers/zombies/steer.zig` — mirror its HMAC verification pattern (if any) or borrow from the Svix integration if present.
5. `samples/platform-ops/SKILL.md` — what the zombie expects to reason over when actor=webhook:github (the SKILL.md prose teaches it to fetch GH run logs first).

---

## Overview

**Goal (testable):** A configured GitHub repo posts a `workflow_run` event with `conclusion=failure` to `POST /v1/webhooks/{zombie_id}/github`. The receiver verifies the `X-Hub-Signature-256` HMAC against the workspace's stored `github.webhook_secret`. On valid signature: normalize the payload into an EventEnvelope (M42 shape) with `actor=webhook:github`, `event_type=webhook`, `request.message=<short summary>` and `request.metadata={run_id, run_url, head_sha, conclusion, ref, repo, attempt}`. XADD to `zombie:{id}:events`. Return 202 within 100ms. The zombie's per-zombie thread (M40) picks up the event, processes it via M42's processEvent + M41's executor session. The agent reasons, fetches GH run logs via `http_request` (the GH API token is in the vault as `${secrets.github.api_token}`), correlates, posts to Slack.

**Problem:** No HTTP receiver exists today for any external webhook. M42 owns the event stream + history but not ingest. The GH Actions wedge is structurally unbuildable until this lands.

**Solution summary:** A single ingest endpoint per source. v1 ships GitHub-specific: `/webhooks/github`. The endpoint handler is thin: verify signature, normalize, XADD, 202. All semantics (which event types matter, what the agent does with them) live in the SKILL.md — not in the receiver. Generic enough to add `/webhooks/gitlab`, `/webhooks/bitbucket` later by mirroring the receiver pattern.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/webhooks/github.zig` | NEW | GH-specific receiver: parse → filter → dedupe → normalize → XADD → 202 (HMAC verified upstream by `auth/middleware/webhook_sig.zig`, A2/A3) |
| `src/http/router.zig` | EXTEND | Register `/v1/webhooks/{zombie_id}/github` (A1 — namespace shared with clerk/svix/approval/grant_approval/zombie) |
| `src/zombie/webhook/normalizer/github.zig` | NEW | GH `workflow_run` payload → EventEnvelope normalizer (A10 layout) |
| `src/zombie/webhook/normalizer/github_test.zig` | NEW | Colocated unit tests (A10, RULE TST-NAM clean) |
| `src/http/handlers/webhooks/github_integration_test.zig` | NEW | Colocated integration test (A10 — project convention; not under top-level `tests/`) |
| `schema/007_core_zombies.sql` | EDIT | Drop dead `webhook_secret_ref` column (A7, pre-v2.0 Schema Guard edit-in-place) |
| `src/cmd/serve_webhook_lookup.zig` | EDIT | Drop `webhook_secret_ref` SELECT path; resolver simplifies to workspace-credential lookup (A6/A7) |
| `src/http/handlers/webhooks/zombie.zig` | EDIT | Drop `webhook_secret_ref` from row struct + SELECT; comment points at workspace credentials (A7) |
| `samples/platform-ops/TRIGGER.md` (or merged frontmatter — wait for M46) | EXTEND | Add `trigger.webhook.github: enabled` flag (or equivalent under `x-usezombie:`) |
| `samples/platform-ops/SKILL.md` | EXTEND | Add a paragraph teaching the agent: when actor=webhook:github, first fetch GH run logs via `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` |
| `samples/fixtures/webhook_github/workflow_run_failure.json` | NEW | Real GH webhook payload structure (anonymized) |
| `samples/fixtures/webhook_github/workflow_run_success.json` | NEW | Used to test "ignore success" filtering |

---

## Sections (implementation slices)

### §1 — Endpoint + signature verification

`POST /v1/webhooks/{zombie_id}/github`

```
1. Read raw request body (DO NOT parse JSON yet — signature is over raw bytes)
2. Read X-Hub-Signature-256 header → expected = "sha256=" + hex(HMAC-SHA256(secret, body))
3. Resolve workspace_id by reading `core.zombies.workspace_id` for the `{zombie_id}` from the URL (A11) → load workspace credential `github` via `vault.loadJson` → pull `webhook_secret` field
4. Compute actual_sig = sha256_hmac(webhook_secret, body)
5. constant_time_compare(actual, expected); on fail → 401 Unauthorized
6. Parse JSON body; verify event type is workflow_run
7. Filter: only conclusion=failure (success/cancelled ignored, return 204 No Content with reason)
8. Normalize → EventEnvelope (§2)
9. XADD zombie:{id}:events
10. Return 202 Accepted with event_id
```

**Implementation default**: `constant_time_compare` via std.crypto in Zig (`std.crypto.utils.timingSafeEql`). Never use string equality.

**Implementation default**: target latency end-to-end <100ms; the bottleneck is vault lookup. Cache the webhook_secret per-workspace in zombied-api's process for 60s with explicit invalidation on credential update.

### §2 — Payload normalization

GitHub `workflow_run.failure` event has ~80 fields; agent only needs ~6. Normalizer extracts:

```
EventEnvelope {
  event_id: <auto from Redis XADD>,
  zombie_id, workspace_id,
  actor: "webhook:github",
  event_type: "webhook",
  request: {
    message: "GH Actions deploy failed: workflow=<name>, run=<run_id>, attempt=<n>",
    metadata: {
      provider: "github",
      run_id: <id>,
      run_url: <html_url>,
      head_sha: <sha>,
      head_branch: <branch>,
      conclusion: "failure",
      repo: "<owner>/<name>",
      attempt: <attempt>,
      workflow_name: <name>,
      received_at: <RFC3339>,
    },
  },
  created_at: <now>,
}
```

The `message` is a short human-readable summary; `metadata` carries the structured fields the agent reasons over.

### §3 — Replay and idempotency

GitHub retries on 5xx with exponential backoff. To handle replays without duplicate processing:

- The receiver computes a `delivery_id` from `X-GitHub-Delivery` header (a UUID GH provides)
- Use `delivery_id` as a stable component of a hash → write to a Redis key `webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery}` with EX 259200 (72h). The `{zombie_id}` scopes the key so two zombies in the same workspace subscribed to the same repo don't share dedupe state.
- If `SET NX` fails, return 200 OK with `{"deduped": true}` — don't 4xx GH (they'd retry). No `original_event_id`; operator-debuggable info lives in the events stream.

**Implementation default**: 72h dedupe window covers GitHub's documented maximum retry window for the same `X-GitHub-Delivery` UUID. After that, treat as new.

### §4 — Per-zombie webhook secret resolution

The webhook secret is per-zombie-config (or per-workspace), not global. Stored in the vault as a structured credential `github` with field `webhook_secret`. The install-skill (M49) prompts for this when generating the `.usezombie/platform-ops/` config.

**Implementation default**: support both shapes during M45 transition:
- Structured (M45 done): `secrets.github.webhook_secret`
- Single-string fallback (pre-M45): `secrets.github_webhook_secret`

### §5 — Event-type filtering

GH webhooks can carry many event types (`push`, `pull_request`, `workflow_run`, `deployment`, etc.). v1 only handles `workflow_run` with `conclusion=failure`. Other event types: 204 No Content with `{"ignored": "<reason>"}`. Generic receiver pattern — adding `pull_request` later means another handler in the same path.

### §6 — Sample SKILL.md prose extension

Add to `samples/platform-ops/SKILL.md`:

> **When the trigger is a GitHub Actions failure webhook** (`actor=webhook:github` in the event):
> 1. Read `metadata.run_url` and `metadata.repo` from the event.
> 2. Call `http_request GET https://api.github.com/repos/{metadata.repo}/actions/runs/{metadata.run_id}/logs` with header `Authorization: Bearer ${secrets.github.api_token}`. Parse the failed step.
> 3. Cross-reference recent commits (last 5 on `metadata.head_branch`) via `http_request GET https://api.github.com/repos/{metadata.repo}/commits?sha={metadata.head_sha}&per_page=5` — look for migration files, config changes, dependency bumps.
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
| Bad HMAC | Wrong secret in vault, or attacker | 401, no XADD, log `webhook_signature_mismatch` |
| Missing webhook_secret in vault | Operator forgot to add it | 401 with body `{"reason": "webhook_secret_not_configured"}` so log surfaces it loudly |
| GH retries on 5xx | Our handler errored | Dedupe key prevents duplicate processing on retry |
| Payload >1MB | Adversarial or unusual workflow | 413 immediately, no signature verify |
| `workflow_run` with conclusion=success | Normal GH traffic | 204 with reason; don't burn agent budget |
| `delivery_id` reuse (rare) | GH bug or replay attack | Dedupe key catches; 200 OK with `deduped: true` |

---

## Invariants

1. **Signature verified before any work**. Vault lookup, XADD, parsing — none happen on unsigned/badly-signed payloads.
2. **Constant-time comparison**. Never string-compare HMACs.
3. **Dedupe is keyed by `X-GitHub-Delivery`**, not by payload hash, since GH guarantees delivery_id stability across retries.
4. **The receiver is dumb**. It does not interpret the payload beyond filtering on event type + conclusion. All product logic lives in SKILL.md.

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
| `test_normalizer_extracts_metadata` | Real GH payload (fixture) → assert metadata fields match |
| `test_secret_cache_invalidation` | Update github.webhook_secret in vault → next POST uses new secret (cache busts within 60s) |
| `test_e2e_zombie_processes_webhook` | Full path: POST → XADD → worker thread (M40) picks up → processEvent (M42) → executor.startStage (M41) → assertion: `core.zombie_events` row with `actor=webhook:github`, `status=processed` |

Fixtures in `samples/fixtures/webhook_github/` (A10 — no `m43-` prefix).

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 9 tests above
- [ ] Manual smoke: configure a real GH repo's webhook to point at staging zombid, push a failing deploy, observe zombie posts diagnosis to Slack within 30s
- [ ] Replay attack test: capture a real signed payload, replay 73h later → second is treated as new (post-window) but inside 72h is deduped
- [ ] `make memleak` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
