<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M259_001: Any fleet on any model searches the web, reads a page and acts on one, inside its network policy

**Prototype:** v2.0.0

**Milestone:** M259

**Workstream:** 001

**Date:** Nov 26, 2026

**Status:** IN_PROGRESS

**Priority:** P1, 13 of the 21 catalogue models offer no search, and every browser call is refused

**Categories:** API, CLI, DOCS, INFRA, OBS, UI

**Batch:** B1. Per Indy on Nov 27, web tools (§§1–5) start on `feat/m259-web-tools` alongside M258. The Firecracker spike (§6) starts after M258 merges. M259_002 follows as B2.

**Depends on:** M258_001 (Firecracker engine), M258_003 (production switch and bubblewrap retirement), and M258_002 (checkpoint behavior). The dependency applies to the browser. Review the three active M258 specs against the merged compute layer before §6.

**Provenance:** Originally drafted Nov 26, 2026. Readiness review on Nov 27 compared main at `ece273764` with the compute branch at `46723c018`.

**Canonical architecture:** `docs/architecture/runner_execution.md` §Tool catalog and §Network egress, and `docs/architecture/billing.md`. This spec rewrites their web and browser rows.

---

## Overview

**Goal (testable):** Every model's `web_search` answers through the first working vendor, `web_fetch` and browser reach only what policy admits, and private fleets search nothing.

**Terms.** Rust paths that start with a crate name are under `rustd/crates/<layer>/`; `guffbolt/` and `guffbolt-ai/` are under `rustd/`. L is `/v1/runners/me/runs/{run_id}`.

A **public** fleet has `http_egress` `public` ("Allow all" on); a **private** one has `restricted`, the default at birth. A **vendor** is a web search company the daemon calls with a platform key. The browser is driven over the Chrome DevTools Protocol (CDP).

**Problem:** Read from source.

- **Search is missing on 13 of 21 catalogue models.** `web_search` is a provider-hosted tool sent on Messages and Responses only (`guffbolt-ai/src/wire/request/mod.rs:84-92`). The catalogue holds 4 Messages, 4 Responses, 11 Chat and 2 Gemini rows (`af_observability/src/semconv/model-library-allowlist.json`).
- **Hosted search escapes the fleet's policy.** The provider searches outside `af_egress` (`af_egress/src/lib.rs:1-11`), so a private fleet can still send data to a search engine.
- **A default fleet cannot read the page.** A fleet is born `restricted` (`afd_setup/src/creation/create.rs:273`). Its `allow` holds only approved secrets' hosts and the GitHub API host (`afd_gate/src/policy/shape/mod.rs:36-59`), so `web_fetch` answers `host_not_allowed` (`af_egress/src/policy/mod.rs:198`).
- **The fleet cannot ask for a host.** `request_access` asks for a vault secret or an integration only (`afr_tools/src/verbs/access/mod.rs:1-2,36-44`).
- **`web_fetch` stops at the first redirect** (`guffbolt/src/tools/web/web_fetch.rs:51`) and returns the whole page when `max_chars` is absent (`:66-69`). It drops every JSON for Linked Data (JSON-LD) block, because `html2text` 0.17.4 skips every `<script>` (`src/lib.rs:2062-2070`).
- **Nothing counts a search.** No code in `rustd/` reads or counts one, and `billing.md` names none.
- **Every browser call is refused.** `SandboxEnv` and `LazySandboxEnv` answer "waits for the Firecracker engine" (`afr_tools/src/generic/env/mod.rs:48-49,304`; `generic/env/lazy.rs:95`). The arguments are untyped (`guffbolt/src/tools/browser.rs:59,83,107`), yet every run is offered the three tools (`afd_gate/src/policy/catalog.rs:46-48`).

**Solution summary:**

- **One search tool on every wire.** `web_search` is a guffbolt function tool over a `SearchBackend` trait. Provider-hosted search is deleted from guffbolt-ai, with no alias.
- **Search vendors.** The daemon sends Parallel an explicit `mode: fast` and tries Exa if Parallel is unavailable or fails. It reads each key from the vault of `PLATFORM_ADMIN_WORKSPACE_ID`.
- **The policy decides first.** A private fleet's search and extract are refused before any call. A host the fleet's policy names is read locally, following redirects that are each re-checked.
- **Cost.** The runtime fee covers vendor cost. Reserve each dispatch in Postgres before sending. The per-run count bounds physical requests from above.
- **The fleet can ask for a host,** a person approves it once, and sources reach the thread.
- **Browser.** Typed tools read an accessibility snapshot and act on element references. Firecracker is the only supported engine.

**Unverified.** §6 measures Chromium confinement, transport, image size and recovery on merged M258. Discovery records pending privacy and browser decisions.

**Design flow:**

```
web_search {query} ─► runner SearchBackend ─► POST L/searches ─► daemon: fence · fleet public? · query valid
                         for vendor in [parallel, exa]: cooling or no vault key ─► next
                         commit reservation for run, vendor and operation ─► send one request
                         valid results ─► return · vendor failure ─► next · denied policy, fence, ledger or deadline ─► stop
web_fetch {url} ─► host in the fleet's policy? ─yes─► af_egress local GET, ≤20 redirects, each hop re-admitted
                   │no ─► public: POST L/extracts ─► Parallel full content    private: [network_restricted] names request_access
request_access {host} ─► card ─► Approve ─► NetworkPolicy.allow gains the host
browser_open · browser · screenshot ─► afr_tools RunBrowser ─► CDP over M258's stream into the run's VM
   Chromium in the VM ─► admit each paused request with its actual method, URL and body
                      ─► the engine's egress wall (M258) ─► the page
```

## Rulings

**Ruled by Indy on Nov 26, 2026** (quotes in Discovery).

| # | Ruling |
|---|---|
| Order | Web tools can proceed alongside M258. The Firecracker spike uses the merged M258 engine. |
| Search | Our own function tool `web_search` on every wire, as Amp does. No provider-hosted search is sent. |
| Vendors | Parallel Fast first, Exa search fallback. The platform owns both keys. |
| Private fleets | A `restricted` fleet can neither search nor use a vendor's page reader. |
| Gemini | Google Search grounding stays off; Gemini uses our `web_search`. |
| Cost | The runtime fee, `RUN_NANOS_PER_SEC` = 100,000 nanos (`afd_billing/src/nanos.rs:25`), covers search and extraction. Calls are counted, not billed per call. |
| Browser | Our own tool, "just like our memory tool". The names `browser_open`, `browser` and `screenshot` stay. |
| Pixels | A screenshot is wanted only where text is not enough. |

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(web): web_search over several vendors, routed web_fetch, host grants and a browser in the run's VM
- **Intent (one sentence):** Any fleet on any model searches, reads what it found and acts on a page, reaching only what a person allowed.
- **Authoring handshake:** Search works on every model through the daemon. Restricted fleets can request a named host. Browser development targets our own Firecracker after M258 merges.
- **Implementer handshake:** Deliver fleet-controlled search, page reads, host requests and citations on `feat/m259-web-tools`. Use the existing paid runtime and the platform-admin vault. A restricted fleet can request access to a named site. Its search and extraction remain disabled.
- **Build assumptions:** §§1–5 use the daemon and runner on main at `ece273764`. Reconcile protocol and database changes when M258 lands. §6 uses the toolbox builder and Firecracker configuration from that merge.

## Implementing agent — read these first

