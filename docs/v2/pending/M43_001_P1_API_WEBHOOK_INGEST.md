# M43_001: Webhook Ingest — GitHub Actions in v1, Generic Receiver Pattern

**Prototype:** v2.0.0
**Milestone:** M43
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking. The wedge IS GitHub Actions CD-failure responder; without webhook ingest the wedge has no entry point. The chat-only fallback (M42 steer) is the secondary interaction, not the primary.
**Categories:** API
**Batch:** B1 — depends on M42 (event stream + envelope), parallel with M40, M41, M44, M45.
**Branch:** feat/m43-webhook-ingest (to be created)
**Depends on:** M42_001 (writes to `zombie:{id}:events` with the M42 envelope shape). M45_001 (webhook secret stored as a structured credential under `${secrets.github.webhook_secret}`). M40_001 (the per-zombie thread that consumes the event must exist).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §4.1 (Platform-Ops trigger modes), §8.3 (webhook trigger), §8.5 (E2E walkthrough — webhook flow), §12 step 7a.

---

## Implementing agent — read these first

1. `docs/ARCHITECHTURE.md` §8.5 — the GH Actions trigger walkthrough is the worked example.
2. M42's spec (sibling) for the EventEnvelope shape — DO NOT invent a new envelope.
3. GitHub's webhook docs: HMAC SHA-256 signature in `X-Hub-Signature-256` header, payload signed with workspace's webhook secret.
4. `src/http/handlers/zombies/steer.zig` — mirror its HMAC verification pattern (if any) or borrow from the Svix integration if present.
5. `samples/platform-ops/SKILL.md` — what the zombie expects to reason over when actor=webhook:github (the SKILL.md prose teaches it to fetch GH run logs first).

---

## Overview

**Goal (testable):** A configured GitHub repo posts a `workflow_run` event with `conclusion=failure` to `POST /v1/workspaces/{ws}/zombies/{id}/webhooks/github`. The receiver verifies the `X-Hub-Signature-256` HMAC against the workspace's stored `github.webhook_secret`. On valid signature: normalize the payload into an EventEnvelope (M42 shape) with `actor=webhook:github`, `event_type=webhook`, `request.message=<short summary>` and `request.metadata={run_id, run_url, head_sha, conclusion, ref, repo, attempt}`. XADD to `zombie:{id}:events`. Return 202 within 100ms. The zombie's per-zombie thread (M40) picks up the event, processes it via M42's processEvent + M41's executor session. The agent reasons, fetches GH run logs via `http_request` (the GH API token is in the vault as `${secrets.github.api_token}`), correlates, posts to Slack.

**Problem:** No HTTP receiver exists today for any external webhook. M42 owns the event stream + history but not ingest. The GH Actions wedge is structurally unbuildable until this lands.

**Solution summary:** A single ingest endpoint per source. v1 ships GitHub-specific: `/webhooks/github`. The endpoint handler is thin: verify signature, normalize, XADD, 202. All semantics (which event types matter, what the agent does with them) live in the SKILL.md — not in the receiver. Generic enough to add `/webhooks/gitlab`, `/webhooks/bitbucket` later by mirroring the receiver pattern.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/webhooks/github.zig` | NEW | GH-specific receiver: verify signature, normalize, XADD |
| `src/http/handlers/zombies/webhooks/common.zig` | NEW | Shared helpers: HMAC verification primitive, envelope-write helper |
| `src/http/router.zig` | EXTEND | Register `/webhooks/github` route under `/v1/workspaces/{ws}/zombies/{id}/` |
| `src/zombie/webhook_normalizer/github.zig` | NEW | GH `workflow_run` payload → EventEnvelope normalizer |
| `samples/platform-ops/TRIGGER.md` (or merged frontmatter — wait for M46) | EXTEND | Add `trigger.webhook.github: enabled` flag (or equivalent under `x-usezombie:`) |
| `samples/platform-ops/SKILL.md` | EXTEND | Add a paragraph teaching the agent: when actor=webhook:github, first fetch GH run logs via `http_request GET https://api.github.com/repos/{repo}/actions/runs/{run_id}/logs` |
| `tests/integration/webhook_github_test.zig` | NEW | E2E: post signed payload → assert event lands → assert zombie processes it |
| `samples/fixtures/m43-webhook-fixtures/github_workflow_run_failure.json` | NEW | Real GH webhook payload structure (anonymized) |
| `samples/fixtures/m43-webhook-fixtures/github_workflow_run_success.json` | NEW | Used to test "ignore success" filtering |

---

## Sections (implementation slices)

### §1 — Endpoint + signature verification

`POST /v1/workspaces/{ws}/zombies/{id}/webhooks/github`

```
1. Read raw request body (DO NOT parse JSON yet — signature is over raw bytes)
2. Read X-Hub-Signature-256 header → expected = "sha256=" + hex(HMAC-SHA256(secret, body))
3. Resolve workspace_id from path → fetch zombie config → resolve secrets.github.webhook_secret from vault
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
- Use `delivery_id` as a stable component of a hash → write to a Redis key `webhook:dedupe:github:<delivery_id>` with EX 86400 (24h)
- If `SET NX` fails, return 200 OK with `{"deduped": true, "original_event_id": <previous>}` — don't 4xx GH (they'd retry).

**Implementation default**: 24h dedupe window matches GH's retry window. After that, treat as new.

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
  POST /v1/workspaces/{ws}/zombies/{id}/webhooks/github
       headers: X-Hub-Signature-256, X-GitHub-Event, X-GitHub-Delivery
       body: raw GitHub webhook payload
       → 202 Accepted { event_id }
       → 200 OK { deduped: true, original_event_id }
       → 204 No Content { ignored: "<event-type>" }   for non-workflow_run or non-failure
       → 401 Unauthorized                              on signature mismatch
       → 413 Payload Too Large                         for >1MB body

Vault credential shape (M45 structured):
  github = {
    api_token: "<PAT with read scope on Actions>",
    webhook_secret: "<HMAC secret matching repo's webhook config>",
  }

Redis dedupe key:
  webhook:dedupe:github:<delivery_uuid>  EX 86400
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

Fixtures in `samples/fixtures/m43-webhook-fixtures/`.

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 9 tests above
- [ ] Manual smoke: configure a real GH repo's webhook to point at staging zombid, push a failing deploy, observe zombie posts diagnosis to Slack within 30s
- [ ] Replay attack test: capture a real signed payload, replay 24h later → second is treated as new (post-window) but inside 24h is deduped
- [ ] `make memleak` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
