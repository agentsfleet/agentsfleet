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

# M180_001: Signed ingress, cron, and connectors — the event producers serve from Rust

**Prototype:** v2.0.0
**Milestone:** M180
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** PENDING
**Priority:** P1 — trigger-plane parity; the Zig daemon keeps serving production while this lands
**Categories:** API
**Batch:** B5 — after M178 (approvals + workspace surface it feeds)
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M178_001 (approvals, workspace event surface); M177_001 (fleet services); M176_001 (substrate)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/data_flow.md` §B. TRIGGER (six producers, one ingress) + `docs/architecture/connectors.md`

---

## Overview

**Goal (testable):** every signature-verified ingress route (fleet webhooks + approval + GitHub, Svix, QStash schedule fire, connector callbacks, Slack events, Clerk identity events), the schedules surface (CRUD + `:sync`) with its Upstash QStash (external cron provider) sync service, and the connector outbound worker serve from `agentsfleetd-rs` with signature verdicts, rejection codes, replay suppression, and stream writes equal to the Zig daemon.
**Problem:** the trigger plane is the daemon's unauthenticated-edge: HMAC (hash-based message authentication code) verification, timestamp windows, and replay suppression are the only wall between the internet and `XADD fleet:{id}:events` — a port defect here is a security defect, and cron double-fires or lost webhooks corrupt the "operational outcomes do not fall into limbo" promise.
**Solution summary:** port the four signature middlewares (webhook signature, webhook HMAC, Svix, trusted-client-IP) with constant-time compares, the ingress handler groups, the schedules store + QStash client + sync service, the connector callback relay/complete pair + Slack events, and the outbound answer worker as a supervised task — graded by signature fixture matrices and the integration subset.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): signed ingress, schedules + QStash, connectors
- **Intent (one sentence):** every external event producer — webhook, cron, connector — lands events through `agentsfleetd-rs` with the same signatures accepted, the same forgeries rejected, and the same stream entries written.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/data_flow.md` §B. TRIGGER — the six producers on one ingress, entry-id-as-event-id, the three webhook rejection codes (UZ-WH-020 misconfig · UZ-WH-010 bad signature · UZ-WH-011 stale timestamp, 5-minute window), and QStash replay suppression.
2. `src/agentsfleetd/crypto/hmac_sig.zig` — the canonical HMAC construction (single source; scrubbed key pads) the Rust canon must match bit-for-bit.
3. `src/agentsfleetd/auth/middleware/` — `webhook_sig.zig`, `webhook_hmac.zig`, `svix_signature.zig`, `trusted_client_ip.zig` — verification order and failure codes.
4. `src/agentsfleetd/cron/` — `Service.zig`, `Store.zig`, `QStashClient.zig`: the daemon owns no timer; QStash calls back in, signature-verified.
5. `src/agentsfleetd/queue/` outbound worker + `docs/architecture/connectors.md` — connector answer delivery semantics.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/src/afd_api/**` | EDIT | Route variants + handlers: `/v1/webhooks/{fleet_id}[/approval|/github]`, `/v1/webhooks/svix/{fleet_id}`, `/v1/ingress/{provider}`, `/v1/ingress/qstash/schedules`, `/v1/connectors/{provider}/callback` (GET relay / POST complete), `/v1/connectors/slack/events`, `/v1/auth/identity-events/clerk`, workspace+fleet `/schedules[/{schedule_id}[:sync]]` |
| `rustd/src/afd_auth/**` | EDIT | the four ingress middlewares with constant-time verification |
| `rustd/src/afd_cron/**` | CREATE | schedules store, QStash client, sync service, fire-queue handling |
| `rustd/src/afd_connectors/**` | CREATE | connector callback flows, Slack event handling, outbound answer worker |
| `rustd/src/agentsfleetd/**` | EDIT | outbound worker joins the supervisor |
| `rustd/Cargo.toml` | EDIT | new members |
| `make/test-integration.mk` | EDIT | ingress/cron/connector subset against the Rust binary |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — CTM (every signature compare constant-time — the load-bearing rule of this milestone), PRI (webhook and Slack payloads are untrusted input), TIM (the 5-minute timestamp window is an explicit named invariant), ECL (provider outage ≠ bad signature), NSQ, UFS, NDC, TST-NAM, MSID, ERR (UZ-WH-* referenced), FLS, FLL.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — for the schedules CRUD surface.
- `dispatch/write_rust.md` — deterministic replay/race tests; REVIEW cites Microsoft guideline mnemonics.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | one middleware per module; handler-per-provider |
| LOGGING | yes | rejected ingress logs code + provider, never payload bytes or signatures |
| MILESTONE-ID | yes | none in source/tests |
| UFS | yes | window widths, header names, provider ids as named constants |
| SCHEMA GUARD | no | no schema change |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/` ingress/cron/connector trees (Zig daemon) — behaviour and code source of truth; `crypto/hmac_sig.zig` is the byte-level oracle for the signature canon.
- **Reference:** M176 afd_crypto — the HMAC canon lands there once; this milestone consumes it (no second implementation — RULE UFS/OWN).
- **Reference:** `docs/architecture/data_flow.md` §B. TRIGGER — the invariant table is the acceptance oracle for rejection codes and replay behaviour.

## Sections (implementation slices)

### §1 — Signature middlewares

`webhook_sig`, `webhook_hmac`, `svix_signature`, `trusted_client_ip` as tower layers over the afd_crypto canon: constant-time compares, the three rejection codes (UZ-WH-020 / UZ-WH-010 / UZ-WH-011), the 5-minute timestamp window, Svix's scheme (id.timestamp.payload, base64 secrets, multiple signatures header).

- **Dimension 1.1** — signature matrix per middleware: valid passes; wrong key, tampered body, missing header, malformed header each → the documented code → Test `test_signature_matrix_per_middleware`
- **Dimension 1.2** — timestamp window: 4m59s accepted, 5m01s → UZ-WH-011; skew in both directions → Test `test_timestamp_window_bounds`
- **Dimension 1.3** — verification is constant-time (no early-return on first mismatched byte — structural assertion on the compare path) → Test `test_signature_constant_time_compare`
- **Dimension 1.4** — unconfigured webhook secret → UZ-WH-020, never a verify attempt → Test `test_unconfigured_secret_code`

### §2 — Webhook ingress handlers

`/v1/webhooks/{fleet_id}` (+`/approval`, `/github`), `/v1/webhooks/svix/{fleet_id}`, `/v1/ingress/{provider}`, `/v1/auth/identity-events/clerk` — verified payloads become `XADD fleet:{id}:events` entries (entry id IS the event id) with `INSERT … ON CONFLICT DO NOTHING` idempotency; the approval webhook resolves gates through the M178 approvals service.

- **Dimension 2.1** — verified webhook → one stream entry + one durable row; replayed delivery → zero new rows (idempotent) → Test `test_webhook_ingest_idempotent`
- **Dimension 2.2** — approval webhook resolves the gate exactly as the M178 surface does (one continuation row) → Test `test_approval_webhook_continuation`
- **Dimension 2.3** — GitHub-flavored payload parsing parity on a fixture corpus (deliveries the Zig daemon accepts/rejects) → Test `test_github_webhook_corpus_parity`
- **Dimension 2.4** — Clerk identity events mutate the same identity state as the Zig daemon on fixture events → Test `test_clerk_identity_event_parity`
- **Dimension 2.5** — every route + method in this spec's Interfaces inventory exists in the Route enum; extras and gaps both fail → Test `test_route_inventory_matches_interfaces`

### §3 — Schedules and QStash

Workspace+fleet `/schedules[/{schedule_id}[:sync]]` CRUD, the schedules store, the QStash client (create/update/delete upstream schedules), the sync service (`:sync` reconciles), and the fire path `/v1/ingress/qstash/schedules`: signature verified at ingress, replay suppressed atomically, the daemon owns no timer.

- **Dimension 3.1** — schedule CRUD + `:sync` reconciliation parity (store rows + upstream calls recorded against a QStash fake) → Test `test_schedule_sync_reconciles`
- **Dimension 3.2** — schedule fire: verified callback → event append; duplicate fire (same delivery) suppressed atomically under concurrency → Test `test_schedule_fire_replay_suppressed`
- **Dimension 3.3** — QStash outage during CRUD → typed retryable error; store and upstream never diverge silently (sync repairs) → Test `test_qstash_outage_sync_repair`

### §4 — Connectors and Slack

`/v1/connectors/{provider}/callback` (GET relay / POST complete) finishing OAuth (Open Authorization) grants into the vault via M176 crypto; `/v1/connectors/slack/events` (URL-verification challenge + event deliveries); workspace `/connectors[/{provider}[/connect]]` surface parity.

- **Dimension 4.1** — callback relay/complete: grant lands in the vault under the provider key name; states/nonces validated → Test `test_connector_callback_grant`
- **Dimension 4.2** — forged/expired callback state → rejected, no vault write → Test `test_connector_callback_rejects_forged`
- **Dimension 4.3** — Slack URL-verification answered; signed events accepted; bad Slack signature rejected → Test `test_slack_events_verification`

### §5 — Outbound answer worker

The connector outbound queue worker as a supervised task: delivers fleet answers to connector destinations with jittered retry/backoff, failure accounting, and clean shutdown (joins on stop; in-flight delivery completes or re-queues).

- **Dimension 5.1** — queued answer delivered once; destination 5xx → retry with backoff then documented terminal handling → Test `test_outbound_delivery_retry`
- **Dimension 5.2** — shutdown mid-delivery: task joins; no lost or double-delivered answer → Test `test_outbound_shutdown_no_loss`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 (first, alone) | §1 middlewares | Claude Code · Fable 5 · xhigh | the security wall; everything else consumes it |
| B2 | §2 webhooks | Claude Code · Opus 5 · high | idempotency + fixture corpus, well-oracled |
| B2 | §3 schedules/QStash | Claude Code · Opus 5 · high | reconciliation logic with a fake-upstream oracle |
| B2 | §4 connectors/Slack | Codex · GPT 5.6 tera · high | vendor flows with crisp fixtures |
| B3 | §5 outbound worker | Claude Code · Opus 5 · high | supervised-task semantics from the M176 pattern |

Indy decides how many agents actually spin per batch.

## Interfaces

```
Ingress routes (per src/agentsfleetd/http/route_template.zig):
  POST /v1/webhooks/{fleet_id}[/approval|/github] · /v1/webhooks/svix/{fleet_id}
  POST /v1/ingress/{provider} · /v1/ingress/qstash/schedules
  GET|POST /v1/connectors/{provider}/callback · POST /v1/connectors/slack/events
  POST /v1/auth/identity-events/clerk
  /v1/workspaces/{id}/fleets/{fleet_id}/schedules[/{schedule_id}[:sync]]
Rejection codes: UZ-WH-020 (misconfig) · UZ-WH-010 (bad signature) ·
UZ-WH-011 (stale timestamp, 5-minute window) — existing registry, referenced.
Stream write: XADD fleet:{id}:events — entry id IS the canonical event id.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Forged signature | attacker or key rotation skew | constant-time verify → UZ-WH-010; no stream write, no payload logging |
| Stale delivery | provider retry after window | UZ-WH-011; provider retries per its policy |
| Unconfigured secret | misconfigured fleet | UZ-WH-020 before any verify work |
| Webhook replay | provider at-least-once delivery | idempotent insert + entry-id dedup → zero duplicate rows |
| Cron double-fire | QStash retry race | atomic suppression; exactly one event row |
| QStash outage | vendor down | typed retryable on CRUD; `:sync` reconciles when back |
| Slack retry storm | slow handler | fast-ack semantics parity; events processed idempotently |
| Connector destination down | outbound 5xx | jittered backoff retries; terminal handling parity + failure accounting |
| Forged OAuth callback | CSRF (cross-site request forgery)-style state reuse | state/nonce validation → rejected, no vault write |

## Invariants

1. Every signature compare on this surface is constant-time via the afd_crypto canon — one implementation, no per-handler crypto — `test_signature_constant_time_compare` + OWN review.
2. Nothing writes `fleet:{id}:events` on this surface without a passed signature verdict — middleware ordering enforced by the Route metadata; `test_signature_matrix_per_middleware`.
3. The daemon owns no timer — schedule firing arrives only via verified QStash callbacks; enforced by afd_cron exposing no scheduler task; `test_schedule_fire_replay_suppressed`.
4. Replay is idempotent end-to-end (webhook + cron): duplicate deliveries produce zero additional durable rows — `test_webhook_ingest_idempotent`.
5. Rejected ingress logs code + provider only — never payload bytes, signatures, or secrets — LOGGING gate + log-capture assertions in the signature tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| ingress accept/reject counters (existing families) | ops | each verdict | provider, code, fleet id | no payloads/signatures | `test_signature_matrix_per_middleware` |
| outbound delivery outcome counter (existing) | ops | delivery attempt terminal state | provider, outcome, retry count | no message content | `test_outbound_delivery_retry` |

No product-analytics changes (machine-facing ingress; parity port).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit (negative-heavy) | `test_signature_matrix_per_middleware` | 4 middlewares × {valid, wrong key, tampered body, missing header, malformed header} → documented verdict each |
| 1.2 | unit (negative) | `test_timestamp_window_bounds` | ±window-edge fixtures → accept/UZ-WH-011 exactly at the documented boundary |
| 1.3 | unit | `test_signature_constant_time_compare` | compare path uses the canon's constant-time primitive for all input lengths |
| 1.4 | unit (negative) | `test_unconfigured_secret_code` | no secret configured → UZ-WH-020, verify never invoked |
| 2.1 | integration (replay) | `test_webhook_ingest_idempotent` | same delivery twice → one stream entry, one durable row |
| 2.2 | integration | `test_approval_webhook_continuation` | approval delivery → gate resolved + one continuation row |
| 2.3 | unit | `test_github_webhook_corpus_parity` | fixture corpus → accept/reject verdicts equal the Zig daemon's |
| 2.4 | integration | `test_clerk_identity_event_parity` | fixture identity events → same state mutations as Zig |
| 2.5 | unit | `test_route_inventory_matches_interfaces` | Interfaces inventory ⊆ Route enum with methods; extras/gaps named |
| 3.1 | integration | `test_schedule_sync_reconciles` | drifted store vs fake upstream → `:sync` converges both |
| 3.2 | integration (race/replay) | `test_schedule_fire_replay_suppressed` | concurrent duplicate fires → exactly one event row |
| 3.3 | integration (negative) | `test_qstash_outage_sync_repair` | upstream down during CRUD → typed error; later `:sync` repairs |
| 4.1 | integration | `test_connector_callback_grant` | relay→complete → vault row under provider key name |
| 4.2 | integration (negative) | `test_connector_callback_rejects_forged` | reused/forged/expired state → rejected, zero vault writes |
| 4.3 | integration | `test_slack_events_verification` | URL-verification echoed; valid event accepted; bad signature rejected |
| 4.3 (FM) | integration (negative) | `test_slack_fast_ack_parity` | slow downstream processing → ack timing parity with Zig; retried delivery processed idempotently |
| 5.1 | integration (negative) | `test_outbound_delivery_retry` | destination 5xx×N → backoff schedule + terminal handling parity |
| 5.2 | integration (race) | `test_outbound_shutdown_no_loss` | SIGTERM mid-delivery → join; answer delivered once or re-queued |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Signature wall holds (§1) | `cd rustd && cargo test signature` | exit 0 | P0 | |
| R2 | Ingress + replay parity (§2, §3) | `make test-integration` (ingress/cron lane) | exit 0 | P0 | |
| R3 | Connector flows (§4, §5) | `cd rustd && cargo test connector` + `cargo test outbound` | exit 0 | P0 | |
| R4 | Route inventory parity for the ingress groups (§2) | `cd rustd && cargo test test_route_inventory_matches_interfaces` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

**Credential gate (in scope):** this milestone adds the QStash signing keys, the Svix secret, the Slack signing secret, and the connector OAuth client credentials to the family enumeration — same fetch location (`~/.config/agentsfleet/` via `provision-env-1password`); the boot preflight extends to name them.

## Out of Scope

- Cutover, deploy shape, soak — M181.
- New providers, new webhook schemes, or window-width changes — parity only.
- Slack *bot* behaviour beyond event ingress (the reactive bot surface is its own product track).

---

## Product Clarity (authoring record)

1. **Successful user moment** — a deploy fails on GitHub, the webhook lands on staging `agentsfleetd-rs`, the fleet wakes, a scheduled run fires exactly once from QStash, and the connector answer arrives in Slack — the trigger plane is provably interchangeable.
2. **Preserved user behaviour** — providers keep their configured secrets and endpoints; every accept/reject verdict and retry interaction is unchanged.
3. **Optimal-way check** — middleware-first ordering is the direct path: the wall lands and everything behind it is ordinary handler porting.
4. **Rebuild-vs-iterate** — pure port; the one structural change (outbound worker on the M176 supervisor) preserves observable behaviour.
5. **What we build** — four middlewares, ingress handler groups, cron crate, connectors crate, outbound worker.
6. **What we do NOT build** — new event producers, signature-scheme changes, a daemon-owned timer.
7. **Fit with existing features** — feeds M178's event/approval surfaces; must not destabilize the six-producer ingress invariant (`data_flow.md` §B).
8. **Surface order** — N/A — machine-facing ingress; the schedules CRUD follows the existing API shape.
9. **Dashboard restraint** — no UI change.
10. **Confused-user next step** — a rejected webhook reports its documented UZ-WH-* code in the response and the provider's delivery log; existing troubleshooting docs stay accurate.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices with the signature wall isolated first and alone — every later slice consumes its verdict, so its correctness multiplies.
- **Alternatives considered:** running this concurrent with M178/M179 in Batch 4 (rejected: the approval webhook and schedules surface depend on M178's services; a three-way concurrent batch would put the two daemons' shared behaviour in review simultaneously across three PRs); re-implementing HMAC per middleware (rejected: one canon in afd_crypto, RULE OWN/UFS).
- **Patch-vs-refactor verdict:** this is a **refactor** (same behaviour, new runtime) with fixture-matrix discipline on the security wall.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
