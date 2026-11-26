<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M258_001: Every model searches the web, reads pages and sees a person's images, inside the fleet's network policy

**Prototype:** v2.0.0

**Milestone:** M258

**Workstream:** 001

**Date:** Nov 26, 2026

**Status:** PENDING

**Priority:** P1, a fleet asked to "search booking.com and recommend" fails on every path today, and 12 of the 20 platform models offer no search at all

**Categories:** API, CLI, DOCS, INFRA, OBS, UI

**Batch:** B1, alone; M258_002's browser follows it

**Depends on:** none

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from the harness gap matrix, the Gemini review and Indy's rulings of Nov 26, 2026, each reference re-read at `576d0b462`

**Canonical architecture:** `docs/architecture/runner_execution.md` §Tool catalog and `docs/architecture/billing.md`; this spec rewrites the `web_search` and `web_fetch` rows and names the search cost the runtime fee covers

---

## Overview

**Goal (testable):** On every wire, a public fleet's `web_search` answers Parallel's results and its `web_fetch` reads a public page through Parallel Extract, each call is counted on its run, and a private fleet's search is refused naming the switch.

**Terms.** Rust paths that start with a crate name are under `rustd/crates/<layer>/`; `guffbolt/` and `guffbolt-ai/` are under `rustd/`. L is `/v1/runners/me/runs/{run_id}`. Line references are at `576d0b462`.

A **public** fleet has `http_egress` `public` ("Allow all" on); a **private** one has `restricted`, the default at birth.

**Problem:** Read from source.

- **Search is missing on 12 of 20 platform models.** `web_search` is a provider-hosted tool sent only on Messages and Responses; Chat, Gemini and Ollama get nothing (`guffbolt-ai/src/wire/request/mod.rs:84-93`). The catalogue holds 4 Messages, 4 Responses, 11 Chat and 1 Gemini row (`af_observability/src/semconv/model-library-allowlist.json`).
- **Hosted search escapes the fleet's policy.** The provider searches, and on Responses opens pages, outside `af_egress` (`af_egress/src/lib.rs:1-11`), so a private fleet can still send data to a search engine.
- **A default fleet cannot read the page.** A fleet is born `restricted` (`afd_setup/src/creation/create.rs:273-274`). Its `allow` holds only approved secrets' hosts and the GitHub API host (`afd_gate/src/policy/shape/mod.rs:36-59`), so `web_fetch` answers `host_not_allowed` (`af_egress/src/policy/mod.rs:198-213`).
- **The fleet cannot ask for a host.** `request_access` asks for a vault secret or an integration only (`afr_tools/src/verbs/access/mod.rs:1-2,36-43`).
- **`web_fetch` stops at the first redirect** (`guffbolt/src/tools/web/web_fetch.rs:51`), returns the whole page when `max_chars` is absent (`:66-69`), and returns a binary body as lossy text (`:61-65`).
- **Nothing counts a search.** No code reads or counts one (a `git grep` of `rustd/` for `web_search_requests` and `server_tool_use` finds nothing), and `billing.md` names none.
- **A person cannot show a fleet a picture.** A run's input is the text of `message` alone (`af_wire/src/turn/message.rs:14-24`), though four wires show a tool's image to the model (`guffbolt-ai/src/wire/route/mod.rs:36-48`).
- **The prompt misleads.** It names `task_start` to runs that do not hold it (`afr_agent/src/prompt/mod.rs:49-51` against `afr_agent/src/offer/mod.rs:19-26`), and no prompt block carries today's date.

**Solution summary:**

- **One search tool on every wire.** `web_search` is a guffbolt function tool over a `SearchBackend` trait. Provider-hosted search is deleted from guffbolt-ai, with no alias.
- **Parallel behind the daemon.** The daemon calls Parallel Search and Parallel Extract with the platform key from the platform-admin vault, so no runner holds it. Exa or Tavily later is one more `SearchBackend` implementation.
- **The policy decides first.** A private fleet's search and Extract are refused before any call, because a query or a URL carries data out. A host on the fleet's allowlist is read locally, following redirects that are each re-checked.
- **Counted, not billed per call.** Parallel runs on one platform key, and the runtime fee covers its cost. The daemon counts each run's calls for our own cost tracking.
- **The fleet can ask for a host,** and a person approves it once on the card `request_access` already raises.
- **Sources reach the thread,** and the prompt names only the tools the run holds, the fleet's network and today's date.
- **A person attaches images** to a message through the API, the web app and the CLI, and models that take images see them.

**Design flow:**

```
web_search {query} ─► runner SearchBackend ─► POST L/searches ─► daemon
                                                 1. fence  2. fleet public?
                                                 3. key `parallel` from the platform-admin vault
                                                 4. POST https://api.parallel.ai/v1/search ─► results
                                                 5. the run's search_calls + 1, committed before the answer
web_fetch {url} ─► runner SearchBackend: host on the fleet's allow list? ─yes─► af_egress local GET, ≤20 redirects, each hop re-admitted
                                         │no
                                         ├─ public fleet ─► POST L/extracts ─► daemon ─► Parallel Extract (renders JavaScript)
                                         └─ private fleet ─► [network_restricted] names request_access {host} and "Allow all"
request_access {host} ─► card ─► Approve ─► NetworkPolicy.allow gains the host ─► local reads, the sandbox, M258_002's browser check
```

## Rulings

**Ruled by Indy on Nov 26, 2026** (quotes in Discovery). "Hybrid" was ruled first and then replaced.