1. `rustd/crates/harness/af_providers/src/decision/client.rs` — a platform-keyed vendor call whose failure carries `retry_after` (`:132-136`); each vendor module mirrors it.
2. `rustd/crates/runner/afr_tools/src/egress/mod.rs` — local reads re-admit each redirect. The existing connection check synthesizes a GET.
3. `rustd/guffbolt/src/tools/memory/mod.rs` — `MemoryStore` (`:104`) and its typed tools; §7's browser takes the same shape.
4. `rustd/crates/runner/afr_tools/src/verbs/access/mod.rs` — `request_access`, which §4 extends to a host.
5. `docs/v2/reviews/m211-toolbox-spikes.md` — spike S1, why Chromium cannot start in bubblewrap, and the record format §6 mirrors.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M259_001_P1_API_CLI_DOCS_INFRA_OBS_UI_ANY_MODEL_SEARCHES_READS_AND_BROWSES.md` | CREATE | active implementation spec; moves to `done/` at close |
| `rustd/guffbolt-ai/src/{types/model.rs,types/mod.rs,lib.rs,wire/request/mod.rs,wire/request/tests.rs,wire/route/tests/counting/unsupported.rs}`, `rustd/guffbolt-ai/README.md` | EDIT | §2: `Hosted`, `Request.hosted`, `WEB_SEARCH_MESSAGES` and `fn hosted` deleted |
| `rustd/guffbolt/src/{lib.rs,tools/mod.rs,tools/code.rs,tools/web/**,tools/browser.rs,tools/env/mod.rs,tools/files/image.rs,tools/builder/**}`, `rustd/guffbolt/README.md`, `rustd/guffbolt/examples/fireworks_live.rs`, `rustd/guffbolt/tests/public_api/**` | EDIT/CREATE | §2, §3: `SearchBackend`, `web_search`, routed `web_fetch`; §7: `Backend::Browser`, typed verbs, the shared image fit |
| `rustd/crates/harness/af_providers/src/web/**` | CREATE | §1: the `Vendor` trait, one module per vendor |
| `rustd/crates/harness/af_egress/src/{redirect,error}/**` | EDIT | §3: `MAX_REDIRECTS` 3 to 20; `host_not_allowed` names `request_access` |
| `rustd/crates/protocol/af_wire/src/{activity,access_verb,protocol,paths}/**`, `af_wire/src/web/**` | EDIT/CREATE | §1 verb types; §4 the host request; §5 `sources`; the runner protocol `VERSION` moves once |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/{web.rs,access.rs}`, `afd_http/src/route/runner.rs`, `afd_api/tests/{route_inventory,route_meta_total}.rs`, `af_wire/src/paths.rs` | CREATE/EDIT | runner routes, inventory and counts |
| `rustd/crates/daemon/afd_fleet/src/run/web/**` | CREATE | §1: fence, network, the vendor walk, the cooldown cache and the count |
| `rustd/crates/daemon/afd_db/schema/<UTC>_thread_run_web_calls.sql`, `rustd/crates/daemon/afd_db/src/migration.rs` | CREATE/EDIT | §1: one row per run, vendor and operation |
| `rustd/crates/daemon/afd_db/schema/20261108122100_connector_integration_grants.sql`, `afd_api_wire/src/grant.rs`, `afd_approval/src/**`, `afd_gate/src/policy/{shape,build}/**` | EDIT | §4: `grant_type` `host` in the comment and the constants; an approved host joins `NetworkPolicy.allow` |
| `afd_gate/src/policy/approved/**`, `afd_fleet/src/{run/access,durable_task/github}/**`, `afd_approval/src/{grant_sql,sql}/**` | EDIT | carry grant kind through request, approval, revoke and credential checks |
| `afd_fleet/src/run/{deliver,message}/mod.rs`, `afd_fleet_lifecycle/src/sql/select_secret_access.sql`, `afd_turns/src/view/sql/insert_access_activity.sql`, `ui/packages/app/components/domain/approvals/AccessRequest.tsx` | EDIT | filter delivered secret names, vault holders, access receipts and card lookup by grant kind |
| `afd_db/schema/20261108125110_thread_items.sql`, `afd_fleet/src/run/sql/**`, `afd_turns/src/view/history/**` | EDIT | persist sanitized sources and return them on reload |
| `af_wire/src/tool_trace/**`, `afd_fleet/src/run/tool_trace/**`, `ui/packages/app/lib/streaming/{fleet-stream-row,fleet-stream-tool-trace}.ts` | EDIT | write and decode the saved trace consumed by the reloaded dashboard |
| `rustd/crates/foundation/af_core/src/{error_code,problem}/**` | EDIT | the new codes in Interfaces, each with its `REGISTRY` row and problem entry |
| `rustd/crates/runner/afr_tools/src/{catalog,runtime,stub,verbs,egress,backed}/**`, `afr_tools/src/run.rs`, `afr_tools/src/generic/{browser/**,env/**,mod.rs}`, `afr_tools/src/sandbox/browser/**` | EDIT/CREATE | §2: `Runtime::Provider` and `HostedToolUnavailable` go; §7, §8: `RunBrowser`, `BrowserSource`, the check; the refusal text goes |
| `rustd/crates/runner/afr_agent/src/{offer,router,loop,engine,ledger,records}/**`, `afr_agent/tests/providers/**` | EDIT | §2: no hosted list; §5: sources on the completed call; §7: the run's `BrowserSource` passed down |
| `rustd/crates/runner/afr_supervisor/src/{browser,client/run_verbs,verbs}/**` | CREATE/EDIT | §1 single-send web verbs, §4 typed access outcomes, §7 browser connection |
| Firecracker engine, guest protocol, client, confinement and launcher on merged M258 | VERIFY PATHS AT MERGE | Claude reports `af_guest_wire`, `afr_guest_client` and `guest/{afg_agent,afg_confine,afg_entry,main}` in its local tree. Identify the actual owners before changing the transport or confinement. |
| `rustd/crates/e2e/af_e2e/tests/support/definition_run.rs` | EDIT | it filters on `Runtime::Provider`, which goes |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | `chromiumoxide` 0.9.1, the CDP client |
| `scripts/toolbox/**` | EDIT, with Indy's sign-off for any new script | §7: the browser image `[?]` |
| `deploy/local/bootstrap.ts`, `deploy/local/bootstrap.test.ts`, `deploy/local/acceptance.sh` | EDIT | the local platform-admin vault gets `parallel` from `PARALLEL_API_KEY`, and `exa` when `EXA_API_KEY` is set |
| `.github/workflows/acceptance-on-demand.yml`, `.orly/orly.json` (`burst`) | EDIT, with Indy's approval at build | both vendor keys from 1Password; the item names are `[?]` |
| `ui/packages/app/components/domain/{conversation/summary,approvals,fleet}/**`, `ui/packages/app/lib/**` | EDIT | §4: a host card and "Can access" row; §5: the Sources list |
| `cli/src/commands/{grant,approvals}/**`, `cli/test/**` | EDIT | §4: grant and approval lists show a host |
| `tests/fixtures/fleets/{curious-otter,nosy-magpie}.json`, `tests/fixtures/fleets/README.md`, `ui/packages/app/tests/e2e/acceptance/prime-{curious-otter,nosy-magpie}.spec.ts` | CREATE/EDIT | §9: the two end-to-end fleets |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` |
| `docs/architecture/{runner_execution,billing,runner_protocol}.md` | EDIT | the tool rows, the browser, the search cost inside the runtime fee, the new verbs |
| `~/Projects/docs` tool, billing and API pages, `changelog.mdx` | EDIT, on that repository's own branch after merge | the published behaviour |

## Applicable Rules

- **`domains/http.md`, `domains/http-responses.md`, `domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-429, HTTP-NEWCODE, HTTP-REGISTRY, HTTP-LEAK, HTTP-OPENAPI. **`domains/sql.md`** — SQL-FILE, SQL-COUNTER, SQL-UPSERT, SQL-DEFAULT, TXN. **`domains/auth.md`** — AUTH-NOLOG, AUTH-SECRET, BIL.
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE. **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE. **`code.md`** — NLG, ORP, NDC for the hosted path and the refusal text; CODE-SEAM; PRI for vendor and page text that reaches a prompt.
- **`tests.md`** — TEST-NEG, TEST-REDGREEN, TEST-COUNTER, TEST-RACE, TEST-INJECT, TCF. **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-SECRET. **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; `cargo shear` and both standalone crates stay clean |
| `make test-unit-all` | yes | guffbolt, guffbolt-ai, `afr_tools`, `afr_agent`, app and CLI Dimensions, against fake vendors and a fake CDP endpoint |
| `make test-integration-rustd` | yes | every daemon Dimension, against fake vendor servers |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update both route files |
| `make check-version` | yes | the runner protocol `VERSION` and the published crates move as the gate requires |
| `af_e2e/tests/layers.rs` | yes | `chromiumoxide` sits in `afr_tools`, the stream source in `afr_supervisor`, and neither reaches `daemon/` |
| `make acceptance` (live, Linux, on demand) | not in a green PR | both prime specs run on a burst machine before the PR opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the vendor walk, each vendor, the snapshot and the check each stay one module |

## Prior-Art / Reference Implementations

- OpenCode’s search loop records attempted vendors in memory for one query. In random mode it tries another vendor on HTTP 429. Its hosted Console documents a $0.01 customer charge after a valid result set. Neither source establishes how Console accounts for physical upstream attempts. [Search loop](https://github.com/anomalyco/opencode/blob/2b8f912c8623a79688e236aa94d1e384fac3a325/packages/core/src/websearch.ts#L201-L220) · [Console billing](https://github.com/anomalyco/opencode/blob/2b8f912c8623a79688e236aa94d1e384fac3a325/services/site/src/docs/content/console/websearch.mdx#L36-L40)
- M259 uses the per-query attempted set for fallback. The committed reservation follows the existing decision evaluator: reserve once per physical attempt before calling the vendor. The runtime fee covers the search for the customer. [Decision evaluator](../../../rustd/crates/harness/af_providers/src/decision/mod.rs#L19-L21)
- **Reference:** the decision call reads its vendor key from the platform-admin vault on every call (`af_providers/src/decision/**`; `afd_credential/src/provider/decision.rs`). §1 keeps that inside the daemon, so no runner holds a key.
- **Reference:** `MemoryStore` and `RunMemory` (`afr_tools/src/generic/memory.rs`), a guffbolt trait the runner serves with no credential. §7 gives the browser the same split.
- **Reference:** use the redirect chain for local reads. Browser admission must supply the actual request to the same origin-policy rules.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR).

## Sections (implementation slices)

Build order: §1; then §2 and §4; then §3 and §5; §6's spike before §7; then §8; §9 last.

### §1 — The daemon reserves each vendor dispatch before sending

The runner posts each search or extract once, including on timeout, lost response or 5xx. The daemon validates the input and the live public fleet before dispatch. Search visits each configured vendor at most once in one request, under one deadline inside the runner call timeout. Explicit new tool calls are distinct purchases.

**Deadline.** Use a shared 12-second daemon budget for the complete verb, from handler entry through the final outcome. This fits the existing runner minimum of 13 seconds and default of 20 (`afr_supervisor/src/config/tuning.rs`, `client/http.rs`), and follows the existing 12-second bounded-dispatch precedent. Enforce that inequality in the config test. Divide remaining time at each dispatch among the remaining eligible vendors, including the current vendor. Two eligible vendors give the first at most half the remaining time. The second gets the remainder. Key resolution, reservations and reading bodies consume the same budget. The runner measures one monotonic tool deadline from before the verb POST through any local fallback. Pass the remaining duration into the redirect chain. A lost response ends the call with outcome unknown.

A vendor is skipped while it cools or while the platform-admin vault holds no key bag under its name. Otherwise the daemon makes one call, the key in a header and never in the URL. Search returns validated results on success. A vendor failure can advance the search. Policy, fencing, reservation and overall deadline failures end the walk.

Each vendor module implements one `Vendor` trait in `af_providers/src/web/`. It takes the key per call, holds none, and never retries inside a call, as `DecisionEvaluator` documents (`af_providers/src/decision/mod.rs:21`). Request up to ten results. Track the requested Exa content options when estimating spend. Search and standalone contents have distinct pricing. [Exa pricing](https://exa.ai/pricing)

**Cooldown.** A vendor cools only for a wait it names, parsed by `guffbolt_ai::wire::retry_after` (`guffbolt-ai/src/wire/retry/mod.rs:253`). No named wait means no cooldown. The cooldown lives in a `moka` cache in `afd_fleet` (`afd_fleet/Cargo.toml:77`), read through the daemon's clock seam.

**Extract.** Before vendor dispatch the runner rejects a URL carrying userinfo or changed by the run’s secret masker. The daemon applies its HTTPS, public-fleet and blocked-address checks. Send one URL to Parallel with `advanced_settings.full_content: true`. Accept a successful entry with full content for the single requested page. The vendor’s associated URL alone does not establish the final URL after redirects. Start with its documented dynamic fetch policy and measure freshness on a controlled changing page. [Parallel Extract API](https://docs.parallel.ai/api-reference/extract/extract)

Decode per-URL `errors[]` even on HTTP 200. For a temporary fetch failure (upstream 429 or 5xx), transport failure, vendor outage or cooling, the daemon may return the typed outcome `extract_unavailable` (502/503) with `local_read_permitted: true` only after checking live run ownership, fence and public policy again. The runner may then make one credential-free local GET under its remaining tool deadline and normal per-hop policy. A definitive per-page 4xx other than 429 returns `page_unavailable` with the upstream status. A malformed or ambiguous response, including missing full content, returns `extract_invalid_response`. Neither permits local fallback. A generic 502/503, lost response, denied policy, or reservation failure carries no permission. Expose a bounded reason to the model and omit raw upstream error content.

**State authority.** For each eligible vendor, resolve its platform-admin key, revalidate run ownership, fence and public policy, and increment `thread.run_web_calls` in a committed reservation before dispatch. A refused or failed reservation sends nothing. Release database locks before network I/O. A crash between commit and send can leave one unused reservation. Counts bound physical attempts from above and do not reconcile invoices.

A database failure or ambiguous commit ends the walk with 503 `web_reservation_unavailable`, zero dispatches under that reservation, and no local-read permission. A stale fence retains its existing refusal. A caller who receives this failure sees an unavailable tool result.

- **Dimension 1.1** — DONE. `a_vendor_call_is_counted_before_it_leaves`: the fake vendor observes its committed reservation and the explicit fast mode. Crash after commit leaves one reservation and zero calls. A failed reservation or stale fence at fallback leaves the next vendor untouched.
  Assert the 503 and its problem code, zero vendor requests, and no fallback after a reservation error.
- **Dimension 1.2** — DONE. a failed vendor falls over → Test `a_failed_vendor_falls_over_to_the_next` (integration): Parallel answers 500, Exa answers 200 → `{vendor:"exa"}`, both counted. Both answer 500 → 502 naming each vendor's status, each upstream called once.
- **Dimension 1.3** — DONE. only a named wait cools → Test `a_cooling_vendor_is_skipped_until_its_named_wait_ends` (integration, the clock seam): Parallel's 429 with `Retry-After: 30` → Exa alone answers next. At 31 seconds Parallel is tried again, and a 429 naming no wait cools nothing. Every vendor cooling → 503 with `Retry-After` and zero calls.
- **Dimension 1.4** — DONE. a vendor without a key is skipped → Test `a_vendor_without_a_vault_key_is_skipped` (integration): only the `exa` bag → Parallel sees no call. No bag at all → 503 with the new code, zero calls and no row.
- **Dimension 1.5** — DONE. a call that may not leave reaches no vendor → Test `a_web_call_that_may_not_leave_reaches_no_vendor` (integration): a `restricted` fleet → 403. A stale fence → the stale-fence code; an extract of `https://169.254.169.254/` or `http://example.org` → 400. Each case makes zero upstream calls and writes no row.
- **Dimension 1.6** — DONE. no key leaves the daemon → Test `no_vendor_key_reaches_a_runner_or_a_log` (integration): a sentinel key per vendor. Each is absent from every verb answer, run payload, error sentence and captured log line, and present only in the upstream's header.
- **Dimension 1.7** — an extract reads a public page → Test `an_extract_returns_the_pages_markdown` (integration): `{url:"https://example.org/a"}` → the upstream's Markdown. One `parallel`/`extract` row reads 1.
  Exercise per-URL errors inside a 200, a successful full-content result, and a missing full-content field. Before live acceptance, capture sanitized fixtures for a known redirect, an unreachable page and a rate-limited call.
- **Dimension 1.8** — `a_stalled_vendor_is_dispatched_once_and_answers_inside_the_deadline`: timeout and lost reply exercise the real runner client. Each vendor sees at most one request and the whole walk respects the caller deadline.
  With both vendors eligible test just below and just above the first share. Repeat at the minimum supported runner timeout and verify that one local fallback can spend only the time left.

### §2 — `web_search` is our function tool on every wire; hosted search is deleted

guffbolt gains the `SearchBackend` trait beside `Outbound` (`guffbolt/src/tools/outbound/mod.rs:158`), with `search` and `extract`. `web_search {query}` answers each result as a Markdown heading link, its date and its excerpts, within the tool output budget.

guffbolt-ai loses `Hosted`, `Request.hosted`, `WEB_SEARCH_MESSAGES` and `fn hosted`, so no wire sends a provider tool. The runner's `SearchBackend` is the verb client in `afr_tools`. `WEB_SEARCH` becomes a supervisor entry, and `Runtime::Provider`, `offer::hosted`, `HOSTED_ELSEWHERE` and `HostedToolUnavailable` go.

- **Dimension 2.1** — no wire sends a provider tool → Test `every_wire_offers_web_search_as_a_function` (unit, guffbolt-ai): each of the five wires offers `web_search` as a function. None sends `web_search_20250305`, `{"type":"web_search"}` or `googleSearch`.
- **Dimension 2.2** — results read as Markdown → Test `a_web_search_answers_its_results_as_markdown` (unit, guffbolt, a fake backend): two results → two `## [title](url)` blocks with dates and excerpts. A result with no title shows its URL.
- **Dimension 2.3** — a Chat model searches → Test `a_chat_model_searches_through_the_verb` (integration, `afr_agent`, fake Chat stream and verb): one verb call feeds the next request's tool result. A 403 reads `[network_restricted]` naming "Allow all", and the run continues.

### §3 — `web_fetch` reads a page by the fleet's policy

The runner's backend decides before any call. A host the fleet's policy names, through `allow` or the package registries, is read locally through `get_chain`, which re-admits every hop. Any other host goes to `POST L/extracts` on a public fleet and is refused on a private one, naming `request_access {host}` and "Allow all".

**Why the split.** A vendor fetches from its own servers, so a host that admits only the runner cannot be reached through it. The local path keeps today's refusal of private and metadata addresses (`af_egress/src/resolve/mod.rs:60`).

`MAX_REDIRECTS` (`af_egress/src/redirect/mod.rs:16`) becomes 20, the Fetch standard's limit: "If request's redirect count is 20, then return a network error" (fetch.spec.whatwg.org). `http_request` shares it.

With no `max_chars`, a page stops at the command output budget, 40,000 bytes (`guffbolt/src/tools/process/output.rs:22-24`). The local path parses once with `html2text`'s `parse_html` (`src/lib.rs:3130`), renders Markdown from that tree, and appends each JSON-LD block under its own heading. It refuses a body that is not text.

- **Dimension 3.1** — `an_allowlisted_page_is_read_locally_through_its_redirects`: test a local 301, 302, 200 chain. A public fleet reads across an unlisted public host if each hop is admitted. A restricted fleet stops before DNS or connection to the unlisted host. Its refusal gives the canonical hostname and names `request_access {host}`. Neither path forwards credentials across origins.
- **Dimension 3.2** — the Fetch limit holds → Test `the_twenty_first_redirect_is_refused` (unit, `af_egress`): 20 redirects are followed. The 21st stops the chain with its hop-limit reason.
- **Dimension 3.3** — an unlisted host routes by network → Test `an_unlisted_host_goes_to_extract_only_on_a_public_fleet` (unit, `afr_tools`, a fake verb): public → one extract call. Private → `[network_restricted]` naming `request_access`, with no extract and no local request.
- **Dimension 3.4** — a page keeps its structured data → Test `a_fetched_page_comes_back_as_markdown_with_its_json_ld` (unit, guffbolt): an `<h2>`, a `<ul>`, a link and one `application/ld+json` block. The answer holds `## `, `* `, `[text][1]` and the JSON-LD once; a plain `<script>` stays dropped.
- **Dimension 3.5** — the default budget holds → Test `a_fetch_without_max_chars_stops_at_the_output_budget` (unit, guffbolt): a 50,000-character page → 40,000 characters. A closing line names the total.
- **Dimension 3.6** — a binary body is named → Test `a_binary_body_is_refused_with_its_type` (unit, guffbolt): `application/pdf` on the local path → `[unsupported_media] application/pdf`, with no body bytes.
- **Dimension 3.7** — `an_extract_outage_falls_back_to_a_local_read`: one eligible public read on a typed and authorized vendor failure. Losing public access, a blocked destination or an expired deadline yields zero fallback connections. A bare gateway 503 and an extract 200 containing a permanent per-page refusal make zero local requests.

### §4 — `request_access` asks for a host

Add `{host}` alongside the existing `{name, scope?}` shape. Validate a bare DNS hostname and canonicalize it to ASCII. Refuse address literals, localhost, single-label names, and URLs with scheme, userinfo, port or path. For a refused redirect the runner binds `evidence: {tool: "web_fetch", from_host, to_host}` to the actual tool call and run. Use only the ASCII hostnames of the redirecting and refused origins. The daemon requires the known tool enum, validates both bare hostnames, checks `to_host == host` and rejects unknown fields. Direct host requests omit redirect evidence. The model supplies only the requested host.

Key grants by `(fleet_id, grant_type, service)`. Include kind in upsert, lookup, decision, revocation, policy build and every credential-only consumer, including the GitHub publisher. Existing secret and integration grants retain their behavior. The card names the host, refused tool and source of the redirect from recorded evidence. Scope the card to the fleet’s web access. Command reach also depends on the assigned runner network policy. Label the advisory decision date “Respond by”. The gate can still be answered after that date.

The grant and gate resolve together. The run whose turn raised the card receives a continuation carrying outcome, host and refused tool. Approval directs the model to retry the original read under fresh policy. Denial directs it to explain the refusal. A concurrent run finding an existing card continues with pending status and receives no continuation for that card. Revocation affects policies built afterwards. Restricted fleets keep search and extraction disabled.

Example dashboard card after a restricted fleet follows a migration guide link:

> **Allow release-assistant to reach support.vendor.example?**
>
> While reading the migration guide, `web_fetch` received a redirect from `docs.vendor.example` to `support.vendor.example`.
>
> This read needs access to `support.vendor.example`. Approving adds this host to the fleet’s web access for future runs until you revoke it. Credentials require a separate grant.
>
> **Approve** · **Deny**

Approve or Deny resumes the turn that asked. The model chooses whether to call `web_fetch` again. The dashboard can show the recorded decision independently of whether another fetch succeeds.

- **Dimension 4.1** — DONE. a host request raises one card → Test `a_host_request_raises_one_card_and_waits` (integration): `{host:"www.booking.com"}` → a `pending` `host` grant and one card. The turn settles waiting.
- **Dimension 4.2** — DONE. an approved host opens no search → Test `an_approved_host_joins_the_allowlist_and_opens_no_search` (integration): Approve → the resumed run's `allow` holds the host. `POST L/searches` still answers 403.
- **Dimension 4.3** — an address is refused → Test `a_host_request_for_an_address_is_refused` (integration): `10.0.0.5`, `localhost` and `169.254.169.254` → the refusal, no row and no card.
- **Dimension 4.4** — one kind per request → Test `a_request_naming_both_a_name_and_a_host_is_refused` (unit, `afr_tools`): both, or neither → `invalid_arguments`, and nothing is posted.
- **Dimension 4.5** — a person sees the host → Test `an access card for a host names it` (unit, vitest, `ui/packages/app/components/domain/approvals/FleetApprovalSummary.test.tsx`): the card names the ASCII host, originating host, refused tool, grant reach and lifetime. “Respond by” labels the advisory date. Deny leaves “Can access” unchanged. Feed a redirect with a signed path and query. Verify both are absent from the grant, gate, exact-action view and continuation.
- **Dimension 4.6** — DONE. The CLI lists it → Test `grant list shows a host grant` (unit, handler, `cli/test/`): a `host` grant renders its kind and host. `--json` carries `kind: "host"`.
- **Dimension 4.7** — `a_host_and_a_credential_of_one_name_stay_two_grants`: approval and revocation apply independently and a host grant cannot authorize publication.
  Assert that host-only grants cannot deliver a vault secret, appear in its holders list, produce a credential receipt, or select a credential on the card.
- **Dimension 4.8** — preserve raised, already pending and already granted as distinct wire outcomes. Race two requests: one card, and the losing run cannot infer approval from an absent card id. Repeat after an actual approval and verify the covered outcome.

### §5 — Sources reach the thread

Create the sanitized sources once and carry the same list in the live `ToolCallCompleted` frame, the saved `af_wire::tool_trace::ToolTraceCall` and the completed call in `thread.items`. On reload the dashboard reads `readSavedTrace(bodies.tool_calls)` in `fleet-stream-row.ts`. Item history and live activity must apply the same visibility rules. Older calls and refusals default to no sources. Measure each serialized envelope against `af_wire::protocol::MESSAGE_BYTES_MAX`, as `afr_agent/src/records/mod.rs` does for a posted call record. Respect the remaining space in the completion frame, call record and saved report. Keep the ordered prefix that fits both. Never truncate a URL into a different link.

Apply the run’s secret masking and URL disclosure rules before serialization, including to browser snapshots and titles. For vendor extraction cite the safely displayable requested URL unless a final destination is independently established and permitted. Keep signed redirect paths and queries masked on every surface.

- **Dimension 5.1** — a search carries its sources → Test `a_completed_search_carries_its_sources` (unit, `afr_agent`): two results → both URLs and titles, in order. A `[network_restricted]` answer carries none.
- **Dimension 5.2** — a hostile URL is not a link → Test `the run summary links only web sources` (unit, vitest, `ui/packages/app/components/domain/conversation/summary/recorded-run-resources.test.tsx`): two sources → two links with `rel="noopener noreferrer"`. `javascript:alert(1)` → text with no `href`.
- **Dimension 5.3** — `a_redirected_fetch_keeps_its_source_after_a_reload`: compare the live list to history after a reload. Test a sensitive redirect path and query, and a browser click that alone introduced its destination. No protected URL is recoverable from stored sources or titles.
  Reload from persisted turn JSON with the live stream disconnected. Compare all three representations, including an old trace without sources and a list near the frame limit.

### §6 — Spike: Chromium in the run's VM, driven from the supervisor

After M258 merges, run the spike on our Firecracker guest before building browser actions. Record the kernel, guest image and client revisions with the seven observations.

1. Record the confinement and effective renderer sandbox from the chosen configuration. M258 supplies tenant confinement. The reviewed guest disables user namespaces. A browser-specific confinement requires a decision and a real launch test.
2. Inspect the merged M258 toolbox manifest, builder and build-candidate action. Use that same image pipeline for the headless shell, libraries and fonts. At `46723c018` the manifest pins the Debian snapshot, the builder assembles the root with mmdebstrap and packs and checks the EROFS image, and the candidate action inventories and scans it. Build a baseline and browser candidate through the merged pipeline, compare their compressed sizes against its actual cap, and record the image digests and scan result. [M258 builder](https://github.com/agentsfleet/agentsfleet/blob/46723c018/scripts/toolbox/build.sh#L205-L222) · [M258 candidate action](https://github.com/agentsfleet/agentsfleet/blob/46723c018/.github/actions/build-toolbox/action.yml)
  Compare package and inventory counts, reproducibility, installed dependencies, startup smoke test and scan verdict against the merged build’s actual gates.
3. `chromiumoxide` 0.9.1 against the pinned build: connect, navigate, `Accessibility.getFullAXTree`, click by `backendDOMNodeId`, `Page.captureScreenshot`. Record every deserialize failure, and which request kinds pause at the `Fetch` stage.
4. Prove a lossless bidirectional CDP transport, including an intact 5 MiB screenshot. The existing executor route serves its own protocol. Probe supported client pipe or guest stream options before choosing an interface.
  Compare payload digests at both ends, including across a checkpoint freeze.
5. The snapshot's size on three real pages, against the tool output budget.
6. Memory after one page and after ten, and the time from start to the first snapshot, cold and warm.
7. Hold a browser open across a checkpoint, restore, and prove the disposition of old references and profile data. Verify the profile is outside the persisted workspace and that a restart clears it.
  Distinguish a boundary save while Chromium is open from final capture and restore preparation. Close and reap the browser process tree before final capture or restore. Record the real final-capture outcome and verify that restored references issue zero input events.

Repeat §8 with each supported runner network policy. For restricted fleets offer browser actions only where the effective engine policy proves enforcement of fleet destinations even when interception is unavailable. Withhold the browser when that proof is absent. Check name resolution and the browser’s real memory grant on the merged guest.
Record `Browser.getVersion`, the ten-page memory measurement against the actual VM grant, transport integrity and save/restore results. Before shipping actions also pass §8’s origin-policy trial and §9.2 on Firecracker.

- **Dimension 6.1** — the spike is recorded → Test `browser_spike_recorded` (manual): Indy reads the record. Expected: Discovery answers checks 1 to 7, and this line reads `DONE`. Evidence: this spec, in the Pull Request.

### §7 — The browser is our tool: typed verbs over a snapshot with references

guffbolt gains `Backend::Browser` and `with_browser`, as memory has `Backend::Memory` and `with_memory` (`guffbolt/src/tools/builder/catalog.rs:48-53`). A `Browser` trait in `tools/browser.rs` serves three tools with typed, `deny_unknown_fields` arguments. `Env::browser()` and `NO_BROWSER` (`guffbolt/src/tools/env/mod.rs:55,260,333`) go.

One run owns a lazily started browser, shared with its children and serialized by a mutex. The supervisor supplies the transport and executor-selected confinement proved by §6. Use an ephemeral profile outside the checkpoint and prove its lifetime.

The three tools remain sandbox tools. A snapshot names the safely displayable committed URL, title, nodes and opaque actionable references. Bind references to the browser generation and snapshot. After navigation, reconnect or restore, an old reference returns `stale_ref` even when node numbers repeat.

`screenshot` takes the `image` tool's gate. It is refused before the capture on a wire with no image input, then fitted to 2048 pixels and 5 MiB (`guffbolt/src/tools/limits.rs:41-45`). `fitted` (`files/image.rs:258`) becomes a shared helper. M259_001 supplies the usable BrowserSource and runtime check. M259_002 owns prompt text and should withhold both screenshot and image when the wire cannot accept images. M259_001 extends the request_access description just enough to name hosts. M259_002 owns the subsequent wording pass.

M258 provides executor spawn/kill, tenant memory accounting, the egress wall and the guest confinement interface. Choose the browser byte route from §6 evidence on the merged revision.

- **Dimension 7.1** — a malformed action is refused → Test `a_malformed_browser_action_is_refused_naming_the_field` (unit, guffbolt): `{action:"hover"}` → `invalid_arguments` naming `hover`. `{action:"click"}` → naming `target`; the backend is never called.
- **Dimension 7.2** — opening answers a snapshot → Test `browser_open_answers_a_snapshot_with_references` (unit, `afr_tools`, a fake CDP endpoint): one link and one button. The answer holds the title, the URL and two distinct references for this snapshot.
- **Dimension 7.3** — a click acts on a reference → Test `a_click_on_a_reference_answers_the_fresh_snapshot` (unit, a fake CDP endpoint): `click e1` → that node is clicked, then the new snapshot. `e2` from before the navigation → `[stale_ref] e2`, and no click is sent. Repeat after restore with a fresh browser that reuses a node number. No old reference can act.
- **Dimension 7.4** — a screenshot takes the image gate → Test `a_screenshot_takes_the_image_gate` (unit, a fake CDP endpoint): a 4000-pixel-wide capture → a PNG 2048 pixels wide. On the Chat wire → `image_input_unavailable`, and no capture is taken.
- **Dimension 7.5** — a browser that cannot answer says why → Test `a_browser_that_cannot_answer_says_why` (unit, `afr_tools`): no `BrowserSource` → `[browser_unavailable]` naming the engine. A session lost mid-load → `[browser_unavailable] the page closed`; no text names Firecracker.

### §8 — Every browser request meets the fleet's policy twice

The engine's egress wall (M258) is the boundary. Intercepting requests over CDP misses WebRTC and Domain Name System (DNS) traffic, and may miss WebSocket and worker requests. The check below is the second line, and the source of our refusal texts.

Use CDP request interception before navigation on each target. Apply shared egress admission to the actual URL, method and policy-relevant body of each paused request. If required input is unavailable, refuse the request. Apply resolution and blocked-address checks and continue only admitted requests.

**Dimension 8.4** — `a_browser_form_post_is_judged_by_its_method_and_body`: test a host whose policy admits a GET and refuses a form POST at the same URL. Also exercise allowed and refused bodies for one method. Each refusal reaches zero upstream requests. The Firecracker wall remains in force if the CDP connection is lost.

- **Dimension 8.1** — private and metadata addresses are refused → Test `the_browser_refuses_a_private_address` (unit, `afr_tools`, a fake CDP session, a stub resolver): a name resolving to `10.0.0.5` → failed. `169.254.169.254`, as a literal and through a name, → failed `blocked_address`.
- **Dimension 8.2** — a private fleet reaches only its allowlist → Test `the_browser_refuses_a_host_a_private_fleet_lacks` (unit): `allow: [example.org]`. `www.booking.com` fails with `http_request`'s `host_not_allowed` sentence; `example.org` continues with no header.
- **Dimension 8.3** — what the check misses meets the engine's wall → Test `a_browser_request_the_check_misses_meets_the_engines_wall` (integration, `make test-runner-kernel`): a private fleet's page. Its WebSocket and WebRTC connections to an unlisted host and to `10.0.0.5` all fail.
  Run the policy matrix against real enforcement. Include an unrestricted runner assignment paired with a restricted fleet and verify browser unavailability unless the engine separately enforces that fleet’s policy.

### §9 — The real path, end to end

- **Dimension 9.1** — search and read → Test `prime-curious-otter.spec.ts` (e2e, `make acceptance`, fixture `curious-otter`): with "Allow all" on, a person asks for a search and the top page. The turn holds a `web_search` and a `web_fetch` call, and "Sources" lists the URLs. The run has one `search` and one `extract` row.
- **Dimension 9.2** — open, act, look → Test `prime-nosy-magpie.spec.ts` (e2e, `make acceptance`, fixture `nosy-magpie`, a Messages-wire model): the fleet opens the lane's test page `[?]`. It clicks a link by its reference and reports the new title, and its screenshot reaches the model.
- **Dimension 9.3** — `approving_a_redirected_host_finishes_the_original_read` (af_e2e): a scripted model follows the retry instruction: zero contacts before Approve, one after the resumed fetch, and the final answer contains the page sentinel. A second script declines to retry: the continuation arrives, the destination sees zero requests, and the UI never reports a successful read.
- **Dimension 9.4** — `denying_a_redirected_host_contacts_neither_it_nor_a_vendor` (af_e2e): zero destination DNS lookups, connections, vendor requests and reservations. The grant is revoked and the resumed agent explains the denial.

## Interfaces

**Tools.**

```
web_search {query}                  "## [title](url)", date, excerpts; refusal [network_restricted] <sentence>
web_fetch  {url, max_chars?}        Markdown, then each JSON-LD block; max_chars defaults to the output budget
request_access {name, scope?} | {host}
browser_open {url}                  the snapshot: title, URL, a line per node, a reference per actionable node
browser {action:"read", find?}      the snapshot again; with find, only nodes whose text matches
        {action:"click", target} · {action:"type", target, text, submit?}     target: a reference such as e7
        {action:"scroll", direction:"up"|"down"} · {action:"back"}
screenshot {}                       a PNG through the image gate
refusals  [browser_unavailable] · [stale_ref] · [host_not_allowed] · [image_input_unavailable]
```

**Run verbs.** Search and extract use the fence and call number. Each sends once. Access keeps its single-send client and reports the typed decision state.

```
POST L/searches {fencing_token, call_number, query}  200 {vendor, results: [{url, title?, published?, excerpts[]}]}
POST L/extracts {fencing_token, call_number, url}    200 {vendor, url, title?, content}   content is Markdown
POST L/access   {fencing_token, name, scope?} | {fencing_token, host, evidence?: {tool:"web_fetch", from_host, to_host}}   grant_id, state: raised|pending|covered, gate_id?
```

**Codes.** Web-specific failures occupy the UZ-WEB family. Recheck allocation against merged M258 before the browser spike. Stale fences keep the existing refusal.

```
403  a private fleet's web call                 503  no vendor key is configured
503  every vendor is cooling, with Retry-After   502  every vendor failed, naming each status
400  an extract URL or a host request names an address
503  web_reservation_unavailable (walk ends)     502  page_unavailable or extract_invalid_response
502/503  extract_unavailable (local_read_permitted explicitly authorized for this call)
```

**Runner wire.** `ToolCallCompleted` and `ToolTraceCall` gain `sources`, and `AccessRequest` takes `name` or `host`. guffbolt-ai's `Request` loses `hosted`. `VERSION` (`af_wire/src/protocol/mod.rs:15`) moves once.

**Schema.** `thread.run_web_calls` is a new table: `run_id` (`uuid`, a foreign key to `thread.runs` with `ON DELETE CASCADE`), `vendor`, `operation`, `call_count` and the two time columns, keyed by the first three. Grant uniqueness includes fleet, kind and service. Completed tool call items have nullable sources. Vendor, operation and the new `grant_type` `host` of `connector.integration_grants` are application constants. Counts follow run retention. Test run deletion and fleet purge. Longer-lived spend reporting uses operational metrics and vendor records.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| A private fleet searches | security | the model calls `web_search` on a `restricted` fleet | the runner refuses first, the daemon again; zero vendor calls | `a_web_call_that_may_not_leave_reaches_no_vendor` |
| Server-side request forgery (SSRF) by extract | security | a URL names a metadata or private literal | 400 before any call | `a_web_call_that_may_not_leave_reaches_no_vendor` |
| Key leak | security | a key rides a payload, a sentence or a log | no key leaves the daemon | `no_vendor_key_reaches_a_runner_or_a_log` |
| Redirect out of policy | security | a restricted fleet’s allowlisted host redirects to an unlisted one | `host_not_allowed`; the second host gets nothing | `an_allowlisted_page_is_read_locally_through_its_redirects` |
| Vendor down or rate-limited | failure | a 5xx or 429 | the next vendor answers; 502 only when all fail | `a_failed_vendor_falls_over_to_the_next` |
| Every vendor cooling | capacity | each named a wait | 503 with `Retry-After`; no call | `a_cooling_vendor_is_skipped_until_its_named_wait_ends` |
| No vendor configured | failure | no key bag in the platform-admin vault | 503 with its code; no call | `a_vendor_without_a_vault_key_is_skipped` |
| Lost reply | failure | the runner cannot tell whether a call left | return the unknown outcome within the deadline and do not redispatch | Dimension 1.8 |
| Stale run | failure | a superseded fence posts | the stale-fence code; no call | `a_web_call_that_may_not_leave_reaches_no_vendor` |
| Hostile source | security | a result's URL is `javascript:` | shown as text, never linked | `the run summary links only web sources` |
| Unreadable body | failure | a PDF on the local path | `[unsupported_media]` naming its type | `a_binary_body_is_refused_with_its_type` |
| Browser reaches a private address | security | a page sends Chromium to `10.0.0.5` or the metadata address | failed by the check; stopped by the engine's wall | `the_browser_refuses_a_private_address` |
| A request kind the check never sees | security | WebRTC, DNS, a WebSocket or a worker | the engine's wall stops it | `a_browser_request_the_check_misses_meets_the_engines_wall` |
| Lost CDP session | failure | the stream into the VM drops | requests stay paused; `[browser_unavailable] the page closed` | `a_browser_that_cannot_answer_says_why` |
| Stale reference | failure | the page changed since the snapshot | `[stale_ref]`; no click sent | `a_click_on_a_reference_answers_the_fresh_snapshot` |
| Malformed action | failure | an unknown or partial action | `invalid_arguments` naming the field | `a_malformed_browser_action_is_refused_naming_the_field` |

## Invariants

1. No vendor key leaves the daemon — only §1's handler reads a key bag, per call; no wire type has a field for one (Dimension 1.6).
2. A private fleet sends a vendor nothing — the daemon reads `http_egress` from the fleet row before the vendor walk (Dimension 1.5).
3. Every vendor dispatch has a committed reservation. A crash before dispatch can leave a reservation unused (Dimension 1.1).
4. No wire sends a provider-hosted tool — `Hosted` is deleted, so no code can name one (Dimension 2.1).
5. Local requests share the egress policy — `web_fetch`, `http_request` and the browser check all call `Egressed` (Dimensions 3.1, 8.2).
6. End the browser process tree and confirm exit before run-end capture. Prove intermediate save, final capture, restore and stale references separately (§6 and Dimension 7.3).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `web_call` (log) | operator | a vendor call returns | `run_id`, `fleet_id`, `vendor`, `operation`, `outcome`, `ms`, `vendor_status` | no query, no URL, no key | `a_vendor_call_is_counted_before_it_leaves` |
| `agentsfleet_web_calls_total{vendor,operation,outcome}` | operator | the same | vendor, operation, outcome | labels only | `a_failed_vendor_falls_over_to_the_next` |
| `web_vendor_cooling` (log) | operator | a vendor names a wait | `vendor`, `wait_ms` | no key | `a_cooling_vendor_is_skipped_until_its_named_wait_ends` |
| `fleet_access_changed` | product | a host grant is requested, approved, denied or revoked | `fleet_id`, `kind` (`host`), `change` | no host name | `a_host_request_raises_one_card_and_waits` |
| `browser_request_refused` (log) | operator | the check fails a request | `run_id`, `fleet_id`, `reason`, `host` | no path, query, header or body | `the_browser_refuses_a_host_a_private_fleet_lacks` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A search answers through the first working vendor, and each dispatch has a committed reservation (§1) | `make test-integration-rustd` | exit 0; `a_vendor_call_is_counted_before_it_leaves ... ok`, `a_failed_vendor_falls_over_to_the_next ... ok`, `a_cooling_vendor_is_skipped_until_its_named_wait_ends ... ok` | P0 | |
| R2 | A private fleet sends nothing, and no key leaves the daemon (§1) | `make test-integration-rustd` | exit 0; `a_web_call_that_may_not_leave_reaches_no_vendor ... ok`, `no_vendor_key_reaches_a_runner_or_a_log ... ok` | P0 | |
| R3 | Every wire offers our `web_search` and no provider tool (§2) | `make test-unit-all` | exit 0; `every_wire_offers_web_search_as_a_function ... ok` | P0 | |
| R4 | No hosted-search code is left (§2) | `git grep -n -w -e WEB_SEARCH_MESSAGES -e HOSTED_ELSEWHERE -e HostedToolUnavailable -e hosted_tool_unavailable -e web_search_20250305 -- rustd` | 0 matches | P0 | |
| R5 | `web_fetch` routes by policy, follows 20 redirects and keeps JSON-LD (§3) | `make test-unit-all` | exit 0; `an_unlisted_host_goes_to_extract_only_on_a_public_fleet ... ok`, `the_twenty_first_redirect_is_refused ... ok`, `a_fetched_page_comes_back_as_markdown_with_its_json_ld ... ok` | P0 | |
| R6 | A fleet asks for a host and a person approves it once (§4) | `make test-integration-rustd` | exit 0; `a_host_request_raises_one_card_and_waits ... ok`, `an_approved_host_joins_the_allowlist_and_opens_no_search ... ok`, `a_host_request_for_an_address_is_refused ... ok` | P0 | |
| R7 | Sources show, and a hostile URL is not a link (§5) | `make test-unit-all` | exit 0; `a_completed_search_carries_its_sources ... ok`; `the run summary links only web sources` passed | P1 | |
| R8 | The spike is recorded before the browser build (§6) | `git grep -n -e "Dimension 6\.1\*\* — DONE" -- docs/v2` | one line | P0 | |
| R9 | The browser acts on references, takes the image gate and refuses what the policy refuses (§7, §8) | `make test-unit-all` | exit 0; `a_click_on_a_reference_answers_the_fresh_snapshot ... ok`, `a_screenshot_takes_the_image_gate ... ok`, `the_browser_refuses_a_private_address ... ok`, `the_browser_refuses_a_host_a_private_fleet_lacks ... ok` | P0 | |
| R10 | No browser refusal text is left (§7) | `git grep -n -w -e WAITS_FOR_FIRECRACKER -e NO_BROWSER -e test_browser_tools_refuse_until_firecracker -- rustd` | 0 matches | P1 | |
| R11 | The live paths work (§9) | `make acceptance SUITE=all` | exit 0; `prime-curious-otter.spec.ts` and `prime-nosy-magpie.spec.ts` passed | P1 | |
| R12 | The asking turn resumes and its scripted retry reads only after approval (§9.3–4) | `make test-integration-rustd` | both af_e2e redirect grant cases pass | P0 | |
| R13 | Host and credential grants keep separate authority, dispatch is bounded, and reloaded sources preserve redirect privacy (§1, §4, §5) | `make test-integration-rustd` | Dimensions 1.8, 4.7–8 and 5.3 pass | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behaviour evals

- **Grounding rule:** an answer names only URLs that the run's `web_search`, `web_fetch` or browser returned.
- **Golden set:** the follow-ups in `curious-otter.json` and `nosy-magpie.json`, including the worst known case: booking.com asked of a private fleet. Each failure found in use becomes a follow-up.
- **Ship threshold:** grounding holds in every case, checked by R11, whose specs compare the answer's URLs with the turn's sources.
- **Fallback:** a fleet that cannot search says so and names "Allow all". A fabricated URL fails R11.

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the hosted wire specs | `git grep -n -w -e WEB_SEARCH_MESSAGES -e web_search_20250305 -e provider_tools -- rustd` | no output |
| the hosted type and its plumbing | `git grep -n -w -e Hosted -e HOSTED_ELSEWHERE -e HostedToolUnavailable -e hosted_tool_unavailable -e HOSTED_REFUSAL -- rustd/guffbolt rustd/guffbolt-ai rustd/crates/runner rustd/crates/e2e` | no output |
| the provider runtime | `git grep -n -e "Runtime::Provider" -- rustd` | no output |
| the browser refusal | `git grep -n -w -e WAITS_FOR_FIRECRACKER -e NO_BROWSER -e test_browser_tools_refuse_until_firecracker -- rustd` | no output |

**Original author’s symbol inventory. Recheck on merged main.** Production: `guffbolt-ai/src/{lib.rs:47,types/mod.rs:10,types/model.rs:21,26,48,wire/request/mod.rs:12,29,79,84-91}`, `guffbolt/src/lib.rs:111`, `afr_agent/src/{loop/mod.rs:22,214,offer/mod.rs:11,55,65-66,router/mod.rs:23,119,172}`, `afr_tools/src/{catalog/mod.rs:64,315,runtime/mod.rs:66,128,152,stub/mod.rs:49,generic/env/mod.rs:48-49,304,generic/env/lazy.rs:12,95}`, `guffbolt/src/tools/env/mod.rs:55,334`, `guffbolt/examples/fireworks_live.rs:27,197`.

Tests: `guffbolt-ai/src/wire/request/tests.rs:8,79,243-254`, `guffbolt-ai/src/wire/route/tests/counting/unsupported.rs:13`, `afr_agent/src/{offer/tests.rs:13,48,142-143,router/tests.rs:173,222-233}`, `afr_agent/tests/providers/main.rs:51,202`, `afr_tools/src/{catalog/tests.rs:247,338,runtime/tests.rs:10-11,generic/env/tests.rs:17,135,sandbox/browser/tests.rs:40,85}`, `guffbolt/tests/public_api/{plain/answers.rs:135,seam.rs:87}`, `af_e2e/tests/support/definition_run.rs:237`.

Same spelling, other meaning, kept: `afd_http/src/openapi/mod.rs` ("Hosted schedules"); the engine's own Firecracker mentions, which M258 owns (`rustd/Cargo.toml:996`, `afd_runner/src/reconcile.rs:11,110`, `runner/main/src/main_tests.rs:120`, `scripts/test-runner-startup.sh:219`, `docs/architecture/runner_execution.md:312`).

## Out of Scope

- M258 owns the compute engine and its network wall. M259 owns the Firecracker browser spike and the changes it proves necessary.
- The prompt's tool, speaker and environment text, the two approved tool fixes, and images in a message: M259_002.
- A per-fleet or per-workspace vendor choice, and search on a private fleet: ruled out.
- Browser cookies across runs, solving a CAPTCHA, and a forward proxy: not planned.
- Tabs, key presses, select, hover, drag, upload, script evaluation and console logs in the browser: not planned until a fleet needs one.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person turns on "Allow all" and asks a fleet on any model to compare hotels. The answer lists its sources, and the fleet can open a page and click.
2. **Preserved user behaviour** — `http_request`, approved hosts, the access card and every model's chat keep working. `web_fetch` of an allowlisted host still runs locally.
3. **Optimal-way check** — one search tool, a fixed vendor walk, one count table, and Chromium in the VM the run already has. The gap was hosted search on 8 of 21 models, and no browser.
4. **Rebuild-vs-iterate** — rebuild how search reaches a model and how the browser is reached; iterate on `web_fetch` and `request_access`.
5. **What we build** — two daemon verbs over a vendor walk with a per-vendor count, `SearchBackend`, `web_search` and a routed `web_fetch`. Also a host grant, sources, and three typed browser tools with a request check.
6. **What we do NOT build** — a vendor choice per fleet, because the platform picks, or spreading and fan-out across vendors, which cost more. Search on private fleets or hosted search, both ruled out, and externally hosted browsers.
7. **Fit with existing features** — it compounds with "Allow all", the access card and the `image` gate. It must not destabilize the network policy's host rule, which every HTTP path now shares.
8. **Surface order** — API and runner first, then the app's card and Sources, then the CLI's lists, because both read the daemon's new shapes.
9. **Dashboard restraint** — "Sources" shows only when a run called a web tool; no search settings page exists.
10. **Confused-user next step** — a refused search names "Allow all" and `agentsfleet fleet network use <fleet> allow-all`; a refused page names `request_access`, and the card asks the person.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the daemon's verbs first, so the runner's backend has a target. Then the tools, the host grant and the sources; the browser last, behind its spike.
- **Alternatives considered:**
  - Hybrid search, hosted where a provider offers it: ruled first, then replaced by Indy.
  - A random vendor per session with a consent prompt: declined; the platform holds the keys and the fleet's `http_egress` is the consent.
  - A runner that calls a vendor with a per-call key: declined; a tenant runner never holds a key.
  - CSS selectors as targets: declined; a model cannot know a selector it never read.
  - The CDP check as the only wall: declined; it misses WebRTC and DNS.
- **Patch-vs-refactor verdict:** a **refactor** because the hosted path leaves two published crates, search moves into the daemon, and the browser leaves `Env`.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "4. I want to replace the engine first 5. Then the web search with parallel + other providers like opencode and the browse decision 6. so 2 milestones, in the same milestone i need the tools of guffbolt to be improved with the system prompts and such proposed like opencode" — context: the engine first; then this spec, with M259_002 for tools and prompts.
- > Owner (Indy, Nov 26, 2026): "3. Is headless chromium the simplest or anyothers equivalent to chromium" — context: answered below; `chrome-headless-shell` is the pick.
- > Owner (Indy, Nov 26, 2026): "bubblewrap must be retired if we do 3rd party sprites based firecraker or any sandbox or firecracker engine our own." — context: M258 retires it; Chromium then runs in the run's VM.
- > Owner (Indy, Nov 26, 2026): "I want to know how to improve ourselves for the Browser? is there any way else than the browser, the tool must be more like we do in sprites or our own.. just like our memory tool? Also for search Parallel is one, you should be able to do with others like opencode does? we could borrow from there So record the above two." — context: several vendors behind one trait; the browser built like memory.
- > Owner (Indy, Nov 26, 2026): "Hybrid", then "Does parallels offer search too", then "I think i wanna take the call to use like amp for websearch and avoid hybrid?" — context: our function-tool `web_search` on every wire; hosted search is deleted.
- > Owner (Indy, Nov 26, 2026): "i think though this is billable, it must be convered to the platform runtime cost?" — context: no usage line; the runtime fee covers vendor cost, and calls are counted.
- > Owner (Indy, Nov 26, 2026): "Why do you need screenshot", "when will you have this usecase? i am leaning towards a and b" — context: pixels only where text is not enough; the snapshot is the reading path.
- > Owner (Indy, Nov 22, 2026): "the tool can be cut on the browser side keep it in the current form, to get the browser working what do we need to do" — context: the three tool names stay. M253_004's earlier fold into one `browser` was never built (`afr_tools/src/catalog/mod.rs:131-135`).
- > Owner (Indy, Nov 26, 2026): "Essentially add missing features for guffbolt agent harness", "max 2 in 1 milestone for the guffbolt tool updates" and "I need the final scope to fix" — context: two Workstreams in this milestone.
- **Owner decisions, Nov 26, 2026, on the gap matrix** — accepted: Gemini grounding off, and no search on a `restricted` fleet. Page reading goes through a vendor on public fleets only.
- **Nov 27 owner decisions.** Firecracker only. Parallel Fast, Exa search fallback and platform-owned vendor keys. Indy subsequently authorized §§1–5 on this branch alongside M258 and kept the browser spike after merge.
- **Nov 27 owner direction.** The implementer decides the browser design after M258 merges, using the toolbox pipeline that lands and the Firecracker spike evidence. D1, D2 and D4 are post-merge implementation decisions. D3 concerns what the thread can disclose.
  - D1: choose browser confinement after reviewing the isolation trade-off. A disabled renderer sandbox is a proposal, not an accepted launch configuration.
  - D2: verify boundary save, final capture and restore preparation on merged M258. Claude reports that a boundary save freezes a live tenant on its local branch and final capture requires tenant processes to exit. Preserve the workspace save, reap Chromium for final capture, and invalidate browser references after restore.
  - D3 (owner decision pending): recommend the safely displayable requested link and only the final origin on a redirected fetch. Strip sensitive arguments before persistence and suppress the redirect path and query. Use the requested link alone when the final origin is unknown.
  - D4: measure the Debian shell and dependencies in the single toolbox before deciding whether to raise its cap.
- **Denial and tool-offer scope.** Pending owner decision: limit a repeated request for the same denied host to a later human-initiated turn. The existing grant can reopen on a new request. For §2, keep the restricted-fleet refusal and explain “Allow all”. Revisit offer-time filtering when M259_002 builds its environment prompt.
- **Verify before live acceptance.** Confirm vendor retention and resale terms, Parallel’s returned extraction URL and rate-limit headers, vault provisioning, transport support, ephemeral profile behavior and the real Firecracker image size.
- **Known limit, needs an owner disposition** — no browser engine passes a bot wall. Every headless stack failed a Cloudflare managed challenge in an October 2026 test, and so did a headed browser on a cloud address. A vendor's page reader stays the route for reading those sites.
- **Cost monitoring.** Reservations bound the number of attempted dispatches. Vendor invoices settle charges. Cached results and requested content options can affect cost or freshness.
- **Search references.** OpenCode’s per-query attempted set and provider progress informed §1. Its random selection falls back on HTTP 429 and uses 60 seconds when Retry-After is missing. Fixed ordering, fallback on other vendor failures and cooldown only for a named wait are M259 decisions. See the pinned sources in Prior-Art.
- **Consults, browser** — the snapshot with references is where Playwright MCP, Chrome DevTools MCP, agent-browser and opencode all converged.
- **Engine choice** — `chrome-headless-shell`, version-pinned, driven by `chromiumoxide` 0.9.1, the most used maintained tokio CDP client. Firefox is blocked less when headless but has only WebDriver BiDi, with no maintained Rust client; Lightpanda stays dropped (AGPL-3.0, no pixels).
- **Corrections to the source specs:**
  - The catalogue holds 21 models at `679002666` (Chat 11, Gemini 2), not 20.
  - The sources test file is `recorded-run-resources.test.tsx`; `RunResources.test.tsx` does not exist.
  - The blast list adds `afr_agent/src/router/tests.rs:173,233` and `afr_agent/tests/providers/main.rs:51,202`.
- **Restructure record** — this spec takes §1 to §5 and §8 of the web search spec and the browser from the browser spec, both drafted on Nov 26, 2026 and removed by the Pull Request that adds this one. The web spec's §6, §7 and §9 moved to M259_002.
- **Model review, Nov 27.** Fable 5.2 supplied eleven findings. Verified the key collision, retry and reservation boundaries, pending/covered ambiguity, redirect privacy, call history persistence, and compute prerequisites against `ece273764` and `46723c018`. The review also cites `680a26110`; recheck that revision or its successor after merge.
- **Claude Code review, Nov 27.** Verified main and published M258 at `ece273764` and `46723c018`. Claude inspected the additional local-only revisions `680a26110` and `121dc6918`. Refresh the guest paths, runner policies, checkpoint behavior, image limits and wire version from merged main. The web tools can proceed independently of the browser spike.
- **Delivery.** Start §§1–5 on the current main baseline. Sync the feature branch with merged M258 before §6. Reconcile shared route inventories, schema registration, error codes, protocol version, OpenAPI and acceptance setup there. Run §6 before §§7–8 and the live browser fixture.
  Start with the reservation table and migration registration, vendor interface, and Dimension 1.1 against a fake vendor. The spike is ready once the merged engine is available. Shipping browser actions additionally requires confinement evidence, policy isolation, intact transport, successful final capture and §9.2.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
