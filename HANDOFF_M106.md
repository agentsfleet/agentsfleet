# HANDOFF — M106 Slack-resident channel bot

**Ephemeral.** Briefs the next agent; delete at CHORE(close), never commit to the PR.
**Updated:** Jul 01, 2026 (evening) · **Worktree:** `/Users/kishore/Projects/agentsfleet-m106-slack-resident` · **Branch:** `feat/m106-slack-resident`

The **canonical decision + design log lives in the spec's Discovery section** (bottom of `docs/v2/active/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`) — read it first; this file is the fast-path summary.

---

## Scope / Status

Milestone **M106_001** — `@agentsfleet` Slack-resident channel bot (`Status: IN_PROGRESS`).

- ✅ **§1 OAuth install** (Dims 1.1/1.2) · **§2 signed events ingress** (2.1/2.2/2.3) · **§3 materialization core** (3.1/3.3) — all done pre-this-session.
- ✅ **§3 Dim 3.2** — concurrent first-mention convergence test. **§3 fully closed.** (`90aab182`)
- ✅ **§4 activation** — resident fleets were born `installing` and never leased (runner leases only `active` fleets). Added shared `fleet_row.activateFleetOnConn` (guarded idempotent `installing→active`); `channel_fleet` activates **inline before writing the binding** (Invariant 10: binding ⇒ leaseable). Extracted the request-independent row-write primitives (`insertFleetOnConn`/`activateFleetOnConn`/`deleteFleetRow`/`isUniqueViolation`) into **`fleet_row.zig`** (RULE FLL — the new fn tipped `create.zig` over 350; also removed a create↔create_install_steps import cycle). (`af22e14b`)
- ✅ **§4 connector:outbound subsystem** — the generic, provider-routed answer-delivery subsystem (Indy-acked scope decision, see below). (`7e52d308`) Files:
  - `queue/connector_outbound.zig` — durable Redis stream ops (`ensureGroup`/`enqueue`/`readNext[BLOCK]`/`readPending[own-PEL]`/`ack` + OOM-safe RESP decode; mirrors `redis_fleet.zig`).
  - `http/handlers/connectors/outbound/worker.zig` — the **sole** provider-router; boot-started in `serve_background` (like the sweepers, not a daemon); pending-first restart-redelivery via a stable consumer id + blocking new-read; `switch(provider)→slack`; bounded retry, ack-drop on exhaustion.
  - `connectors/slack/post.zig` — `chat.postMessage`: bot token from the `(workspace_id,'fleet:slack')` vault handle via `vault.loadJson`, channel+thread from the event's `request_json`, 200-ok/429/5xx → `delivered`/`permanent`/`retryable`, `UZ-SLK-030`.
  - `fleet/service_report.finalize` — one guarded best-effort `enqueueOutboundAnswer` (fleet_id→provider reverse lookup on the **existing** `idx_connector_channels_fleet_id`; **no migration 032 needed**). **Invariant 9:** `fleet/` enqueues a provider-tagged generic job, never imports `connectors/`.
- ✅ **§4 Dim 4.1** — answer-delivered-in-thread e2e (`7bcb59de`). `connectors/slack/outbound_integration_test.zig` has a **capturing FakeSlack loopback** (reads the outbound `chat.postMessage` body + answers `{ok:true}`) and proves delivery at two levels: `slack_post.deliver` directly AND the full `connector_outbound.enqueue → worker.run → FakeSlack`, asserting the captured body carries channel + `reply_thread_ts` + answer. **Reuse this FakeSlack pattern for 4.2/4.3/E tests.**
- ⏳ **§4 remaining:** **E** (thread re-fetch) + **F remaining** (Dim 4.2 memory round-trip, Dim 4.3 thread-context-transient) — see NEXT STEPS.
- ⏳ **§5** dashboard Connect-Slack connector · **§6** playbooks + arch docs · **CHORE(close)** — not started.

## Working tree / commits

