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

# M151_001: Steer subscribes to the live tail before it sends the message

**Prototype:** v2.0.0
**Milestone:** M151
**Workstream:** 001
**Date:** Jul 30, 2026
**Status:** DONE
**Priority:** P1 — a fast run can stream its first words before the CLI is listening; those frames are unrecoverable live
**Categories:** CLI
**Batch:** B1 — standalone; no parallel workstream
**Branch:** feat/m151-steer-subscribe-first
**Test Baseline:** unit=3276 integration=501
**Depends on:** none (M150_001 shipped the eager first-frame flush this protects)
**Provenance:** LLM-drafted (Claude Fable 5, Jul 30, 2026) — grounded in a source read of `cli/src/commands/fleet_steer.ts` and `fleet_steer_events.ts`
**Canonical architecture:** `docs/architecture/data_flow.md` §D. WATCH

---

## Overview

**Goal (testable):** `agentsfleet steer` opens its Server-Sent Events (SSE) tail BEFORE posting the message, buffers frames until the `202`'s `event_id` arrives, then replays matching frames — so no frame of the steered event can be published before the subscriber exists.
**Problem:** `steerTurnEffect` posts the message first and subscribes with the returned `event_id` afterward. The activity channel is non-replayable pub/sub, so an unusually fast run — made more likely by the runner's eager first-frame flush — can publish its first frames into a channel nobody is listening to. The CLI then shows an idle wait and recovers only the durable terminal text, never the live opening words. Dashboard chat is unaffected (its stream is open before the user types); this is a CLI-only gap.
**Solution summary:** Reorder one turn of the steer flow: open the event stream first, post the message while the stream is live, hold pre-`event_id` frames in a small bounded buffer, then filter-and-replay once the `202` names the event. Stream-open failure degrades to today's post-then-poll behaviour — subscribing earlier must never make steer less reliable than it is now.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(cli): steer subscribes to the live tail before sending the message
- **Intent (one sentence):** The first words of a fleet's reply always stream live in the terminal, no matter how fast the run starts.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/commands/fleet_steer.ts` — `steerTurnEffect` is the turn being reordered; note the injected `SteerDeps`/`StreamGetFn` seams the tests already use
2. `cli/src/commands/fleet_steer_events.ts` — `tailEventStream` + the terminal-poll fallback; the `event_id` filter this spec moves behind a buffer
3. `cli/test/fleet-steer.integration.test.ts` and `cli/test/fleet-steer-linecov.unit.test.ts` — the harness patterns to extend; mirror their fake-stream injection
4. `docs/architecture/data_flow.md` §D. WATCH and §Two streams + one pub/sub channel — why the channel has no replay and why the durable table is the recovery source
5. `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — the TypeScript discipline for the edited surface

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/commands/fleet_steer.ts` | EDIT | `steerTurnEffect` reordered: stream first, then POST; wires the pre-id buffer |
| `cli/src/commands/fleet_steer_events.ts` | EDIT | tail accepts frames before the event id is known: bounded buffer + filter-and-replay once the id arrives; readiness signal + joined close |
| `cli/src/lib/sse.ts` | EDIT | `onOpen` hook fires when response headers are accepted — the server subscribes before writing SSE headers, so headers-received upgrades subscribe-first from dispatch order to an established subscription |
| `cli/test/fleet-steer-tail.unit.test.ts` | ADD | tail suites (buffer, live path, lifecycle, readiness) — split from the linecov file at the size cap |
| `cli/test/fleet-steer.integration.test.ts` | EDIT | subscribe-before-send order proven through the injected stream |
| `cli/test/fleet-steer-linecov.unit.test.ts` | EDIT | buffer/filter/fallback branch coverage |
| `cli/test/fleet-steer-errors.integration.test.ts` | EDIT | stream-open failure and post-failure paths |
| `cli/test/fleet-steer-repl.unit.test.ts` | EDIT | two `streamSignals` counts re-pinned: they encoded post-then-subscribe, the order this spec flips |
| `docs/v2/active/M151_001_P1_CLI_STEER_SUBSCRIBE_BEFORE_SEND.md` | EDIT (moved from `pending/`) | lifecycle moves, folded-scope amendments, Dimension DONE marks |
| `package.json` + `bun.lock` | EDIT | §4 dependency refresh (folded scope): in-range workspace updates |
| `cli/package.json` + `cli/bun.lock` | EDIT | §4: posthog-node, oxlint, @clerk/testing in-range; playwright pinned 1.62.1 |
| `ui/packages/app/package.json` | EDIT | §4: next 16.2.12 (clears three high advisories), TypeScript 7.0.2 + oxlint-tsgolint 7.0.2001, react 19.2.8, playwright 1.62.1, dev majors; adds `typescript-jsapi` (typescript@6 alias) |
| `ui/packages/app/next.config.ts` | EDIT | §4: `experimental.useTypeScriptCli` — TypeScript 7 has no JavaScript compiler API |
| `ui/packages/app/next-env.d.ts` | EDIT (generated) | §4: regenerated by next 16.2.12's build |
| `ui/packages/app/components/domain/island-dynamic/intent-module-loader.test.ts` | EDIT | §4: bundle-guard AST parser imports the `typescript-jsapi` alias — the compiler API it walks left the typescript@7 package |
| `ui/packages/design-system/package.json` | EDIT | §4: in-range updates + jsdom 30, @testing-library/jest-dom 7 |
| `ui/packages/website/package.json` | EDIT | §4: in-range updates + playwright 1.62.1, jsdom 30, jest-dom 7, size-limit 13 |
| `src/agentsfleetd/fleet_runtime/activity_publisher.zig` | EDIT (comment) | coupling comment names the CLI KIND_* mirror site |
| `docs/architecture/data_flow.md` | EDIT | §D WATCH: CLI steer opens the tail before the POST, established-subscription handshake, pre-id buffer |
| `VERSION` + `build.zig.zon` + `cli/package.json` (version field) | EDIT | 0.26.1 → 0.26.2 via `make sync-version` |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (buffer bounds become named constants, no bare numerics), NDC (no dead code: the buffer and fallback are exercised by tests in the same diff), FLL (file/function caps; `steerTurnEffect` stays within the function cap after the reorder), TST-NAM (test names stay milestone-free)
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — `*.ts` surface: `const` discipline, Bun primitives, no new dependencies

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TS FILE SHAPE | yes — at PLAN | extend the two existing command modules; no new files expected |
| File & Function Length (≤350/≤50/≤70) | yes | `fleet_steer.ts` is at 240 and `fleet_steer_events.ts` at 170; if the buffer pushes either near the cap, extract per the repo's `<module>_<concern>` convention |
| UFS (repeated/semantic literals) | yes | pre-id buffer frame/byte caps are named constants, defined once |
| MILESTONE-ID | yes | source and test identifiers stay milestone-free |
| ZIG / UI / DESIGN TOKEN / LOGGING / SCHEMA | no | no Zig, no UI components, no new log emits, no schema |