| # | Ruling |
|---|---|
| Search | Our own function-tool `web_search` on every wire, Messages and Responses included. Provider-hosted search is no longer sent. |
| Vendor | Several search vendors behind one `SearchBackend` trait, as opencode does; Parallel is one of them. The vendor set, the selection and fallover order, the cooldown on a refusal and the counting per vendor are borrowed from opencode's design (research, `[?]` until recorded). |
| Pages | `web_fetch` reads a public page through Parallel Extract, which renders JavaScript on Parallel's servers. |
| Private fleets | A `restricted` fleet can neither search nor use Extract. |
| Gemini | Google Search grounding stays off; Gemini uses our `web_search`. |
| Cost | Parallel runs on one platform key and is not a usage line: the runtime fee, `RUN_NANOS_PER_SEC` = 100,000 nanos (USD 0.0001 a second, `afd_billing/src/nanos.rs:25`), covers it. Calls are counted per run for our own cost tracking. |
| Images | A person attaches images through the API, and both the web app and the CLI send them. |

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(web): web_search and web_fetch on every model through Parallel; images in messages
- **Intent (one sentence):** Any fleet on any model can search the web, read the page it found and see a person's pictures, within what a person allowed.
- **Authoring handshake:** Indy wants search and page reading to work the same on every model, from one vendor behind a trait, with private fleets kept private. ASSUMPTIONS I'M MAKING: 1. The daemon makes every Parallel call, for platform and tenant runners alike (Discovery, agent default). 2. Nothing is backward compatible: `Hosted`, the hosted wire specs and `hosted_tool_unavailable` are deleted. 3. Parallel's cost rides the runtime fee; no per-call price is stored.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/runner/afr_supervisor/src/verbs/decision/mod.rs` — a fenced verb for one platform-keyed call; §1's verbs keep its fence and move the call into the daemon.
2. `rustd/crates/runner/afr_tools/src/egress/mod.rs` — `get_chain` (`:195-230`) re-admits every redirect target; §3's local path uses it unchanged.
3. `rustd/guffbolt/src/tools/web/web_fetch.rs` — the tool §3 routes; it already renders Markdown through `html2text`'s plain config.
4. `rustd/crates/runner/afr_tools/src/verbs/access/mod.rs` — `request_access`, which §4 extends to a host.
5. `rustd/crates/runner/afr_agent/src/prompt/mod.rs` — the prompt §6 derives from the offered tools.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M258_001_P1_API_CLI_DOCS_INFRA_OBS_UI_WEB_SEARCH_AND_FETCH_ON_EVERY_MODEL.md` | CREATE | this spec; moves to `active/`, then `done/` at close |
| `rustd/guffbolt-ai/src/{types/model.rs,types/mod.rs,lib.rs,wire/request/mod.rs,wire/request/tests.rs,wire/route/tests/counting/unsupported.rs}`, `rustd/guffbolt-ai/README.md` | EDIT | §2: `Hosted`, `Request.hosted`, `WEB_SEARCH_MESSAGES` and `fn hosted` deleted |
| `rustd/guffbolt/src/{lib.rs,tools/mod.rs,tools/web/**,tools/builder/**,tools/search.rs,tools/files/listing/grep.rs}`, `rustd/guffbolt/README.md`, `rustd/guffbolt/examples/fireworks_live.rs`, `rustd/guffbolt/tests/public_api/**` | EDIT/CREATE | §2 and §3: `SearchBackend`, `web_search` and routed `web_fetch`; §7: `tool_search` and `grep` |
| `rustd/crates/harness/af_providers/src/web/**` | CREATE | §1: the Parallel implementation of `SearchBackend` |
| `rustd/crates/harness/af_egress/src/{redirect,error}/**` | EDIT | §3: `MAX_REDIRECTS` 20; §4: `host_not_allowed` names `request_access` |
| `rustd/crates/protocol/af_wire/src/{activity,access_verb,protocol,turn,run}/**`, `af_wire/src/web/**` | EDIT/CREATE | §1 verb types; §4 the host request; §5 `sources`; §9 a turn's images; the runner protocol `VERSION` moves once |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/{web.rs,access.rs}`, `afd_http/src/route/*.rs`, `route_inventory.rs`, `route_meta_total.rs` | CREATE/EDIT | §1's two verbs and §4's host request; `INVENTORY` and the counts |
| `rustd/crates/daemon/afd_fleet/src/run/web/**` | CREATE | §1: fence, network, vendor call and the run's count, in that order |
| `rustd/crates/daemon/afd_db/schema/20261108123200_thread_runs.sql` | EDIT | §1: `search_calls` and `extract_calls`, edited in place before the first production deploy |
| `rustd/crates/daemon/afd_api_tenant/src/**`, `afd_turns/src/**`, `afd_api_wire/src/**` | EDIT | §9: `POST F/messages` and `POST W/fleets` take `images`, and the turn keeps them |
| `rustd/guffbolt-ai/src/{types/message.rs,wire/request/**,wire/image.rs}` | EDIT | §9: a user message carries images on the wires that take them |
| `rustd/crates/daemon/afd_db/schema/*_connector_integration_grants.sql`, `afd_api_wire/src/grant.rs`, `afd_approval/src/**`, `afd_gate/src/policy/{shape,approved,build}/**` | EDIT | §4: `grant_type` `host`; an approved host joins `NetworkPolicy.allow` |
| `rustd/crates/foundation/af_core/src/{error_code,problem}/**` | EDIT | the new codes in Interfaces, each with its `REGISTRY` row and problem entry |
| `rustd/crates/runner/afr_tools/src/{catalog,runtime,stub,verbs,egress,backed}/**` | EDIT/CREATE | §2: `Runtime::Provider` and `HostedToolUnavailable` go; the verb client is the runner's `SearchBackend` |
| `rustd/crates/runner/afr_agent/src/{offer,router,loop,prompt}/**`, `afr_agent/tests/providers/**` | EDIT | §2: no hosted list; §5: sources on the completed call; §6: the prompt |
| `rustd/crates/e2e/af_e2e/tests/support/definition_run.rs` | EDIT | it filters on `Runtime::Provider`, which goes |
| `deploy/local/bootstrap.ts`, `deploy/local/bootstrap.test.ts`, `deploy/local/acceptance.sh` | EDIT | the local platform-admin vault gets `parallel` from `PARALLEL_API_KEY`, and initial setup refuses without it |
| `.github/workflows/acceptance-on-demand.yml`, `.orly/orly.json` (`burst`) | EDIT, with Indy's approval at build | `PARALLEL_API_KEY` from 1Password; the item name is unverified |
| `ui/packages/app/components/domain/conversation/**`, `ui/packages/app/components/domain/approvals/**`, `ui/packages/app/components/domain/fleet/**`, `ui/packages/app/lib/**` | EDIT | §4: a host card and "Can access" row; §5: the Sources list; §9: the composer attaches images |
| `cli/src/commands/fleet/**`, `cli/test/**` | EDIT | §4: `fleet grants list` and `fleet approvals list` show a host; §9: `fleet create` and `fleet steer` take `--image` |
| `tests/fixtures/fleets/curious-otter.json`, `ui/packages/app/tests/e2e/acceptance/prime-curious-otter.spec.ts` | CREATE | the end-to-end case: a public fleet searches, reads the top page and shows its sources |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` |
| `docs/architecture/{runner_execution,billing,runner_protocol,thread_model}.md` | EDIT | the tool rows, the search cost inside the runtime fee, the new verbs, a turn's images |
| `~/Projects/docs` tool, billing and API pages, `changelog.mdx` | EDIT, on that repository's own branch after merge | the published behaviour |

## Applicable Rules

- **`domains/http.md`, `domains/http-responses.md`, `domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-NEWCODE, HTTP-REGISTRY, HTTP-LEAK, HTTP-OPENAPI. **`domains/sql.md`** — SQL-FILE, SQL-COUNTER, TXN. **`domains/auth.md`** — AUTH-NOLOG, BIL.
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE. **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE. **`code.md`** — NLG, ORP and NDC for the hosted path; CODE-SEAM; PRI for vendor text that reaches a prompt.
- **`tests.md`** — TEST-NEG, TEST-REDGREEN, TEST-COUNTER, TEST-RACE, TCF. **`domains/logging.md`** — LOG-EVENT, LOG-SECRET. **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; `cargo shear` and both standalone crates stay clean |
| `make test-unit-all` | yes | guffbolt, guffbolt-ai, `afr_tools`, `afr_agent`, app and CLI Dimensions |
| `make test-integration-rustd` | yes | every daemon Dimension, against a fake Parallel upstream |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update both route files |
| `make check-version` | yes | the runner protocol `VERSION` and the published crates move as the gate requires |
| `make acceptance` (live, Linux, on demand) | not in a green PR | `prime-curious-otter.spec.ts` runs on a burst machine before the PR opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the verb handler, the router and the vendor client each stay one module |

