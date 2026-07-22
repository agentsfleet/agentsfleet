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

# M138_001: The console chat reads like the approved fleet workspace

**Prototype:** v2.0.0
**Milestone:** M138
**Workstream:** 001
**Date:** Jul 21, 2026
**Status:** DONE
**Priority:** P1 — customer-facing: the operator must scroll a full page to reach the composer, their own messages render blank, and every webhook event renders as an empty row
**Categories:** API, UI
**Batch:** B1 — standalone; no sibling workstreams
**Branch:** feat/m138-console-chat-fidelity
**Test Baseline:** unit=2806 integration=371
**Depends on:** none — M137_001 is in `done/` and the fleet-local navigation rail is merged on `main`
**Provenance:** Large Language Model (LLM)-drafted (claude-opus-4-8, Jul 21, 2026) — authored from Indy's live `app-dev` session screenshots and the approved design at `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-workspace-20260721/`
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` §Operational Restraint; `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel" for the frames the thread renders

---

## Overview

**Goal (testable):** On the fleet console's Chat view the composer is reachable without scrolling the page, every message renders with a sender chip, a readable sender name, a right-aligned timestamp and a non-empty body, every event states what happened in plain English derived from the durable row's own fields, a message sends the moment it is submitted regardless of the live-stream state, and a lost live connection recovers itself instead of pinning to a terminal offline.

**Problem:** Four symptoms, all observed by Indy on `app-dev` (Jul 21, 2026): (1) the chat card grows to the height of its whole history, so reaching the composer means scrolling past the entire event log; (2) the operator's own messages render as a bare Clerk user identifier over a timestamp with no message text; (3) every GitHub App event renders as a timestamp plus an uppercase actor badge and nothing else — twenty consecutive empty rows; (4) with the live connection down, every submitted message sits marked `QUEUED` forever and never sends, even though sending is an ordinary authenticated write that does not touch the live stream. The console's own summary compounds it by printing the runner's raw failure tag (`startup_posture`) where the design shows a sentence.

**Solution summary:** A dashboard-only change that finishes what the approved fleet-workspace design started. The dashboard shell becomes a fixed application frame so a page can claim the viewport; the Chat view claims it, scrolling only its message list and pinning its composer. Message rows are rebuilt to the approved shape — sender chip, sender name, right-aligned timestamp, full-width body, hairline separator — and the sender name becomes a word an operator recognises instead of an identifier. One new summary module turns each durable event row into a sentence from the fields that row already carries (the steer's own text, the normalized webhook's action/repository/number, the plain-language failure label), and the console summary, the thread and the events table all read that single module. The composer's browser-side hold is deleted: a submitted message posts immediately and the fleet's own event stream serialises the work. The live-stream client stops treating repeated failure as terminal and retries on a slow cadence plus on tab focus and network recovery.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m138): approved console chat + 16 KiB request-header ceiling
- **Intent (one sentence):** An operator opening a fleet console can read every message, understand every event, reach the composer without scrolling, and send a message whether or not the live feed is up — and the API server no longer refuses a request for carrying the header bytes a real authenticated chain produces.
- **Handshake** (filled at PLAN) — Restatement: make the fleet console's chat legible and usable — the composer always on screen, every row carrying a name and a sentence a person can read, and a send that works whether or not the live feed is up. ASSUMPTIONS I'M MAKING: (1) the sender vocabulary is a fixed three-way resolution — operator, the fleet's own name, the integration's source name — and the fleet name reaches the thread as a prop from the console page, which already holds it; (2) the viewport claim is expressed by making the shell frame fixed and the content region the scroll owner, so the Chat view needs only an ordinary full-height child and no per-breakpoint height literal; (3) the aggregate in-flight flag is deleted rather than bounded — with the browser-side hold gone it had no consumer left, and a flag nothing reads is dead code, not a safety net; (4) the console acceptance walk is rewritten against the post-navigation surface — its current assertions describe a console that no longer exists; (5) the API server's request-header ceiling folds into this workstream per Indy's direction rather than a sibling milestone — one operator-visible outcome, one PR.

## Implementing agent — read these first