## Prior-Art / Reference Implementations

- **Reference:** the dashboard's stream registry (`ui/packages/app/lib/streaming/fleet-stream-registry.ts`) — the platform's existing subscribe-early consumer; mirror its posture (stream open before user action, durable list as recovery), not its browser mechanics. The CLI change stays inside the existing `SteerDeps` injection seams, which is what keeps the tests deterministic.

## Sections (implementation slices)

### §1 — Subscribe first, buffer until the event is named

One steer turn becomes: open the stream → POST the message → buffer every arriving frame until the `202`'s `event_id` is known → drop buffered frames for other events, replay the matching ones in arrival order, then continue the existing tail. **Implementation default:** the buffer lives in the events module beside the tail it feeds, capped by named frame-count and byte constants sized to the pre-id window (one `202` round-trip), because a cap without a name is how UFS violations start.

- **Dimension 1.1** — the stream is open before the POST fires (order proven through the injected stream fake) → Test `test_stream_opens_before_post` — DONE
- **Dimension 1.2** — frames arriving before the `event_id` is known are buffered and replayed in arrival order once it is → Test `test_pre_id_frames_replayed_in_order` — DONE
- **Dimension 1.3** — buffered frames for a different event are dropped, never rendered → Test `test_foreign_event_frames_dropped` — DONE
- **Dimension 1.4** — the pre-id buffer is bounded; overflow drops oldest and the tail still functions (durable poll remains the backstop) → Test `test_pre_id_buffer_bounded` — DONE

### §2 — Failure paths degrade to today's behaviour

Subscribing earlier must never make steer worse than post-then-subscribe. Every new failure path lands on an existing, tested recovery.

- **Dimension 2.1** — stream fails to open → the turn proceeds exactly as today: POST, then terminal poll; the user sees the same outcome shapes → Test `test_stream_open_failure_degrades_to_poll` — DONE
- **Dimension 2.2** — the POST fails after the stream opened → the stream is closed, no orphan connection, the existing error rendering is unchanged → Test `test_post_failure_closes_stream` — DONE
- **Dimension 2.3** — abort (Ctrl-C) during the pre-id window → interrupted cleanly, stream closed, no buffered frames rendered → Test `test_abort_in_pre_id_window` — DONE