## Prior-Art / Reference Implementations

- **Reference:** the decision call (`afr_supervisor/src/verbs/decision/mod.rs`; `af_providers/src/decision/**`; `docs/architecture/durable_tasks.md` §Paying for a decision call), a fenced verb with a platform-admin vault key. §1 keeps both and makes the call in the daemon, so the key never leaves it.
- **Reference:** `afr_tools/src/egress/mod.rs:195-230`, the redirect chain; §3 sends `web_fetch` through it. `request_access` and its card (M256_001 §3); §4 adds a kind.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR).

## Sections (implementation slices)

Build order: §1; then §2 and §4; then §3; then §5 and §6; §8 last. §7 and §9 fit anywhere. §2's runner backend needs §1's verbs, and §5 needs §2 and §3.

### §1 — The daemon calls Parallel, and each call is counted on its run

`POST L/searches` and `POST L/extracts` are fenced run verbs. The daemon checks the fence, then that the fleet row reads `http_egress` `public`, then that the platform-admin vault holds the key `parallel`. It then calls Parallel with `x-api-key`, and commits the run's count before it answers.

A search sends `search_queries: [query]` and the query as `objective`, and no `mode`, so Parallel's documented default `advanced` applies. An extract sends `urls: [url]` and asks for the full content as Markdown. Before any call, an extract refuses a URL that is not `https` or whose host is a blocked literal (`af_egress/src/resolve/mod.rs:74`).

Parallel documents no maximum query length (API reference, read Nov 26, 2026). So `query` and `url` take `af_validate`'s text ceiling, and the runner frame bounds the request.

**State authority:** `thread.runs.search_calls` and `extract_calls`, each `CHECK (… >= 0)`. **Guarded transition:** every call that reached Parallel adds one, in a statement fenced by the run's token. A retried verb counts its second vendor call too.

**Cost:** no usage line is written; the runtime fee covers Parallel's cost (Discovery names the risk).

- **Dimension 1.1** — a search answers and is counted → Test `a_search_call_returns_parallels_results_and_counts_one` (integration, a fake Parallel upstream): a public fleet's `{query:"booking.com hotels lisbon"}` → 200 with the upstream's results; the run's `search_calls` is 1; the upstream saw the vault key in `x-api-key`; no usage line is written.
- **Dimension 1.2** — a replay counts the vendor's second call → Test `a_replayed_search_counts_each_vendor_call` (integration): the same call number twice → two upstream calls and `search_calls` 2.
- **Dimension 1.3** — a private fleet sends nothing → Test `a_private_fleets_search_is_refused_before_the_vendor` (integration): `http_egress` `restricted` → 403 with the new code; the upstream counts zero calls; `search_calls` stays 0.
- **Dimension 1.4** — a missing key is named → Test `a_web_call_without_the_vendor_key_is_refused` (integration): no `parallel` entry in the platform-admin vault → 503 with the new code, zero upstream calls, nothing counted.
- **Dimension 1.5** — a vendor failure is a sentence → Test `a_failed_vendor_call_is_named_and_counted` (integration): the upstream answers 429, then 500 → 502 with the new code naming the vendor's status; each reached call is counted.
- **Dimension 1.6** — the key stays in the daemon → Test `the_vendor_key_never_reaches_a_runner` (integration): a per-run sentinel key → absent from every run payload, verb answer and captured log line; present only in the upstream's `x-api-key`.
- **Dimension 1.7** — an extract reads a public page → Test `an_extract_call_returns_the_pages_markdown` (integration): `{url:"https://example.org/a"}` → the upstream's full content as Markdown; `extract_calls` is 1.
- **Dimension 1.8** — a blocked literal never leaves → Test `an_extract_of_a_blocked_literal_is_refused` (integration): `https://169.254.169.254/latest/meta-data` and `http://example.org` → refused, zero upstream calls.
- **Dimension 1.9** — a stale fence spends nothing → Test `a_web_call_with_a_stale_fence_spends_nothing` (integration): a superseded fencing token → the existing stale-fence code, zero upstream calls.