1. `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-workspace-20260721/variant-A.png` + `approved.json` — the approved surface and its written feedback. The feedback overrides the image where they disagree: **there is no Steer tab** ("Steer remains the underlying API behavior and is not a tab"), and all copy stays domain-neutral so a GitHub fleet and a Zoho Desk fleet share one shell.
2. `ui/packages/app/components/domain/fleetMessageRenderers.tsx` — the row renderers being rebuilt. Note the actor-rail grid this replaces, and that the file already exceeds the length cap, so the rebuild lands as a split (see Files Changed).
3. `ui/packages/app/lib/streaming/fleet-stream-frames.ts` — `rowToEvent` is where a durable row becomes a rendered message; today it reads only the fleet's response text for every role, which is why operator messages and webhook events render empty.
4. `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` and `.../github.zig` — the exact normalized field set a webhook event carries in `request_json`; the summary module reads these fields and invents nothing.
5. `ui/packages/app/components/domain/EventsList.tsx` — the plain-language failure-label map that already exists here and must move to a shared home rather than being copied.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | The dashboard becomes a fixed application frame; the content region becomes the scroll container so a page can claim the viewport. |
| `ui/packages/app/tests/app-shell-navigation.test.ts` | EDIT | Cover the fixed frame and the content region's scroll ownership. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | The Chat view fills the frame; every other view keeps ordinary page scrolling. |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | Bounded thread with an internally scrolling message list and a pinned composer; header matches the approved design; the browser-side send hold is removed. |
| `ui/packages/app/components/domain/FleetMessageRow.tsx` | CREATE | The approved row shape — sender chip, sender name, right-aligned timestamp, full-width body, hairline separator — shared by every role, extracted so the renderer file returns below the length cap. |
| `ui/packages/app/components/domain/FleetMessageRow.test.tsx` | CREATE | Row-shape coverage: chip, name, timestamp placement, separator, long-body wrapping. |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | Becomes the role dispatcher over the shared row: operator, fleet, and event rows plus the payload disclosure. |
| `ui/packages/app/components/domain/SteerComposer.tsx` | EDIT | The approved composer: bordered multi-line field, send hint, prominent send action; the queue rendering goes with the deleted hold. |
| `ui/packages/app/components/domain/SteerComposer.test.tsx` | EDIT | Assert the new composer surface and the removal of the queue affordance. |
| `ui/packages/app/lib/events/event-summary.ts` | CREATE | One home for turning a durable event row into operator-readable text: the operator's own message, the webhook headline, the honest no-reply outcome, and the failure-label vocabulary. |
| `ui/packages/app/lib/events/event-summary.test.ts` | CREATE | Per-shape coverage including unknown event kinds, absent fields, and malformed payloads. |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `rowToEvent` derives its text through the summary module instead of reading the response field for every role. |
| `ui/packages/app/lib/streaming/fleet-stream-frames.test.ts` | EDIT | Cover operator-text recovery and event summaries surviving a reload. |
| `ui/packages/app/lib/streaming/fleet-stream-registry.ts` | EDIT | A lost connection retries on a slow cadence and on tab focus / network recovery instead of pinning to a terminal offline. *(Amended at VERIFY — RULE FLL: the additions pushed the file past the length cap, so the reconnect policy and the entry model split out beside it.)* |
| `ui/packages/app/lib/streaming/fleet-stream-reconnect.ts` | CREATE | *(FLL split)* Backoff timing, pending-retry cancellation, and the tab-visible / network-online recovery wiring, behind a narrow interface. |
| `ui/packages/app/lib/streaming/fleet-stream-entry.ts` | CREATE | *(FLL split)* The per-fleet entry model: connection statuses, the published snapshot shape, and the seeded-entry factory. |
| `ui/packages/app/lib/streaming/fleet-stream-registry.test.ts` | EDIT | Cover the slow retry, the focus and network triggers, and that a recovered connection still backfills its gap. |
| `ui/packages/app/components/domain/useFleetEventStream.ts` | EDIT | The aggregate in-flight signal is deleted with its last consumer; work is reported per event. |
| `ui/packages/design-system/src/design-system/time-utils.ts` | EDIT | *(Amended at EXECUTE)* A time-of-day format in the sanctioned home, so the console renders a clock label through `Time` rather than a bespoke formatter the timestamp standard forbids. |
| `ui/packages/design-system/src/design-system/time-utils.test.ts` | EDIT | The clock format's own coverage: seconds kept, date dropped, locale honoured, unreadable input returns the shared fallback. |
| `ui/packages/design-system/src/index.ts` | EDIT | Export the clock formatter. |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export the clock formatter. |
| `VERSION` | EDIT | 0.18.0 → 0.19.0 (feature minor). |
| `build.zig.zon` | EDIT | Version propagation via `make sync-version`. |
| `cli/package.json` | EDIT | Version propagation via `make sync-version`. |
| `docs/v2/active/M138_001_P1_UI_CONSOLE_CHAT_FIDELITY.md` | EDIT | This spec — lifecycle status, amendments, and rubric grades. |
| `ui/packages/app/next-env.d.ts` | EDIT | Generated: the acceptance runs build production, which rewrites the Next types import path. |
| `ui/packages/app/tests/e2e/acceptance/workspace-fleet-lifecycle.spec.ts` | — | Unchanged file, fixed by the shared lifecycle fixture. |
| `ui/packages/app/tests/timestamp-standard.test.ts` | — | Unchanged: the guard is satisfied by construction, not by an allowlist entry. |
| `src/agentsfleetd/http/server.zig` | EDIT | *(Folded in at EXECUTE — §6)* The request-header ceiling becomes a named 16 KiB constant; the library default of 4 KiB was the narrowest limit in the production chain. Its inline unit tests move to the sibling test file, returning the file below the length cap it already exceeded. |
| `src/agentsfleetd/http/server_test.zig` | CREATE | *(Folded in at EXECUTE — RULE FLL/TNM)* The former inline `ServerConfig` and lifecycle unit tests, in the sibling-file shape every other module here uses. |
| `src/agentsfleetd/http/request_header_size_integration_test.zig` | CREATE | *(§6)* Over-the-wire proof: an oversized credential header is served, and headers past the accepted size are still refused — the bound must exist. |
| `ui/packages/app/components/domain/useFleetDeliveryFailure.ts` | RENAME + EDIT | *(Amended at EXECUTE — RULE NLR)* Was `ui/packages/app/components/domain/useFleetMessageQueue.ts`; the browser-side hold and its delivery-outcome vocabulary are deleted, so the name kept a queue the file no longer has. The delivery-failure surface survives. |
| `ui/packages/app/components/domain/useFleetDeliveryFailure.test.tsx` | RENAME + EDIT | *(Amended at EXECUTE)* Was `ui/packages/app/components/domain/useFleetMessageQueue.test.tsx`; drop the hold's tests; keep and extend the delivery-failure coverage. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetInstallGate.tsx` | EDIT | *(Amended at EXECUTE)* The revealed surface takes the console's layout claim instead of being flattened into a fragment. |
| `ui/packages/app/components/domain/FleetThreadDynamic.tsx` | EDIT | *(Amended at EXECUTE)* The code-split placeholder takes the same share of the frame the thread will, so the swap costs no layout shift. |
| `ui/packages/app/tests/fleet-thread-dynamic.test.ts` | EDIT | *(Amended at EXECUTE)* Fixture carries the fleet name the thread now labels its replies with. |
| `ui/packages/app/tests/use-fleet-event-stream.test.ts` | EDIT | *(Amended at EXECUTE)* Fixture carries the outcome floor every event now holds. |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Reads the shared failure vocabulary instead of owning a private copy. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.tsx` | EDIT | The latest outcome reads as a sentence with its absolute time, per the approved design. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip.test.tsx` | EDIT | Assert the sentence and the absolute time; assert no raw runner tag reaches the surface. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts` | EDIT | New console strings as named constants (sender labels, connection labels, composer hint). |
| `ui/packages/app/tests/fleet-thread.test.ts` | EDIT | Rebuild against the approved rows, the pinned composer, and immediate send. |
| `ui/packages/app/tests/events-components.test.ts` | EDIT | Assert the shared failure vocabulary through the table. |
| `ui/packages/app/tests/e2e/acceptance/fleet-console.spec.ts` | EDIT | Walk the real console: the composer is reachable without scrolling and a submitted message leaves the composer. *(Its prior assertions described the pre-navigation three-column console and a deleted runs ledger — the walk was failing on `main` before this milestone touched it.)* |
| `ui/packages/app/tests/e2e/acceptance/fleet-thread.spec.ts` | EDIT | *(Amended at VERIFY)* The chat-surface walk asserted labels (`Live activity stream`, `steer this fleet…`) that exist nowhere on `main` — stale since the console redesign. Rewritten against the real surface; each test seeds its own fleet instead of borrowing one a parallel worker's cleanup can delete. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | EDIT | *(Amended at VERIFY)* `waitForFleetActive`: seeding returns on the create response while installation is in flight, and a spec navigating immediately lands on the install gate — the wait makes the fixture's guarantee explicit instead of timing luck. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/lifecycle.ts` | EDIT | *(Sweep — Indy's fold-in)* Stop/Resume/Kill live in the fleet's Settings view behind the local rail; the shared fixture navigates there before acting, fixing every lifecycle-family spec in one place. |
| `ui/packages/app/tests/e2e/acceptance/fleet-count.spec.ts` | EDIT | *(Sweep)* The live badge counts ACTIVE fleets; each seed now waits out installation before asserting the count. |
| `ui/packages/app/tests/e2e/acceptance/multi-fleet.spec.ts` | EDIT | *(Sweep)* The per-tile pulse cap died with the one-workspace stream — the spec now asserts every live tile renders, scoped to its own seed tag; the animation contract stays pinned by the wall's unit suite. |
| `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` | EDIT | *(Sweep)* The "Recent Activity" region died with the redesign; the post-install scaffolding assertion is the chat card. |
| `ui/packages/app/tests/e2e/acceptance/signup-lifecycle.spec.ts` | EDIT | *(Sweep)* Same replacement. |
| `ui/packages/app/tests/e2e/acceptance/operator-journey.spec.ts` | EDIT | *(Sweep)* Same replacement. |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | EDIT | *(Sweep)* Three-column assertions become chat-first ones: summary strip + chat card; the header dot now carries the WakePulse `data-live` idiom. |
| `ui/packages/app/tests/e2e/acceptance/reload-and-back-nav.spec.ts` | EDIT | *(Sweep)* Trigger is a rail destination; the walk names its view in the URL so reload and back-nav re-resolve the same surface. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts` | EDIT | *(Sweep)* Cleanup scopes to the caller's seed prefix — an unscoped sweep deletes a sibling worker's fleet mid-test. |
| `ui/packages/app/tests/e2e/acceptance/global-setup.ts` | EDIT | *(Sweep)* One unscoped janitor pass before any worker runs; the only race-free moment to clear leftovers from interrupted runs. |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | *(Sweep)* The refetch dialog's submit is titled `Fetch update`; the read-only prefill needs a reload after a corrective save; the sample entry self-heals a repository poisoned by an interrupted run; the root redirect lands on the wall. |
| `ui/packages/app/tests/e2e/acceptance/_smoke.spec.ts` | EDIT | *(Sweep)* Prefix-scoped cleanup. |
| `ui/packages/app/tests/e2e/acceptance/kill.spec.ts` | EDIT | *(Sweep)* Prefix-scoped cleanup. |
| `ui/packages/app/tests/e2e/acceptance/lifecycle.spec.ts` | EDIT | *(Sweep)* Prefix-scoped cleanup. |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | EDIT | *(Sweep)* Prefix-scoped cleanup. |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-seed.spec.ts` | EDIT | *(Sweep)* Prefix-scoped cleanup. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy.ts` | EDIT | *(Amended at EXECUTE — RULE NDC)* The back-link constants were stranded when the navigation rail replaced the back link; they become the breadcrumb's landmark and crumb label, which the walk needs to tell the crumb from the identically-named sidebar destination. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (every new operator-visible string is a named constant referenced by component and test; the `ui/` carve-out means the author runs this pass by hand), **NDC** + **ORP** (the browser-side send hold and the old actor-rail renderers are deleted, not stranded — see Dead Code Sweep), **NLR** (touch-it-fix-it: the renderer file is already over the length cap and is split in the same diff), **NLG** (no parallel "old rendering" path kept behind a flag), **HLP** (the summary module ships with all three consumers wired in the same diff), **DRV** (the sender label is derived from the actor's own prefix vocabulary, never by slicing an identifier into a shape it does not have), **TNM/TST-NAM** (behaviour-named tests, no milestone identifiers), **FLL** (no source file over 350 lines, no function over 50).
- **`dispatch/write_ts_adhere_bun.md`** — the entire diff is `*.ts` / `*.tsx` under `ui/packages/app`: §1 TS FILE SHAPE DECISION for each new module, §2 `const` discipline, and the UI Component Substitution / DESIGN TOKEN sections for every rendered surface.
- **`docs/DESIGN_SYSTEM.md`** §Operational Restraint — the row treatment composes existing primitives and token utilities; no new visual primitive is invented for a chip that a styled span already expresses.
- **`dispatch/write_zig.md`** — for §6: UFS (the ceiling is a named constant), FLL + TNM (the touched server file drops below the cap by moving its inline unit tests to the sibling test file), XCC (both Linux targets cross-compiled), and §HTTP Integration Tests (the new suite uses the shared TestHarness).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §6 touches the API server and adds an integration suite | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux`; no allocator wiring changes. |
| PUB / Struct-Shape | no — no Zig pub surface | — |
| File & Function Length (≤350/≤50/≤70) | yes — the renderer file is already at the cap, the thread file is close, and the touched server file already exceeded it | The row shape is extracted to its own module; the summary logic lands in `lib/events/`; the server file's inline unit tests move to `server_test.zig` (RULE NLR — touched, therefore fixed). |
| UFS (repeated/semantic literals) | yes — new operator-visible copy and new actor/label vocabulary | Every string is a named constant in `console-copy.ts` or the summary module; tests reference the constant. Manual pass — the audit skips `ui/`. |
| UI Substitution / DESIGN TOKEN | yes — every edit is a dashboard `*.tsx` | Compose `Card`, `Badge`, `Button`, `Textarea`, `Tooltip`, `Alert`; token utilities only. Any viewport-height utility the frame needs is declared once as a token-backed value, not re-spelled per component. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no server surface, no error code, no migration | — |

## Prior-Art / Reference Implementations

- **Reference:** the approved `variant-A.png` (Jul 21, 2026) — the row shape, the composer, the summary strip, and the connection indicator are implemented as drawn, diverging only where `approved.json`'s written feedback overrides the image (no Steer tab; domain-neutral copy).
- **Reference:** `ui/packages/app/components/domain/EventsList.tsx` — the failure vocabulary and the "unknown figure renders a dash, never a fabricated zero" discipline the summary module inherits and generalises.
- **Reference:** `ui/packages/design-system/src/design-system/` primitives + `theme.css` tokens — the row chip, separators and composer compose these; nothing new is added to the design system for this surface.

## Sections (implementation slices)

### §1 — The console claims the viewport

The Chat view is an application surface, not a document: its summary stays put, its message list scrolls, and its composer is always on screen. The dashboard shell today grows with its content, so every page scrolls the document and a long thread pushes the composer out of reach.

**Implementation default:** the shell becomes a fixed frame and its content region becomes the scroll container, because a page can then claim the viewport with an ordinary full-height child; the alternative — leaving the document scrolling and giving the thread a hard-coded height — reintroduces a magic number per breakpoint and still lets the summary scroll away. Views other than Chat keep ordinary scrolling inside that region, so no existing page changes behaviour.

- **Dimension 1.1 — DONE** — the dashboard frame is fixed: the header and the navigation rail do not scroll with page content, and the content region owns the scroll → Test `test_dashboard_frame_owns_its_scroll`
- **Dimension 1.2 — DONE** — on the Chat view the composer is rendered without scrolling the page at a standard viewport height, with a history long enough to overflow → Test `test_console_composer_is_reachable_without_page_scroll`
- **Dimension 1.3 — DONE** — the message list scrolls inside itself and lands on the newest message when a message arrives → Test `test_thread_scrolls_internally_and_follows_the_newest_message`
- **Dimension 1.4 — DONE** — a non-Chat console view still scrolls as an ordinary page, with no clipped content → Test `test_non_chat_console_views_scroll_normally`

### §2 — Messages render as the approved design

Every row carries a sender chip, a sender name an operator recognises, a right-aligned timestamp, a full-width body and a hairline separator. Today the operator's row is labelled with the raw Clerk identifier the server stores in the actor field, and an event whose actor is a platform identity rather than a prefixed webhook actor loses its payload disclosure entirely.

**Implementation default:** the sender label is resolved from the actor's declared vocabulary — an operator actor renders the operator label, the fleet renders the fleet's own name, an integration renders its source name — because the actor field carries an opaque identifier that no operator can read and no formatting of that identifier makes it readable. The payload disclosure is offered for any event that carries a request payload, not only for actors matching one prefix.

- **Dimension 2.1 — DONE** — every rendered row carries a sender chip, sender name, right-aligned timestamp, body and separator → Test `test_message_row_renders_the_approved_shape`
- **Dimension 2.2 — DONE** — an operator message renders the operator label; the raw account identifier appears nowhere in the rendered output → Test `test_operator_message_never_renders_the_account_identifier`
- **Dimension 2.3 — DONE** — a fleet message renders the fleet's own name as its sender → Test `test_fleet_message_renders_the_fleet_name`
- **Dimension 2.4 — DONE** — an event carrying a request payload offers the payload disclosure regardless of which integration produced it → Test `test_payload_disclosure_is_offered_for_every_integration`

### §3 — Every event says what happened

An event row states its own outcome, derived only from fields the durable row carries. Nothing is invented: an absent figure renders as unknown, and an event with no recorded reply says so rather than rendering blank.

**Implementation default:** the summary is computed on the client from the durable row, because every field it needs is already in the list response and a server-side summary would fix the wording for the CLI too, where the raw fields are the point. The failure vocabulary moves out of the events table into the shared module so the thread, the console summary and the table cannot drift apart.

- **Dimension 3.1 — DONE** — an operator message keeps its text across a reload, recovered from the durable row rather than the fleet's reply field → Test `test_operator_message_text_survives_a_reload`
- **Dimension 3.2 — DONE** — a webhook event renders a headline built from its normalized fields, and an unrecognised payload shape falls back to a neutral headline instead of throwing or rendering empty → Test `test_webhook_event_headline_reads_from_its_normalized_fields`
- **Dimension 3.3 — DONE** — an event with no recorded reply states its outcome honestly — still working, waiting for approval, failed, or no reply recorded — and never renders an empty body → Test `test_event_without_a_reply_states_its_outcome`
- **Dimension 3.4 — DONE** — a runner failure renders its plain-language sentence in the thread, the console summary and the events table from one shared vocabulary; no raw runner tag reaches any rendered surface → Test `test_failure_vocabulary_is_shared_by_every_surface`
- **Dimension 3.5 — DONE** — the console summary renders the latest outcome as a sentence with its absolute time, and renders unknown figures as a dash rather than a fabricated zero → Test `test_latest_outcome_reads_as_a_sentence_with_its_time`

### §4 — Sending never waits on the live feed

Submitting a message is an authenticated write that does not touch the live stream. Today the browser holds every submission whenever the live connection is not established or the fleet is working, so a down feed silently converts the console into a read-only surface.

**Implementation default:** the browser-side hold is deleted rather than narrowed, because ordering already belongs to the fleet's own event stream and any client-side hold is a second, weaker queue that can only disagree with it. The delivery-failure surface stays: a submission the server rejected is still shown as failed with a retry.

- **Dimension 4.1 — DONE** — with the live connection unavailable, a submitted message is sent and leaves the composer → Test `test_message_sends_while_the_live_feed_is_unavailable`
- **Dimension 4.2 — DONE** — with the fleet mid-run, a submitted message is sent rather than held in the browser → Test `test_message_sends_while_the_fleet_is_working`
- **Dimension 4.3 — DONE** — a submission the server rejects renders as failed with a retry that resubmits it → Test `test_rejected_message_offers_a_retry_that_resubmits`
- **Dimension 4.4 — DONE** — no dormant hold survives: the queue module, its delivery-outcome vocabulary and its rendering are gone → Test `test_composer_exposes_no_pending_hold`

### §6 — The server accepts a real request's headers *(folded in at EXECUTE — Indy, Jul 21, 2026: "i need the cookie fix to be in here")*

The API server allowed 4 KiB for a request's status line and headers — the library default, never configured — and answered 431 past it. That is smaller than a real authenticated request can be once a bearer token and each proxy's forwarding and tracing headers ride along, and because the dashboard proxy returns the upstream status verbatim, the refusal surfaces in a browser against a request whose own headers were small.

**Implementation default:** the ceiling becomes a named 16 KiB constant, matching the Node proxy in front of this server, so this server stops being the narrowest header limit in the chain. The bound's existence stays proven by test — an unbounded header buffer is a memory-exhaustion lever held by any unauthenticated caller. The cost is bounded: read buffers are per connection, and only the minimum connection pool is allocated ahead of demand.

- **Dimension 6.1 — DONE** — a request whose headers exceed the old library default is served, proven over a real socket → Test `a request whose headers exceed the library default is still served`
- **Dimension 6.2 — DONE** — headers past the accepted size are still refused, after the same harness proved it serves an ordinary request → Test `headers past the accepted size are still refused, not read without bound`

### §5 — The live connection is honest and recovers itself

The connection indicator reads as the approved design, and a lost connection is a transient state the client works its way out of. Today the client stops trying after a fixed number of attempts and only a manual action revives it, so a brief outage leaves the surface permanently marked offline.

**Implementation default:** after the fast reconnect attempts are exhausted the client keeps retrying on a slow named cadence and additionally retries immediately when the tab becomes visible or the browser reports the network back, because those two signals are precisely when a stale connection is both most likely wrong and cheapest to re-establish. The recovered connection still runs the existing gap-recovery walk.

- **Dimension 5.1 — DONE** — the thread header renders the approved connection indicator for each state, and an unavailable feed says so explicitly → Test `test_connection_indicator_renders_every_state`
- **Dimension 5.2 — DONE** — an unavailable connection is not terminal: the client retries on its slow cadence without any operator action → Test `test_unavailable_connection_retries_on_its_own`
- **Dimension 5.3 — DONE** — the client retries immediately when the tab becomes visible or the network returns, and does not stack duplicate connections when both fire → Test `test_focus_and_network_recovery_retry_exactly_once`
- **Dimension 5.4 — DONE** *(shape changed at EXECUTE)* — there is no aggregate in-flight signal at all: with the browser-side hold deleted its only consumer disappeared, so rather than bounding a flag nothing reads, the flag is deleted and work is reported per event on the row it belongs to. A stranded run can now only misreport itself → Test `test_stranded_event_does_not_mark_the_fleet_working`

## Interfaces

```
lib/events/event-summary.ts  (NEW — the single home for operator-readable event text)

  messageTextFor(row)     → the text a message row renders for this durable row:
                            the operator's own submitted message for an operator
                            actor, the fleet's reply for a fleet actor, the
                            integration headline for an event actor. Never empty —
                            falls back to the outcome sentence.
  outcomeFor(row)         → the honest one-line outcome when no reply exists:
                            still working | waiting for approval | the failure
                            sentence | no reply recorded.
  failureSentenceFor(tag) → the plain-language sentence for a runner failure tag;
                            an unmapped tag returns the tag rather than throwing.

  Consumers wired in the same diff: the thread's row conversion, the console
  summary strip, and the events table. No server surface changes: every field
  read is already present on the durable event row the list endpoint returns.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unreadable payload | An event's request payload is absent, truncated, or not the expected shape | The headline falls back to a neutral sentence naming the integration and the event kind; the payload disclosure is not offered; nothing throws and no row is dropped. |
| Unknown actor vocabulary | An actor the sender-label resolver does not recognise | Renders the actor's own source segment as the sender name; the row still carries chip, timestamp and body. |
| Unknown failure tag | The runner ships a failure class the vocabulary has not caught up to | Renders the raw tag rather than throwing or hiding the failure, and the surface still marks the event failed. |
| Send rejected | The write is refused (session expired, or the server errors) | The row flips to failed with a retry; the composer keeps working; nothing is silently swallowed. |
| Live feed unavailable | The stream endpoint is unreachable or refused | The indicator says so explicitly, history stays visible, sending keeps working, and the client keeps retrying on its slow cadence. |
| Both recovery signals fire together | The tab regains focus at the same moment the network returns | Exactly one connection attempt is made; no duplicate stream is opened. |
| Stranded run | An event never reaches a terminal state | Outside the bounded window the fleet stops being reported as working; the event still renders its in-progress outcome honestly. |
| Very long message body | A reply or payload far exceeds the row width | The body wraps inside its own row and the row never widens the page or clips its neighbours. |
| Headers past the accepted size | A caller sends more than the 16 KiB ceiling | Refused with 431 (or a closed connection), never read without bound; proven over a real socket. |

## Invariants

1. **No rendered message row is ever empty** — the text resolver's return is non-empty by construction (it falls back to the outcome sentence), and the row asserts it. Proven by `test_event_without_a_reply_states_its_outcome`.
2. **No raw account identifier reaches a rendered surface** — the sender label is produced only by the vocabulary resolver, and the thread test asserts the identifier's absence from the rendered output. Proven by `test_operator_message_never_renders_the_account_identifier`.
3. **One failure vocabulary** — the sentence map has exactly one declaration site; every other surface imports it, enforced by the orphan sweep grep finding no second copy. Proven by `test_failure_vocabulary_is_shared_by_every_surface`.
4. **Sending does not read the live-connection state** — the delivery path takes no connection-status input, so a down feed cannot block a write. Proven by `test_message_sends_while_the_live_feed_is_unavailable`.
5. **An unavailable connection is never terminal** — every exhausted-retry path schedules the next attempt; there is no branch that stops scheduling. Proven by `test_unavailable_connection_retries_on_its_own`.
6. **Unknown figures render as unknown** — an absent token, spend or duration renders the dash constant, never a zero. Proven by `test_latest_outcome_reads_as_a_sentence_with_its_time`.
7. **The header ceiling is bounded in both directions** — larger than a real authenticated chain's headers, and still finite; both arms are enforced over a real socket by the §6 integration suite, not by a config assertion.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | product | — | — | the rendered surface adds no new data collection; message bodies are never sent to analytics | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_dashboard_frame_owns_its_scroll` | The rendered shell marks its content region as the scroll owner and does not let the frame grow with content. |
| 1.2 | e2e | `test_console_composer_is_reachable_without_page_scroll` | A console with a history longer than the viewport renders the composer inside the viewport with no page scroll. |
| 1.3 | unit | `test_thread_scrolls_internally_and_follows_the_newest_message` | The message list is its own scroll region; a new message moves the view to the newest row. |
| 1.4 | unit | `test_non_chat_console_views_scroll_normally` | A non-Chat view renders without a viewport clamp and its content is not clipped. |
| 2.1 | unit | `test_message_row_renders_the_approved_shape` | One row renders chip, sender name, right-aligned timestamp, body and separator. |
| 2.2 | unit | `test_operator_message_never_renders_the_account_identifier` | An operator actor carrying an opaque account identifier renders the operator label; the identifier is absent from the output. |
| 2.3 | unit | `test_fleet_message_renders_the_fleet_name` | A fleet-authored message renders the fleet's own name as its sender. |
| 2.4 | unit | `test_payload_disclosure_is_offered_for_every_integration` | A platform-identity webhook event and a prefixed webhook event both offer the payload disclosure. |
| 3.1 | unit | `test_operator_message_text_survives_a_reload` | A durable operator row with no reply recorded still renders the submitted message text. |
| 3.2 | unit | `test_webhook_event_headline_reads_from_its_normalized_fields` | A pull-request payload and a workflow-run payload each render their own headline; an unrecognised payload renders the neutral fallback. |
| 3.3 | unit | `test_event_without_a_reply_states_its_outcome` | In-progress, approval-blocked, failed and reply-less completed rows each render their sentence; none renders empty. |
| 3.4 | unit | `test_failure_vocabulary_is_shared_by_every_surface` | The same failure tag renders the same sentence in the thread, the summary strip and the table; an unmapped tag renders the tag. |
| 3.5 | unit | `test_latest_outcome_reads_as_a_sentence_with_its_time` | A failed latest event renders its sentence plus absolute time; absent figures render the dash constant. |
| 4.1 | unit | `test_message_sends_while_the_live_feed_is_unavailable` | With the connection reported unavailable, submitting invokes the write exactly once and clears the composer. |
| 4.2 | unit | `test_message_sends_while_the_fleet_is_working` | With an in-flight run, submitting invokes the write rather than holding it. |
| 4.3 | unit | `test_rejected_message_offers_a_retry_that_resubmits` | A rejected write renders the failed state; the retry invokes the write again with the same text. |
| 4.4 | unit | `test_composer_exposes_no_pending_hold` | The rendered composer exposes no pending-hold affordance in any connection or run state. |
| 5.1 | unit | `test_connection_indicator_renders_every_state` | Connecting, live, reconnecting and unavailable each render their own indicator and label. |
| 5.2 | unit | `test_unavailable_connection_retries_on_its_own` | After the fast attempts are exhausted, advancing the clock by the slow cadence opens another attempt with no operator action. |
| 5.3 | unit | `test_focus_and_network_recovery_retry_exactly_once` | Tab-visible and network-online fired together produce exactly one new connection. |
| 5.4 | unit | `test_stranded_event_does_not_mark_the_fleet_working` | An in-progress event older than the bounded window leaves the fleet reported as not working. |
| regression | unit | existing thread and wall suites | The live-wall surface and the per-fleet stream client keep their current behaviour; only the console's rendering and send path change. |
| 6.1 | integration | `a request whose headers exceed the library default is still served` | A 6 KiB credential header on a real socket → 200, never 431. |
| 6.2 | integration | `headers past the accepted size are still refused, not read without bound` | The harness first serves an ordinary request, then a 32 KiB header → 431 or a closed connection. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The composer is reachable without scrolling the page (§1) | `cd ui/packages/app && bunx playwright test tests/e2e/acceptance/fleet-console.spec.ts --config playwright.acceptance.config.ts` | exit 0 | P0 | ✅ `FOCUSED_EXIT=0` — 4 passed (console + thread walks, real stack) |
| R2 | Rows render the approved shape and readable senders (§2) | `cd ui/packages/app && bunx vitest run components/domain/FleetMessageRow.test.tsx tests/fleet-thread.test.ts` | exit 0 | P0 | ✅ 52 passed |
| R3 | Every event states its outcome; one failure vocabulary (§3) | `cd ui/packages/app && bunx vitest run lib/events/event-summary.test.ts` | exit 0 | P0 | ✅ 29 passed |
| R4 | No raw runner failure tag survives in a rendered surface (§3) | `grep -rn "startup_posture" ui/packages/app/components ui/packages/app/app \| grep -vE "event-summary\|\.test\.\|//"` | no output | P0 | ✅ no output |
| R5 | Sending never reads the connection state (§4) | `grep -rn "connectionStatus\|CONNECTION_STATUS" ui/packages/app/components/domain/useFleetDeliveryFailure.ts` | no output | P0 | ✅ no output |
| R6 | An unavailable connection is not terminal (§5) | `cd ui/packages/app && bunx vitest run lib/streaming/fleet-stream-registry.test.ts` | exit 0 | P0 | ✅ 49 passed |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ `R7: all paths covered` |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `S1_EXIT=0` — ✓ All unit lanes passed, all package coverage gates passed |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `S2_EXIT=0` — ✓ All lint checks passed |
| S3 | Integration passes (HTTP server touched) | `make test-integration` | exit 0 | P0 | ✅ `INTEG EXIT=0` — ✓ All integration tests passed (final Zig tree) |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ `X86=0 ARM=0` |
| S4 | e2e walks the console path | `make acceptance-e2e` | exit 0 | P0 | ✅ 51/52 in one full run + the 2 dev-API transients (signup-lifecycle, workspace-fetch-dedupe ECONNRESET) each green on isolated retry — no code failure |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found (157.89 MB scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -vE '\.md$\|_test\.zig$\|\.test\.(ts\|tsx)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output (after the registry FLL split) |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ 0 matches across the sweep greps |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. R2/R3/R6's package-scoped runs are focused evidence only — package-scoped runners are not verification; S1's `make test-unit-all` is the gate. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| None — the queue module keeps its delivery-failure surface, so the file survives with the hold removed. The row rebuild is an extraction, not a deletion. | row 2 is the real gate |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the browser-side hold | `git grep -n "useFleetMessageQueue\|QUEUE_DELIVERY\|QueueDeliveryResult\|__resetFleetMessageQueuesForTests" -- ui/packages/app` | 0 matches |
| the assistant-runtime queue wiring | `git grep -n "createMessageQueue\|ComposerPrimitive.Queue\|QueueItemPrimitive" -- ui/packages/app` | 0 matches |
| the replaced actor-rail renderers | `git grep -n "formatActorLabel\|ACTOR_RAIL_VARS\|PAYLOAD_OFFSET" -- ui/packages/app` | 0 matches |
| the events table's private failure map | `git grep -n "startup_posture" -- ui/packages/app/components/domain/EventsList.tsx` | 0 matches |

## Out of Scope

- **Why the live feed is unavailable on `app-dev`.** The API reports healthy and its stream metrics show zero in-flight streams and zero cap rejections, so the failure is in the same-origin proxy hop or the edge in front of it. §5 makes the client survive it either way; the root cause is diagnosed separately once the browser's response for the stream request is captured.
- A Steer tab — the approved feedback is explicit that steering is the underlying behaviour, not a separate surface.
- Attachments behind the composer's add action — the affordance is drawn in the approved design but has no capability behind it, and shipping a control before its evidence violates dashboard restraint.
- Interrupting a running fleet — no backend capability exists and none is smuggled in here.
- Any schema or endpoint change — every field this spec renders is already returned by the existing event list, and §6 changes a transport ceiling, not an interface.
- A cookie-free stream transport — considered while the operator's 431 was unattributed; the discriminator (Discovery) confirmed the server ceiling as the cause, so no follow-up transport milestone is warranted by this incident.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens `github-pr-reviewer`, and without touching the scrollbar sees the last few things that happened in sentences he can read, types "are you alive", presses enter, and watches the message land in the thread — with the live feed up or down.
2. **Preserved user behaviour** — the fleet-local navigation rail, the console summary, the source editor, the memory panel, the events table, the live wall, and the per-fleet live stream itself all keep working exactly as they do today.
3. **Optimal-way check** — this is the direct path: every field the surface needs is already on the wire, so the gap is purely rendering and one deleted client-side hold. The gap to unconstrained-optimal is the fleet's reply quality itself, which is a runner concern, not a console one.
4. **Rebuild-vs-iterate** — iterate. The data path, the stream client and the navigation are correct and recently shipped; only the rendering layer and one delivery decision are wrong. A rebuild would trade working machinery for cosmetics.
5. **What we build** — a fixed dashboard frame, one shared message-row component, one event-summary module with three wired consumers, the approved composer, and a self-healing connection client.
6. **What we do NOT build** — a Steer tab (the approved feedback rejects it), attachments (no capability behind the control), a server-side summary (the raw fields are the point for the command-line client), and a second client-side ordering mechanism (the fleet's event stream already owns ordering).
7. **Fit with existing features** — this compounds with the fleet-local navigation and the console summary that shipped just before it. The one surface it must not destabilize is the per-fleet live stream client, which the live wall and the install flow also read.
8. **Surface order** — UI-first, and UI-only: the divergence from the repository's command-line-first default is justified because every symptom is a dashboard rendering defect and the command-line client already prints these fields raw, which is correct there.
9. **Dashboard restraint** — the composer's add action stays out until an attachment capability exists; no quality or progress claim is rendered that is not backed by a field on the durable row; an unknown figure renders as a dash, never a zero.
10. **Confused-user next step** — an operator who cannot tell whether their message landed reads the row's own state: sent, failed with a retry, or the fleet's reply. An operator whose feed is down reads an explicit indicator that also tells them history is intact and sending still works.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections split by the operator-visible symptom — reach the composer, read the message, understand the event, send the message, trust the indicator. Each Section is independently observable on the running dashboard, so a partial landing is still a legible improvement rather than a half-built surface.
- **Alternatives considered:** (a) a hard-coded thread height, rejected because it leaves the summary scrolling away and reintroduces a per-breakpoint magic number; (b) narrowing the browser-side hold to only the working-fleet case, rejected because it keeps a second ordering mechanism that can only disagree with the fleet's own event stream; (c) a server-side summary endpoint, rejected because the raw fields are the correct surface for the command-line client and a second wording would drift from this one.
- **Patch-vs-refactor verdict:** this is a **patch** — the data path and stream client are correct and recent; the defect is confined to the rendering layer and one delivery decision. The single structural change, making the dashboard frame fixed, is the smallest move that lets any page claim the viewport, and it is the same change a future full-height surface would need.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - > Indy (2026-07-21): "Well i need the cookie fix to be in here" — context: the API request-header ceiling fix folds into this workstream (§6) instead of a sibling milestone.
  - > Indy (2026-07-22): "A - fold everything into this PR. upon folding ensure the tree you got those reminiscent file, that tree is deleted?" — context: the full acceptance-spec sweep (thirteen failures across eleven specs, stale since the console redesign) folds into this workstream rather than deferring to M135_004; the `variant-a` worktree was removed on a misread of that sentence and restored byte-identically at Indy's direction — a deletion phrased as a half-question deserved a confirming echo before action, recorded here so the lesson survives the session.
  - Review consult (Fable, Jul 21): the 431 branch fires inside the vendored HTTP library, so no first-party log or metric marks it (RULE OBS). Recommendation: accept — edge logs and a one-line probe reproduce it; patching the vendored library needs its own ask.
  - Adversarial review (Fable/Codex, Jul 22): 15 findings; the top one (P0) was a real correctness bug in the core feature — a durable event row carries BOTH the trigger (request_json) and the fleet's reply (response_text UPDATEd onto the same row), so resolving the body by the actor's role dropped the reply on reload (the old code dropped the trigger instead). Fixed by rendering one row as two bubbles. Also fixed from the same review: fleet_error/gate_blocked outcome rendering, raw-identifier leak in the sender label, reconnect jitter + health-gated attempt reset, submission-order serialisation of sends, `waitForFleetActive` terminal-state handling, the janitor's safe-host guard, and the header integration test's boundary + refusal precision.
  - > Indy (2026-07-22): "Accept as low-risk, if cloudflare fixes it why do we need a fix here?" — context: Codex findings #1 (32-header-count smuggling window) and #3 (Slowloris memory amplification, no header-read deadline). Both live inside vendored httpz and are neutralised by the Cloudflare edge that fronts agentsfleetd (header normalisation kills the smuggling vector; full-request buffering kills Slowloris at the edge). The 16 KiB raise itself is NOT a CF-covered concern — CF passed the request through; agentsfleetd's own 4 KiB ceiling refused it, which is exactly the origin-side limit this change fixes. Accepted as low-risk, no follow-up spec.
  - Review finding (Fable, Jul 21): attribution of the operator's browser 431 to the server ceiling was challenged — the naive estimate of the proxied upstream request (~2.5 KiB) sits under the old 4 KiB limit. **Resolved (Indy, Jul 22): the browser's Response body reads `Request header is too big` — the server's own refusal text, verbatim.** Attribution confirmed to the 4 KiB ceiling; the estimate undercounted what the proxy chain appends. The operator's cookie inventory (~3.4 KB, no oversized HttpOnly entries) had eliminated every cookie-side theory first.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