### §3 — Existing surfaces unchanged

- **Dimension 3.1** — REPL multi-turn steer: each turn takes the new order; turn boundaries and prompts render as today → Test: existing `fleet-steer-repl.unit.test.ts` suite passes, with its two stream-count assertions updated to the subscribe-first order (every turn opens a tail before its POST, the tail always carries an abort signal for close ownership, and a failed turn's tail is proven closed) — the counts were proxies for the pre-reorder call order this spec exists to flip — DONE
- **Dimension 3.2** — JSON mode output shape (`{ event_id, ...outcome }`) is byte-identical → Test `test_json_mode_shape_unchanged` — DONE

### §4 — Dependency refresh (folded scope — owner-directed mid-stream)

Workspace-wide dependency refresh folded into this workstream by owner instruction (see Discovery). In-range updates across all five package roots; explicit pins for the out-of-range toolchain pieces; the app crosses to TypeScript 7 now that Next.js 16.2.12 ships the backported `experimental.useTypeScriptCli` backend (the Go-native compiler removed `lib/typescript.js`, so the build shells out to the project-local `tsc`).

- **Dimension 4.1** — full unit + lint suites green on the bumped toolchain → Test: `make test-unit-all` and `make lint-all` exit 0 — DONE (final quiet-machine run exit 0; the two earlier full-run reds were load flakes — `admin-models-ui` then the website `App` test, each 100% green in isolation and green in the final run)
- **Dimension 4.2** — the dashboard app production-builds on TypeScript 7 through the local-CLI backend (the Vercel deploy path) → Test: `cd ui/packages/app && bun run build` exit 0 — DONE
- **Dimension 4.3** — the `next` high-severity advisories (Server-Side Request Forgery in Server Actions, middleware/proxy bypass, Denial of Service) are out of range → Test: `bun audit` no longer lists `next` — DONE

## Interfaces

```
No wire or command surface changes. POST /v1/workspaces/{ws}/fleets/{id}/messages,
the SSE stream endpoint, frame shapes, CLI flags, and JSON output are all
unchanged. Only the order of subscribe vs send inside one CLI turn changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stream-open failure | SSE endpoint unreachable / auth rejected | degrade to post-then-poll (today's path); outcome rendering identical; no retry loop added |
| POST failure after subscribe | network / 4xx / 5xx on the message send | close the stream, surface the existing `CliError`; no orphan SSE connection (proven by the fake's close-count) |
| Pre-id buffer overflow | chatty foreign events or a flooded channel during the `202` round-trip | drop-oldest within named caps; matching frames may be lost live but the durable terminal poll still completes the turn |
| Abort mid-window | user Ctrl-C between subscribe and `202` | `failSteerInterrupted` as today; stream closed; nothing rendered |

## Invariants

1. The subscription is established — response headers accepted, which the server only sends after its channel subscribe — before the message POST fires, bounded by `TAIL_OPEN_MAX_WAIT_MS`; a tail that misses the bound is closed unheard and the durable poll alone decides the outcome (a late tail must never pass a truncated reply off as complete). Enforced by the ordering test (marker at headers-accepted behind a real delay) and the ready-timeout test; a regression flips either.
2. No frame belonging to another event is ever rendered — the `event_id` filter applies to buffered and live frames alike (1.3).
3. The pre-id buffer is bounded by named constants — UFS-checked; overflow behaviour is drop-oldest, tested (1.4).
4. Every failure path lands on a pre-existing tested recovery — no new terminal states, no new output shapes (§2, 3.2).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

No new events or log emits: the change is client-side ordering inside one CLI turn. The analytics/funnel playbook needs no update.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_stream_opens_before_post` | injected stream + HTTP fakes record call order → stream-open recorded strictly before POST |
| 1.2 | unit | `test_pre_id_frames_replayed_in_order` | two matching frames arrive pre-id → rendered in arrival order after the id resolves |
| 1.3 | unit (negative) | `test_foreign_event_frames_dropped` | pre-id frames carrying another event id → never rendered, matching frames still replay |
| 1.4 | unit (negative) | `test_pre_id_buffer_bounded` | overflow the named cap pre-id → oldest dropped, no crash, tail continues |
| 2.1 | integration (negative) | `test_stream_open_failure_degrades_to_poll` | stream fake fails to open → POST still sent, terminal poll renders today's outcome |
| 2.2 | integration (negative) | `test_post_failure_closes_stream` | POST fake rejects → stream close-count is 1, existing error surface rendered |
| 2.3 | unit (negative) | `test_abort_in_pre_id_window` | abort signal fires pre-id → interrupted error, stream closed, zero frames rendered |
| 3.1 | regression | existing REPL suite | passes; two stream-count assertions re-pinned to subscribe-first (see Dimension 3.1) |
| 3.2 | regression | `test_json_mode_shape_unchanged` | JSON mode → `{ event_id, ...outcome }` byte-identical to today |
| 4.1 | regression | `make test-unit-all` + `make lint-all` | bumped toolchain → both exit 0 |
| 4.2 | build | `cd ui/packages/app && bun run build` | TypeScript 7 via local-CLI backend → exit 0 |
| 4.3 | audit | `bun audit` | `next` absent from the advisory listing |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Subscribe-before-send proven with buffering and filtering (§1) | `cd cli && bun test 2>&1 \| tail -3` | exit 0 | P0 | ✅ 0 fail — 1466 tests, 156 files |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ every diff path has a table row |
| R3 | App production build on TypeScript 7 (§4, the Vercel path) | `cd ui/packages/app && bun run build 2>&1 \| tail -3` | exit 0 | P0 | ✅ route map emitted, exit 0 (TypeScript 7 local-CLI backend) |
| R4 | `next` advisories cleared (§4) | `bun audit 2>&1 \| grep -c '^next '` | `0` | P0 | ✅ 0 |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ exit 0 — All unit lanes passed (quiet machine; two earlier reds were load flakes, each green in isolation) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ All lint checks passed |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S8 | No oversize source file (test files exempt per RULE FLL's self-audit scope) | `git diff --name-only origin/main...HEAD \| grep -v -E '\.md$\|\.lock$\|\.test\.\|_test\.' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

Pruned standard rows: S3 integration (no daemon HTTP/schema/Redis change; the CLI integration tier runs inside `bun test` via R1), S4 e2e (the CLI integration tests drive the real command path against fakes at the system boundary — the established house tier for this surface), S5 memleak / S6 cross-compile (no Zig touched), S9 orphan sweep (no deletions).

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- **Wake-the-poller** (the 0–1 s lease-pickup gap) — its own follow-up workstream on the daemon lease surface.
- **Dashboard changes** — the browser stream is already open before the user types; nothing to do.
- **Replayable activity channel** — a server-side redesign the platform deliberately rejected (`data_flow.md`: durable history is the recovery source); this spec closes the gap client-side.
- **Changelog `<Update>`** — rides CHORE(close) in `~/Projects/docs` (cross-repo, own-branch flow).
- **Held-back dependency majors (§4)** — `@assistant-ui/react` 0.14→0.15 (pre-1.0 runtime chat surface; breakage would be runtime-visible, not compile-visible), the exact-pinned `@radix-ui/*` set (deliberate pins), and `happy-dom` 20.11.1 (attempted, reverted to 20.10.6: its detached browser now performs real navigation fetches on anchor click, so the sidebar docs link pulls the live docs site and a relative fetch dies on the default `localhost:3000` window origin — crashed the app coverage lane). Each needs its own verified pass.
- **`react-router` advisory** — the React Server Components-mode Cross-Site Request Forgery advisory's patched line sits above the react-router-dom 7.x range this repo can reach in-range; tracked for its own bump decision.

---

## Product Clarity (authoring record)

1. **Successful user moment** — you run `agentsfleet steer` against a warm, fast fleet and the reply's first words stream into your terminal live — never a silent wait that ends with text appearing all at once from the history fallback.
2. **Preserved user behaviour** — every flag, output shape (human and JSON), REPL turn flow, error message, and recovery path renders exactly as today.
3. **Optimal-way check** — the unconstrained optimum is a replayable channel server-side; the platform's settled design says durable history is the recovery source and the live tail is best-effort. Given that, subscribe-first is the complete client-side fix, not a partial one.
4. **Rebuild-vs-iterate** — patch. One turn's ordering plus a bounded buffer inside existing injection seams; no architectural surface moves.
5. **What we build** — the reorder, the bounded pre-id buffer with filter-and-replay, seven new tests, two regression pins.
6. **What we do NOT build** — server replay, dashboard work, retry loops on stream-open failure (the degrade path is today's behaviour by design).
7. **Fit with existing features** — completes M150's eager first-frame flush (the frames it ships early are now always heard); must not destabilize the steer REPL, pinned by §3.
8. **Surface order** — CLI-only by definition of the gap.
9. **Dashboard restraint** — N/A — no UI surface.
10. **Confused-user next step** — unchanged: a steer that misses live frames still completes from the durable event history, and `agentsfleet events` remains the self-serve history view.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one behavioural Section (the reorder + buffer), one failure Section (each new path degrades to a tested recovery), one regression Section — the smallest split where every Dimension is independently verifiable.
- **Alternatives considered:** (a) poll the durable events list immediately after the `202` to backfill missed frames — rejected: adds a read per steer for a window that subscribe-first eliminates outright, and still shows the opening words late; (b) server-side replay buffer on the activity channel — rejected: contradicts the platform's settled eyeballs-vs-audit split and is a daemon redesign for a client-side ordering bug.
- **Patch-vs-refactor verdict:** this is a **patch** because the injection seams (`SteerDeps`, `StreamGetFn`) already support the reordered flow; the defect is call order, not architecture.

## Discovery (consult log)

- **Consults** — Architecture: `data_flow.md` §D WATCH + §Two streams answer the flow; no conflict (the channel is deliberately non-replayable; this spec works with that grain). Origin: surfaced as finding F3 during the M150_001 review (greptile P1 + Codex cross-model pass), recorded in that spec's Discovery with an Indy go-ahead to spec the CLI fix as its own workstream.
- **Metrics review** — no analytics/funnel playbook update required: no signal changes.
- **Folded scope (§4)** — owner-directed mid-stream fold-in of the dependency refresh:
  > Indy (2026-07-30): "just bump typescript playwright and other pacakges and ensure we succeed in test so that gives us confidence in deployment of vercel? does vercel support typescript 7 i heard there was a pull request pending and a blocker?" — context: dependency refresh requested while M151 was in EXECUTE.
  > Indy (2026-07-30): "i want the deps refresh in your old PR, not a new one." then "m151-steer*" — context: fold into `feat/m151-steer-subscribe-first` instead of a separate chore PR.
  The TypeScript 7 blocker Indy referenced is vercel/next.js issue #95490 (`next build` misdetects TypeScript 7 because the Go-native compiler dropped `lib/typescript.js`), fixed by PR #95639 (canary) and backport PR #95831 → stable in Next.js 16.2.12 behind `experimental.useTypeScriptCli`.
  > Indy (2026-07-30): "keep the simple alias for now, as TS 7.1 will sunset the oxc-parser if we write it today." — context: the bundle-guard test's `typescript-jsapi` (typescript@6) alias is the accepted bridge; no oxc-parser rewrite; migrate to the official TypeScript 7.1 API when it ships (grep `typescript-jsapi`).
- **Adversarial review (REVIEW)** — five specialist reviewers + a cross-model Codex pass. Applied: subscribe-first upgraded from dispatch order to an established-subscription guarantee (`onOpen` headers hook + bounded ready-wait; the spec's Goal was overclaimed without it); a timed-out tail is closed unheard and the poll decides (prevents a late tail rendering a truncated reply as complete); `Effect.acquireUseRelease` scopes the tail so every exit closes it, and `close()` joins the stream's settlement; id-less frames are dropped (every daemon frame stamps `event_id`); a buffered `event_complete` beats a late stream error; a cancel before the send suppresses the POST; byte cap measured with `Buffer.byteLength` and pinned by a multi-byte test; stale root `playwright-core` override raised to 1.62.1; daemon coupling comment names the CLI mirror site. False positive rejected with evidence: `@typescript/native` "dead alias" — the app's lint script invokes `node_modules/@typescript/native/bin/tsc`. Deferred for Indy (perf-opportunities discipline): `SseFrame.rawLength` to avoid per-frame restringify, a frame-size cap in `sse.ts` (trusted-server surface), the 60 s tail timer now starting pre-POST rather than post-202, and `linkedAbort`/`streamGet` signal-linking dedup.
- **Spec amendment (EXECUTE)** — Dimension 3.1 originally claimed the REPL suite would pass unmodified. Two of its `streamSignals` assertions turned out to encode the pre-reorder call order (single-shot turns passed no stream signal; a turn whose POST failed never opened a stream). Subscribe-first necessarily changes both, so the assertions were re-pinned to the new invariant and the failed-turn pin strengthened to prove the tail closes. Every other assertion in the suite — dispatch shape, error surfaces, turn continuation — passes untouched.
- **Skill-chain outcomes** — `/write-unit-test`: diff ledger 13/13 resolved (11 tested, 2 won't-test with reasons); negative-path 67%; error-path 100%; red-green proven (order probe fails on the parent commit with the flipped call order, passes on HEAD); one gap closed (already-aborted signal at tail open). `/write-integration-test`: no service-layer tier required — no daemon/schema/Redis surface changed; fleet-scoped pre-subscription is the dashboard's existing live pattern; the injected-boundary bun suite is the house integration tier for the CLI (runs under R1); close-count pin proves no orphan stream connection.
- **Deferrals** — (none)