### §2 — `web_search` is our function tool on every wire; hosted search is deleted

guffbolt gains the `SearchBackend` trait, beside `Outbound` (`guffbolt/src/tools/mod.rs:36-38`), with `search` and `extract`. Its `web_search {query}` tool answers each result as a Markdown heading link, its date and its excerpts, within the tool output budget.

guffbolt-ai loses `Hosted`, `Request.hosted`, `WEB_SEARCH_MESSAGES` and `fn hosted`, so no wire sends a provider tool. Gemini grounding stays off.

The runner's `SearchBackend` is the verb client in `afr_tools`. `WEB_SEARCH` becomes a supervisor entry, and `Runtime::Provider`, `offer::hosted`, `HOSTED_ELSEWHERE` and `HostedToolUnavailable` go.

- **Dimension 2.1** — every wire sends a function and no provider tool → Test `every_wire_offers_web_search_as_a_function` (unit, guffbolt-ai): for Messages, Responses, Chat, Gemini and Ollama → `web_search` is in the function tools; no `web_search_20250305`, no `{"type":"web_search"}` and no `googleSearch` is sent.
- **Dimension 2.2** — results read as Markdown → Test `a_web_search_answers_its_results_as_markdown` (unit, guffbolt, a fake backend): two results → two `## [title](url)` blocks with dates and excerpts.
- **Dimension 2.3** — a refusal is a sentence → Test `a_refused_search_names_the_network_switch` (unit, `afr_tools`): the verb's 403 → `[network_restricted]` naming "Allow all" and `agentsfleet fleet network use <fleet> allow-all`; the run continues.
- **Dimension 2.4** — a Chat model searches → Test `a_chat_model_searches_through_the_verb` (integration, `afr_agent` with a fake Chat stream and a fake verb): the model calls `web_search` → one verb call, and the result is the next request's tool result.

### §3 — `web_fetch` reads a page by the fleet's policy

The runner's backend decides before any call. A host the fleet's policy names, through `allow` or the package registries (`af_egress` `listed()`), is read locally through `get_chain`, which re-admits every hop.

Any other host goes through `POST L/extracts` on a public fleet. On a private fleet it is refused, naming `request_access {host}` and "Allow all".

**Why the split.** Parallel fetches from its own servers, so a host that admits only the runner's address cannot be reached through it. The local path keeps today's refusal of private and metadata addresses (`af_egress/src/resolve/mod.rs:60`).

`MAX_REDIRECTS` becomes 20, the Fetch standard's limit: "If request's redirect count is 20, then return a network error" (fetch.spec.whatwg.org, HTTP-redirect fetch). `http_request` shares it.

With no `max_chars`, a page stops at the command output budget, 40,000 bytes (M253_004 decision 8). The local path keeps `html2text`'s plain config, which renders headings, lists, emphasis and reference links as Markdown. It refuses a body that is not text.

- **Dimension 3.1** — an allowlisted host is read locally → Test `an_allowlisted_host_is_read_locally_through_its_redirects` (integration, `afr_tools`, the `af_egress` test upstream): a private fleet with `allow:[the upstream]`; 301, 302, then 200 → the page's text; no extract call.
- **Dimension 3.2** — a redirect leaves no hole → Test `a_redirect_to_a_host_outside_policy_is_refused` (integration): hop two names an unlisted host → `host_not_allowed`, and that host receives no request.
- **Dimension 3.3** — the Fetch limit holds → Test `the_twenty_first_redirect_is_refused` (unit, `af_egress`): 20 redirects are followed; the 21st stops the chain with its hop-limit reason.
- **Dimension 3.4** — a public page goes through Extract → Test `a_public_fleets_page_is_read_through_extract` (unit, `afr_tools`, a fake verb): an unlisted host on a public fleet → one extract call and no local request.
- **Dimension 3.5** — a private fleet's page stays home → Test `a_private_fleets_page_outside_its_allowlist_is_refused` (unit): an unlisted host on a private fleet → `[network_restricted]` naming `request_access`; no extract and no local request.
- **Dimension 3.6** — a page is Markdown → Test `a_fetched_page_comes_back_as_markdown` (unit, guffbolt): an `<h2>`, a `<ul>` and a link → `## `, `* ` and `[text][1]` with its footnote.
- **Dimension 3.7** — the default budget holds → Test `a_fetch_without_max_chars_stops_at_the_output_budget` (unit, guffbolt): a 50,000-character page → 40,000 characters and the closing line naming the total.
- **Dimension 3.8** — a binary body is named → Test `a_binary_body_is_refused_with_its_type` (unit, guffbolt): `application/pdf` on the local path → `[unsupported_media] application/pdf`, with no body bytes.

### §4 — `request_access` asks for a host

`request_access` takes `{name, scope?}` as today or `{host}`. The daemon reads the host with `af_egress::allowlist_host` (`af_egress/src/allowlist.rs:28`) and refuses an address literal or `localhost`. It then raises today's card, and the run ends waiting as a secret request does.

The grant row has `grant_type` `host` (`afd_api_wire/src/grant.rs:45-49` gains it) and the host as its `service`.

On Approve, the policy build adds the host to `NetworkPolicy.allow` (`afd_gate/src/policy/shape/mod.rs:36-59`). `listed()` then admits it, the sandbox's allowlist names it (`afr_supervisor/src/egress/mod.rs:71-85`), and M258_002's browser check reads the same list.

A host grant opens that host only; search stays off on a private fleet. `host_not_allowed` names `request_access` (`af_egress/src/error/mod.rs:69-74`).