`git status` clean except this `HANDOFF_M106.md` (untracked, ephemeral). Branch **pushed**, origin == `7bcb59de`. **PR #468 OPEN** — https://github.com/agentsfleet/agentsfleet/pull/468 — **DO NOT MERGE** (incremental; the bot ingests + materializes + activates + has the whole delivery subsystem, but the mention→answer e2e isn't test-proven yet and §5/§6 are pending). New commits this session (on top of the prior §1–§3 stack):

```
7bcb59de test(m106): §4 Dim 4.1 — answer delivered in-thread (plumbing e2e)
d70ee56d docs(m106): record §4 subsystem-built + test-strategy decisions
7e52d308 feat(m106): §4 connector:outbound subsystem + Slack answer poster
af22e14b feat(m106): §4 activate resident fleet inline for leasing
90aab182 test(m106): §3 Dim 3.2 — concurrent first-mention converges on one fleet
```

**CI (PR #468):** was green except `test-integration`, which fails ONLY on the 2 pre-existing `tenant_billing` shared-drift flakes (pass in isolation, identical on `main` — NOT a regression, do NOT chase). The 3 new pushes each passed the pre-push gate; re-check greptile + CI on the latest.

## Running infra (reuse it)

Docker `agentsfleet-postgres` + `agentsfleet-redis` UP + healthy. Redis CA at `.tmp/redis-ca.crt`.

### Single integration-test recipe
```bash
DB="postgres://agentsfleet:agentsfleet@localhost:5432/agentsfleetdb?sslmode=disable"
LIVE_DB=1 TEST_DATABASE_URL="$DB" \
  TEST_REDIS_TLS_URL="rediss://:agentsfleet@localhost:6379" \
  REDIS_URL_API="rediss://:agentsfleet@localhost:6379" \
  REDIS_TLS_CA_CERT_FILE="$PWD/.tmp/redis-ca.crt" \
  zig build test -Dtest-filter="<substr of the test name>" --summary all
```
`-Dtest-filter` is name-based but pulls in a whole compile-unit's tests (a filter unique to your test → +N tests over the ~28 baseline; 0 failures = green). `zig build` (native) is the fast compile check; `zig fmt <file>` auto-fixes formatting. DB reset if billing drifts: `make _reset-test-db && DATABASE_URL_MIGRATOR="$DB" zig build run -- migrate`.

---

## 🔒 LOCKED DECISIONS — do NOT relitigate

The **6 original** (namespace `/v1/connectors/slack/*`; Option-C shared connector state; admin-vault secrets; GitHub stays its App flow; reactive config = one `api` trigger; reuse `insertFleetOnConn`) + **new this session:**

7. **§4 answer delivery = a GENERIC `connector:outbound` subsystem, built inside M106** (Indy: *"Build connector:outbound in M106 now"*). Not a Slack-only post. One boot-started provider-routed consumer thread (NOT a separate daemon); `fleet/` never imports `connectors/` (Invariant 9). Grafana/Jira/Linear later = one `post.zig` + one `switch` arm.
8. **The runner is isolated** (nullclaw links as a library, runs in a sandboxed forked child with no network/token; `src/runner/daemon/lease_run.zig`). It CANNOT post to Slack — the answer surfaces server-side ONLY at `service_report.finalize` (`body.response_text`), which is where delivery is triggered.
9. **§4 test strategy = plumbing tests + behavioral eval** (Indy: *"build the plumbing tests first"*). The `TestHarness` has **no runner**, so a full mention→LLM→answer e2e is not harness-reproducible. Automated integration proves the PLUMBING (delivery via FakeSlack, memory round-trip, thread-transient); Dim 4.4's model-behavior half (fresh in-thread value overrides memory) is a documented STAGING eval, not a harness test.
10. **`/reports` rename is OUT of M106** — a separate ticket. Indy flagged `POST /v1/runners/me/reports` as poorly named (it means "submit the terminal result of a completed execution"); rename target `POST /v1/runners/me/leases/{lease_id}/result`. It is a shipped runner↔server wire endpoint used by EVERY fleet (renaming breaks the deployed runner + the `protocol` module + tests) → its own small PR, NOT here.

---

## 🔜 NEXT STEPS (in order)

1. **§4 E — thread re-fetch** (`events.zig`). `recent_thread_msgs[]` is emitted EMPTY by `events.zig:buildRequestJson`. Populate it at INGRESS (best-effort, Indy OK'd ingress placement) via Slack `conversations.replies` (GET `{api_base}/conversations.replies?channel=&ts=&limit=N`, bounded last-N) using the per-install bot token (`vault.loadJson` on `(workspace_id,'fleet:slack')`, key via `credential_key.allocKeyName(alloc,"slack")`) + `hx.ctx.connector_slack_api_base_override orelse post.SLACK_API_BASE_DEFAULT`. Reuse the `std.http.Client.fetch` + Allocating-writer pattern from `post.zig`/`oauth2.exchange`. **Degrade to empty on any failure** (answer still works from durable memory + mention text); dedup (`event.ts`) makes a slow-retry safe. Consider a bounded timeout so a slow Slack call can't blow the 3 s ack.
2. **§4 F — remaining plumbing tests** (Dim 4.1 ✅ DONE). Reuse the **capturing FakeSlack** already built in `slack/outbound_integration_test.zig` (a `fleet_events.uid` needs a v7-shaped UUID — the CHECK bit once). **4.2:** POST then GET `/v1/runners/me/memory/{channel_fleet_id}` → assert a fact stored under the channel scope is recalled (memory reused unchanged). **4.3:** assert `recent_thread_msgs` reaches `request_json` but nothing from it lands in `memory.memory_entries` (pairs with E). **4.4** behavioral half → document as a staging eval in the spec Acceptance/Eval section (not a harness test).
3. **§5** dashboard Connect-Slack connector (flip `ui/packages/app/lib/integrations/catalog.ts` Slack card to OAuth; `SlackConnectorRow` mirroring `GithubConnectorRow` + `startSlackConnectAction`; "Slack connected: {team}"; `connector:write`). Dim 5.1.
4. **§6** playbooks (`slack_app_registration`, `github_app_registration`) + arch docs (`high_level`/`user_flow`/`data_flow`/`direction`/`roadmap` forward-marked + `scenarios/slack-channel-resident.md`). Dims 6.1–6.3.
5. **CHORE(close):** `/write-unit-test` + `/review` + `gh pr create` already-open so `/review-pr` + `kishore-babysit-prs`; move spec `docs/v2/active/→done/` + `Status: DONE`; changelog `<Update>` in `~/Projects/docs` + revise affected docs pages; PR Session Notes; delete THIS file; working tree clean → PR #468 merge-ready.

---

## ⚠️ Zig 0.16 traps (all bit this milestone)

- `errdefer` does NOT fire on `return null` from a `?T` (optional) fn — free explicitly on all paths.
- `std.Io.Writer.Allocating` body → read via `aw.toOwnedSlice()`, never the seed `ArrayList`; on a non-error-union fn, manage `aw.deinit()` by hand (no errdefer fires).
- Computed response-header values → allocate on `hx.res.arena`, not `hx.alloc` (dispatch arena, freed before httpz writes).
- `conn.query()` results must drain (scope the `PgQuery` + `defer q.deinit()`) before the next query/exec on the same conn, or `error.ConnectionBusy`.
- Gates that bit this session (all mechanical, fixed): **FLL** (`create.zig` >350 → split to `fleet_row.zig`), **ZLint unused-import** (`pg` in `create.zig`), **UFS** (repeated Redis command tokens → named consts). Run `zig fmt <file>` + `zig build` before each commit; the pre-commit hook runs harness-verify + lint + test-auth.

## Deferred finding to raise with Indy (still open)

`queue/redis_client.zig` `setNx` conflates a Redis server-error reply with "key exists" → a fresh mention could 200-ack as duplicate + drop silently under Redis write-degradation (OOM/read-only). **Pre-existing, identical in the webhook producer, out of M106 scope.** Recommended: separate queue-layer PR. Surfaced to Indy at pickup; still awaiting fix-now-vs-ticket call (they did not object to "separate ticket").