- **Dimension 4.1** — a host request raises one card → Test `a_host_request_raises_one_card_and_waits` (integration): `{host:"www.booking.com"}` → a `pending` grant of type `host`, one card carrying the turn, the turn settles waiting.
- **Dimension 4.2** — an approved host is reachable → Test `an_approved_host_joins_the_next_runs_allowlist` (integration): Approve → the resumed run's `NetworkPolicy.allow` holds `www.booking.com`.
- **Dimension 4.3** — an address is refused → Test `a_host_request_for_an_address_is_refused` (integration): `10.0.0.5`, `localhost` and `169.254.169.254` → the refusal, no row and no card.
- **Dimension 4.4** — one kind per request → Test `a_request_naming_both_a_name_and_a_host_is_refused` (unit, `afr_tools`): both, or neither → `invalid_arguments`, and nothing is posted.
- **Dimension 4.5** — a host grant opens no search → Test `a_host_grant_leaves_search_off` (integration): a private fleet with an approved host → `POST L/searches` still answers 403.
- **Dimension 4.6** — the refusal points at the fix → Test `the_host_not_allowed_text_names_request_access` (unit, `af_egress`): the refusal's sentence names `request_access` and the host.
- **Dimension 4.7** — a person sees the host → Test `an access card for a host names it` (unit, vitest, `ui/packages/app/components/domain/approvals/FleetApprovalSummary.test.tsx`): a host card reads "wants to reach www.booking.com"; Deny leaves "Can access" unchanged.
- **Dimension 4.8** — the CLI lists it → Test `fleet grants list shows a host grant` (unit, handler, `cli/test/`): a `host` grant renders its kind and host, and `--json` carries `kind: "host"`.

### §5 — Sources reach the thread

`ToolCallCompleted` (`af_wire/src/activity/mod.rs:82`) gains `sources`, each `{url, title}`, for a `web_search` or `web_fetch` call and absent for any other. The runner fills it from the backend's answer, bounded by the runner frame. The app's run summary lists a run's sources under "Sources", linking each `http` or `https` URL and showing any other as text.

- **Dimension 5.1** — a search carries its sources → Test `a_completed_search_carries_its_sources` (unit, `afr_agent`): two results → the completed call's `sources` holds both URLs and titles, in order.
- **Dimension 5.2** — a failure carries none → Test `a_refused_search_carries_no_sources` (unit): a `[network_restricted]` answer → `sources` is absent.
- **Dimension 5.3** — the person sees them → Test `the run summary lists a run's sources` (unit, vitest, `ui/packages/app/components/domain/conversation/summary/RunResources.test.tsx`): two sources → two links with `rel="noopener noreferrer"`.
- **Dimension 5.4** — a hostile URL is not a link → Test `a javascript source is shown as text` (unit, vitest): `javascript:alert(1)` → rendered as text with no `href`.

### §6 — The prompt names what the run holds, the fleet's network and today's date

The fixed text (`afr_agent/src/prompt/mod.rs:43-56`) is built from the run's offered tools, so it names `task_start` only when the run holds it.

Two lines follow it, outside the saved compaction's digest. One states the fleet's network and that a private fleet's search is off. The other is the date block, "Today is Thursday, November 26, 2026 (UTC).", day-grained so the prompt's cached prefix changes at most once a day.

- **Dimension 6.1** — no promise of a missing tool → Test `a_run_without_task_tools_is_not_told_about_task_start` (unit, `afr_agent`): a payload with no decision profile → the prompt names no `task_start`; with one, it does.
- **Dimension 6.2** — the date is there and stays out of the digest → Test `a_runs_prompt_carries_todays_date_outside_the_digest` (unit): a fixed clock → the instructions hold the date line and `Prompt.fixed` does not.
- **Dimension 6.3** — a private fleet is told → Test `a_private_fleets_prompt_says_search_is_off` (unit): `restricted` → the network line names "Allow all"; `public` → it says search is on.

### §7 — guffbolt's approved tool fixes

Indy approved these on Nov 22, 2026 (`docs/v2/done/M253_004_P1_API_DOCS_INFRA_GUFFBOLT_AI_PROVIDERS_FOR_GUFFBOLT.md:508`). `tool_search` answers 8 matches when no `limit` is named, in place of `usize::MAX` (`guffbolt/src/tools/search.rs:138`). `grep` gains `context`, the lines around each match, 0 when absent, and cuts each line at 500 characters with a note (`docs/v2/done/M253_004_TOOLS_BRAINSTORM.md:20,236`).

- **Dimension 7.1** — the default limit is 8 → Test `tool_search_answers_eight_when_no_limit_is_named` (unit, guffbolt): 12 deferred matches and no `limit` → 8.
- **Dimension 7.2** — a long line is cut → Test `grep_cuts_a_long_line_at_500_characters` (unit, guffbolt): a 2,000-character matching line → 500 characters and the note.
- **Dimension 7.3** — context is shown → Test `grep_shows_context_lines_around_a_match` (unit, guffbolt): `context: 1` → the line before and after each match; absent → the match alone.

### §8 — The real path, end to end

A public fixture fleet searches, reads the top page and shows its sources, on the live stack.


- **Dimension 8.1** — the real path → Test `prime-curious-otter.spec.ts` (e2e, `make acceptance`, fixture `curious-otter`): a person turns on "Allow all" and asks the fleet to search and read the top page → the answer turn holds a `web_search` and a `web_fetch` call, "Sources" lists the URLs, and the run has one `search` and one `extract` line.

### §9 — A person's message carries images

`POST F/messages` and `POST W/fleets` take `images`, and the turn keeps them beside `message`, so Postgres holds them. The web app's composer, `agentsfleet fleet create --image <path>` and `fleet steer --image <path>` send them. Today a run's input is the text of `message` alone (`af_wire/src/turn/message.rs:14-24`).

The runner puts the images in the run's first user message on Messages, Responses, Gemini and Ollama (`guffbolt-ai/src/wire/route/mod.rs:36-48`). A fleet whose model is on the Chat wire refuses the message, since Chat takes text alone.

**Bounds, each forced by a provider or a transport.** Each image is PNG, JPEG, GIF or WebP, the four kinds the wires take (`guffbolt/src/tools/files/image.rs`). Each is at most 5 MiB decoded, the Messages wire's per-image cap (`guffbolt/src/tools/limits.rs:43-44`).

The runner scales a side past 2048 pixels (`:41-42`). The route's body limit is the runner frame, 32 MiB (`af_wire/src/protocol/mod.rs:29`), because every image rides the run's frame. No image count is invented.

- **Dimension 9.1** — an image reaches the model → Test `a_message_with_an_image_reaches_the_model` (integration, a fake Messages provider): `POST F/messages {message, images:[a PNG]}` → the turn keeps it, and the run's first request carries it as an image block in the user message.
- **Dimension 9.2** — a model that cannot see is refused → Test `an_image_for_a_model_that_takes_none_is_refused` (integration): a fleet on a Chat-wire model → 409 with the new code naming the model, and no turn.
- **Dimension 9.3** — the bounds hold → Test `a_message_image_past_the_wire_bounds_is_refused` (integration): a 6 MiB PNG, a TIFF, and bytes that do not decode as their declared kind → 400 naming the bound, and no turn.
- **Dimension 9.4** — the web app sends it → Test `the composer sends an attached image` (unit, vitest, the composer's test file): attach a PNG and send → one `POST F/messages` whose `images` holds it; a 6 MiB file is refused before sending.
- **Dimension 9.5** — the CLI sends it → Test `fleet steer --image sends the file` (unit, handler, `cli/test/`): `--image shot.png` → the request's `images` carries it base64; a missing file → exit 4 and no request.

## Interfaces

**Tools.**

```
web_search {query}                  results as "## [title](url)", date, excerpts; refusal [network_restricted] <sentence>
web_fetch  {url, max_chars?}        the page as Markdown; max_chars defaults to the command output budget
request_access {name, scope?} | {host}
```

**Run verbs.** Each carries `fencing_token` and `call_number`, as the trigger verbs do.

```
POST L/searches  {fencing_token, call_number, query}  200 {results: [{url, title, published?, excerpts[]}]}
POST L/extracts  {fencing_token, call_number, url}    200 {url, title?, content}   content is Markdown
POST L/access    {fencing_token, name, scope?} | {fencing_token, host}     201 {grant_id, gate_id}; 200 as today
```

**Codes.** New codes are numbered at PLAN from the next free in the family, each with its `REGISTRY` row and problem entry. The stale-fence code is reused.

```
403  a private fleet's web call          503  the vendor key is missing
502  the vendor failed                   400  a host request names an address
409  an image the fleet's model cannot see
400  an image past the wire's bounds
```

**Runner wire.** `ToolCallCompleted` gains `sources`, and the run payload's event carries the turn's `images`. `AccessRequest` takes `name` or `host`, and guffbolt-ai's `Request` loses `hosted`. `VERSION` moves once.

**Messages.** `POST F/messages` and `POST W/fleets` take `images?: [{media_type, data}]`, `data` base64. The media types are PNG, JPEG, GIF and WebP; each image is at most 5 MiB decoded.

**Schema** (edited in place, pre-production). `thread.runs` gains `search_calls INTEGER NOT NULL` and `extract_calls INTEGER NOT NULL`, each `CHECK (… >= 0)`, written 0 at the run's insert. `connector.integration_grants` takes `grant_type` `host`, an application constant.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| A private fleet searches | security | the model calls `web_search` on a `restricted` fleet | the runner refuses first, the daemon refuses again; zero vendor calls | `a_private_fleets_search_is_refused_before_the_vendor` |
| Server-side request forgery (SSRF) by Extract | security | a URL names a metadata or private literal | refused before any call | `an_extract_of_a_blocked_literal_is_refused` |
| Redirect out of policy | security | an allowlisted host redirects to an unlisted one | `host_not_allowed`; the second host gets nothing | `a_redirect_to_a_host_outside_policy_is_refused` |
| Key leak | security | the key rides a payload or a log | the key never leaves the daemon | `the_vendor_key_never_reaches_a_runner` |
| Replay | failure | the runner retries a verb | the second vendor call is counted, so the count stays our true cost | `a_replayed_search_counts_each_vendor_call` |
| Vendor down or rate-limited | failure | Parallel answers 5xx or 429 | 502 naming the status; the model may retry | `a_failed_vendor_call_is_named_and_counted` |
| Key missing | failure | no `parallel` entry in the platform-admin vault | 503 with its code; no call | `a_web_call_without_the_vendor_key_is_refused` |
| Image a model cannot see | failure | a person attaches an image for a Chat model | 409 naming the model; no turn | `an_image_for_a_model_that_takes_none_is_refused` |
| Oversized image | capacity | an image past 5 MiB or of another kind | 400 naming the bound; no turn | `a_message_image_past_the_wire_bounds_is_refused` |
| Stale run | failure | a superseded fence posts | the stale-fence code; no call | `a_web_call_with_a_stale_fence_spends_nothing` |
| Hostile result | security | a result's URL is `javascript:` | shown as text, never linked | `a javascript source is shown as text` |
| Unreadable body | failure | a PDF on the local path | `[unsupported_media]` naming its type | `a_binary_body_is_refused_with_its_type` |

## Invariants

1. The Parallel key never leaves the daemon — only §1's handler loads it, and no wire type has a field for it (Dimension 1.6).
2. A private fleet sends no query and no URL to the vendor — the daemon reads `http_egress` from the fleet row before the call (Dimensions 1.3 and 3.5).
3. Every vendor call is counted on its run — the count is added by the same handler that made the call, before it answers (Dimension 1.2).
4. Every local hop is admitted by policy — `get_chain` re-prepares each target through the run's guard (Dimension 3.2).
5. No wire sends a provider-hosted tool — `Hosted` is deleted, so a provider tool cannot be named (Dimension 2.1).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `web_call` (log) | operator | a search or extract verb answers | `run_id`, `fleet_id`, `operation`, `outcome`, `ms`, `vendor_status` | no query, no URL, no key | `a_search_call_returns_parallels_results_and_counts_one` |
| `agentsfleet_web_calls_total{operation,outcome}` | operator | the same | operation, outcome | labels only | `a_failed_vendor_call_is_named_and_counted` |
| `fleet_access_changed` | product | a host grant is requested, approved, denied or revoked | `fleet_id`, `kind` (`host`), `change` | no host name | `a_host_request_raises_one_card_and_waits` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A search and an extract answer through the daemon, and each vendor call is counted on its run (§1) | `make test-integration-rustd` | exit 0; `a_search_call_returns_parallels_results_and_counts_one ... ok`, `a_replayed_search_counts_each_vendor_call ... ok`, `an_extract_call_returns_the_pages_markdown ... ok` | P0 | |
| R2 | A private fleet sends nothing, and the key never leaves the daemon (§1) | `make test-integration-rustd` | exit 0; `a_private_fleets_search_is_refused_before_the_vendor ... ok`, `the_vendor_key_never_reaches_a_runner ... ok`, `an_extract_of_a_blocked_literal_is_refused ... ok` | P0 | |
| R3 | Every wire offers our `web_search` and no provider tool (§2) | `make test-unit-all` | exit 0; `every_wire_offers_web_search_as_a_function ... ok` | P0 | |
| R4 | `web_fetch` routes by policy, follows up to 20 redirects and returns Markdown (§3) | `make test-unit-all` | exit 0; `a_private_fleets_page_outside_its_allowlist_is_refused ... ok`, `the_twenty_first_redirect_is_refused ... ok`, `a_fetched_page_comes_back_as_markdown ... ok` | P0 | |
| R5 | A fleet asks for a host and a person approves it once (§4) | `make test-integration-rustd` | exit 0; `a_host_request_raises_one_card_and_waits ... ok`, `an_approved_host_joins_the_next_runs_allowlist ... ok`, `a_host_request_for_an_address_is_refused ... ok` | P0 | |
| R6 | Sources show, and the prompt names only what the run holds (§5, §6) | `make test-unit-all` | exit 0; `a_completed_search_carries_its_sources ... ok`, `a_run_without_task_tools_is_not_told_about_task_start ... ok`; `the run summary lists a run's sources` passed | P1 | |
| R7 | guffbolt's approved fixes hold (§7) | `make test-unit-all` | exit 0; `tool_search_answers_eight_when_no_limit_is_named ... ok`, `grep_cuts_a_long_line_at_500_characters ... ok` | P2 | |
| R8 | No hosted-search code is left (§2) | `git grep -n -w -e WEB_SEARCH_MESSAGES -e HOSTED_ELSEWHERE -e HostedToolUnavailable -e web_search_20250305 -- rustd` | 0 matches | P0 | |
| R9 | A person's image reaches a model that takes images, and is refused for one that does not (§9) | `make test-integration-rustd` | exit 0; `a_message_with_an_image_reaches_the_model ... ok`, `an_image_for_a_model_that_takes_none_is_refused ... ok` | P1 | |
| R10 | The live path works (§8) | `make acceptance SUITE=all` | exit 0; `prime-curious-otter.spec.ts` passed | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behaviour evals

- **Grounding rule:** an answer names only URLs that the run's `web_search` or `web_fetch` returned.
- **Golden set:** the follow-ups in `tests/fixtures/fleets/curious-otter.json`, including the worst known case: booking.com asked of a private fleet. Each failure found in use becomes a follow-up.
- **Ship threshold:** grounding holds in every case, checked by R10, whose spec compares the answer's URLs with the turn's sources.
- **Fallback:** a fleet that cannot search says so and names "Allow all". A fabricated URL fails R10.

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the hosted wire specs | `git grep -n -w -e WEB_SEARCH_MESSAGES -e web_search_20250305 -e provider_tools -- rustd` | no output |
| the hosted type and its plumbing | `git grep -n -w -e Hosted -e HOSTED_ELSEWHERE -e HostedToolUnavailable -e hosted_tool_unavailable -- rustd/guffbolt rustd/guffbolt-ai rustd/crates/runner rustd/crates/e2e` | no output |
| the provider runtime | `git grep -n -e "Runtime::Provider" -- rustd` | no output |

**Every hit at `576d0b462`, for SPEC-BLAST.** Production: `guffbolt-ai/src/{lib.rs:47,types/mod.rs:10,types/model.rs:21-26,48,wire/request/mod.rs:12,29,79,84-91}`, `guffbolt/src/lib.rs:111`, `afr_agent/src/{loop/mod.rs:22,214,offer/mod.rs:11,55-66,router/mod.rs:23,119,172}`, `afr_tools/src/{catalog/mod.rs:64,315,runtime/mod.rs:66,152,stub/mod.rs:49}`, `guffbolt/examples/fireworks_live.rs:27,197`. Tests: `guffbolt-ai/src/wire/request/tests.rs:8,79,243-254`, `guffbolt-ai/src/wire/route/tests/counting/unsupported.rs:13`, `afr_agent/src/{offer/tests.rs:13,48,142-143,router/tests.rs:222-228}`, `afr_tools/src/{catalog/tests.rs:247,338,runtime/tests.rs:10}`, `guffbolt/tests/public_api/{plain/answers.rs:135,seam.rs:87}`, `af_e2e/tests/support/definition_run.rs:237`. Same spelling, other meaning, kept: `afd_http/src/openapi/mod.rs:160` ("Hosted schedules").

## Out of Scope

- Code mode, skills, structured output, and Model Context Protocol (MCP) servers for fleets: not planned in M258.
- Files other than images in a message, and audio or video.
- The browser: M258_002. The Firecracker engine: its own later milestone.
- A per-fleet search setting beyond "Allow all".

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person turns on "Allow all", asks a fleet on any model to compare hotels on booking.com, and reads an answer with its sources listed.
2. **Preserved user behaviour** — `http_request`, private fleets' approved hosts, the access card and every model's chat keep working; `web_fetch` of an allowlisted host still runs locally.
3. **Optimal-way check** — one tool pair, one vendor behind one trait, one count per run; the gap was a provider-hosted search on 8 of 20 models.
4. **Rebuild-vs-iterate** — rebuild how search reaches a model; iterate on `web_fetch`, `request_access` and the prompt.
5. **What we build** — two daemon verbs over Parallel with a per-run count; the `SearchBackend` trait, `web_search` and a routed `web_fetch`. Also a host grant, sources on a completed call, a prompt from the offered tools, and images in a message.
6. **What we do NOT build** — a per-fleet vendor choice (one platform vendor); a vendor fallback chain (one vendor first); a search on private fleets (ruled out); provider-hosted search as an option (ruled out, no alias).
7. **Fit with existing features** — it compounds with "Allow all", the access card and the `image` tool's path. It must not destabilize the network policy's host rule, which `http_request` and M258_002's browser check share.
8. **Surface order** — API and runner first, then the app's card and Sources, then the CLI's lists, because both read the daemon's new shapes.
9. **Dashboard restraint** — "Sources" shows only when a run called a web tool; no search settings page exists.
10. **Confused-user next step** — a refused search names "Allow all" and `agentsfleet fleet network use <fleet> allow-all`; a refused page names `request_access`, and the card asks the person.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the daemon's verbs first, so the runner's backend has a target; then the tools, the host grant, the reader's surfaces and the prompt.
- **Alternatives considered:**
  - Hybrid search, hosted where a provider offers it: ruled first, then replaced by Indy.
  - A runner that calls Parallel with a per-call key, as the decision call does: declined by the author: a tenant runner never holds the key.
  - Google Search grounding on Gemini: declined, with no renderer for Google's Search Suggestions.
  - Extract for every page: declined, because an allowlisted host may admit only the runner.
- **Patch-vs-refactor verdict:** a **refactor** because the hosted-tool path is deleted from two published crates and the runner, and search moves from the provider to the daemon.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "I want to know how to improve ourselves for the Browser? is there any way else than the browser, the tool must be more like we do in sprites or our own.. just like our memory tool? Also for search Parallel is one, you should be able to do with others like opencode does? we could borrow from there So record the above two." — context: search runs on several vendors like opencode, replacing "Parallel first" and the out-of-scope second vendor; the first half is M258_002's.

- > Owner (Indy, Nov 26, 2026): "Essentially add missing features for guffbolt agent harness", "max 2 in 1 milestone for the guffbolt tool updates" and "I need the final scope to fix" — context: M258 holds two Workstreams, M258_002's browser and this spec.
- > Owner (Indy, Nov 26, 2026): "Hybrid", then "Does parallels offer search too", then "I think i wanna take the call to use like amp for websearch and avoid hybrid?" — context: the last replaces "Hybrid". Our function-tool `web_search` runs on every wire, hosted search is deleted, and Parallel is the first vendor behind a `SearchBackend` trait.
- **Owner decisions, Nov 26, 2026, on the gap matrix's recommendations** — accepted: Gemini grounding off, Gemini using our `web_search`; a `restricted` fleet cannot search; page reading through Parallel Extract on public fleets.
- > Owner (Indy, Nov 26, 2026), recorded in the sibling specs: "Allow it now", "look for the updated lightpanda", "i am thinking to move to the firecracker spec as well", "Why do you need screenshot", "when will you have this usecase? i am leaning towards a and b" (M258_002); "I agree on those 3", "4 is very less availability", "Finish lines decide done", "make haiku as the default model", "update and check all the crates and push in this PR", "everything you do must go in 1 PR" (M257_001).
- **Agent default, open to Indy** — the daemon makes every Parallel call, for platform and tenant runners alike. The matrix proposed a runner call with a platform key, plus a daemon proxy for tenant runners (`docs/v2/pending/M251_001_…:202-214`). One daemon path keeps the key off every runner and makes the count the daemon's own.
- **Agent default, open to Indy** — no `mode` is sent, so Parallel's default `advanced` applies at USD 5 per 1,000; `turbo` or `fast` cost USD 1 per 1,000 (pricing page, read Nov 26, 2026).
- > Owner (Indy, Nov 26, 2026): "i think though this is billable, it must be convered to the platform runtime cost?" — context: Parallel runs on one platform key and writes no usage line; the runtime fee covers it, and calls are counted per run for our own cost tracking.
- **Named risk, the fee against the search.** The runtime fee is USD 0.0001 a second (`afd_billing/src/nanos.rs:25`). One `advanced` search at USD 0.005 equals 50 seconds of fee, and one extract at USD 0.001 equals 10 seconds. A run that searches more often than once per 50 seconds costs the platform more than it earns; `search_calls` against the run's seconds shows where.
- > Owner (Indy, Nov 26, 2026): "In web and the cli, since if its api then allow both" — context: a person attaches images to a message through the API, and both the web app and the CLI send them (§9).
- **Consults** — the harness gap matrix (Nov 26, 2026, `576d0b462`), §0, §1.1, §4 and §6. References cited here only: Amp uses Parallel for `web_search` and `read_web_page`; opencode's `webfetch` follows 10 redirects, re-checking each; Codex sends hosted search on Responses. The Gemini review (Nov 26, 2026) sets grounding aside for its display terms.
- **Parallel, read Nov 26, 2026** — `POST /v1/search` takes `search_queries` (required), `objective` and `mode` (`turbo`, `fast`, `basic`, `advanced`; default `advanced`), with `x-api-key`; it documents no query length maximum. `POST /v1/extract` takes `urls` and returns Markdown, "including JavaScript-heavy pages and PDFs".
- **Parallel's prices, read Nov 26, 2026 (`docs.parallel.ai/resources/pricing`)** — per 1,000 requests, `turbo` or `fast` cost USD 1 and `basic` or `advanced` USD 5. Results past 10 cost USD 1 per 1,000, and Extract USD 1 per 1,000 URLs.
- **Corrections to the matrix** — the grant type constants live in `afd_api_wire/src/grant.rs:45-49`; `afd_gate/src/policy/build/mod.rs:76,110` uses them. The 500-character `grep` cap is in the M253_004 brainstorm (`:20,236`), while `grep.rs:13` is the default of 100 matches. `web_fetch` already renders Markdown through `html2text`'s plain config, so §3 changes the description and adds a test, not a converter.
- **Unverified (`[?]`)** — the 1Password item for `PARALLEL_API_KEY` (`agentsfleet-parallel-api-token` follows the Fireworks item's pattern); whether a reloaded thread reads `sources` from the kept activity or from the turn's tool calls, which §5's build settles.
- **Model reviews** — none yet.
- **Open blockers** — the two agent defaults and the known limit above wait on Indy before the spec leaves `pending/`.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
