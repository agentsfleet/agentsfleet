<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M254_001: The browser tools open, act on and photograph real pages on Chromium in a Fly.io Sprite, and every request passes the fleet's network policy

**Prototype:** v2.0.0

**Milestone:** M254

**Workstream:** 001

**Date:** Nov 22, 2026; amended Nov 26, 2026

**Status:** PENDING

**Priority:** P2, every real environment refuses the browser tools, so a fleet cannot click through a page that needs JavaScript

**Categories:** INFRA

**Batch:** B2, after M254_002 merges

**Depends on:** M254_002, whose `request_access` host grant puts a host on a private fleet's allowlist, which the browser's check reads

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 22, 2026); amended (Claude Opus 5.5, Nov 26, 2026) from the M254 harness gap matrix §5 and §6 and Indy's rulings of Nov 26, 2026, each reference re-read at `576d0b462`

**Canonical architecture:** `docs/architecture/runner_execution.md` §Tool catalog and §Limits; this spec adds the browser there

---

## Overview

**Goal (testable):** `browser_open` answers a page's text and `screenshot` a PNG from Chromium in a Fly.io Sprite, and the check on every request refuses each host the fleet's policy does not admit and each private or metadata address.

**Terms.** Rust paths that start with a crate name are under `rustd/crates/<layer>/`; `guffbolt/` is `rustd/guffbolt/`. Line references are at `576d0b462`.

The supervisor drives Chromium over the Chrome DevTools Protocol (CDP). A **Sprite** is a Fly.io sandbox: a Firecracker microVM with its own kernel, with checkpoint and restore.

**Problem:** Read from source.

- **The tools always refuse.** `SandboxEnv` and `LazySandboxEnv` answer "waits for the Firecracker engine: Chromium cannot start inside this sandbox" (`afr_tools/src/generic/env/mod.rs:47-49,303-304`; `generic/env/lazy.rs:94-95`). Every run is still offered all three tools (`afd_gate/src/policy/catalog.rs:46-48`).
- **Chromium cannot start in bubblewrap.** Its sandbox needs a user namespace or a setuid helper. bubblewrap runs with `--disable-userns` (`afr_sandbox/src/bubblewrap/mod.rs:71`), and seccomp refuses `unshare` (`afr_sandbox/src/harden/linux.rs:32`); spike S1 recorded the failures (`docs/v2/reviews/m211-toolbox-spikes.md:27-42`).
- **Nothing on the runner can carry a browser's traffic.** `af_egress` builds no proxy (`af_egress/src/network/mod.rs:248-252`), each slot's input chain is `iif afvN drop` (`afr_sandbox/src/egress/rules/mod.rs:7-13`), and the sandbox network ignores the fleet's toggle (`afr_supervisor/src/egress/mod.rs:94-115`).
- **The arguments are untyped.** All three handlers take a bare `Value` (`guffbolt/src/tools/browser.rs:59,83,107`).

**Solution summary:**

- **A `BrowserBackend` trait.** The runner drives a browser through one trait, so a second backend adds an implementation and changes no tool.
- **The first backend is Chromium in a Sprite.** The Sprite gives Chromium a kernel of its own, so Chromium's own sandbox starts, and nothing runs in the runner's bubblewrap sandbox.
- **CDP interception is the network check.** Every request pauses at the request stage, the supervisor applies the fleet's host rule and the private-address rule, and only then does the request continue.
- **Screenshots come with Chromium,** shown to the model through the existing `image` path.
- **Typed actions,** and the three refusal texts retire.
- **Later backend.** Our own Firecracker engine on the runner host is a second backend in a later milestone, outside M254.

**Design flow:**

```
browser_open {url} ─► supervisor BrowserBackend ─► daemon: create the run's Sprite (platform token)
                                                     ─► the supervisor gets that Sprite's CDP endpoint
supervisor ◄──── CDP over TLS ────► Chromium in the Sprite
                                      │ Fetch.requestPaused, every request, before it leaves
                                      ▼
            supervisor check: 1. host rule, NetworkPolicy (public · allow · package registries)
                              2. address rule, resolved by the supervisor: private, loopback, metadata refused
                 pass ─► Fetch.continueRequest        fail ─► Fetch.failRequest (AccessDenied)
screenshot ─► Page.captureScreenshot ─► image path (2048 px longest side, 5 MiB) ─► the model's next step
run ends, held sandbox included ─► daemon deletes the run's Sprite
```

## Rulings

**Ruled by Indy on Nov 26, 2026** (quotes in Discovery).

| # | Ruling |
|---|---|
| Backend | Real Chromium behind a `BrowserBackend` trait. The first backend is Chromium in a Fly.io Sprite. |
| Later | Our own Firecracker engine on the runner host is a second backend in a later milestone. Sprite cost or latency, or a need for a VM per run, triggers it. |
| Lightpanda | Dropped, so no AGPL-3.0 binary enters the toolbox. |
| Network | Every request passes a check that enforces the fleet's `NetworkPolicy` and refuses private and metadata addresses. |
| Pixels | Screenshots come with Chromium. |

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(browser): the browser tools work on Chromium in a Sprite behind the fleet's network policy
- **Intent (one sentence):** A fleet can open a page that needs JavaScript, click, type and photograph it, and reach only what its network policy allows.
- **Authoring handshake:** Indy wants the browser on a real Chromium, starting with Fly.io Sprites, with no hole in the network policy. ASSUMPTIONS I'M MAKING: 1. M254_002 merges first. 2. §1's spike passes before §2 starts, and its record amends this spec where it disagrees. 3. The platform token stays in the daemon, as M254_002's search key does. 4. Nothing is backward compatible.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/harness/af_egress/src/policy/mod.rs` — `listed()` (`:198-213`), the host rule the check applies, so one rule decides for `http_request`, `web_fetch` and the browser.
2. `rustd/crates/harness/af_egress/src/resolve/mod.rs` — `unblocked` (`:60`) and `blocked_literal` (`:74`), the address rule.
3. `rustd/guffbolt/src/tools/env/mod.rs` — the `Browser` trait (`:260-269`) that `BrowserBackend` serves.
4. `rustd/guffbolt/src/tools/files/image.rs` and `rustd/guffbolt/src/tools/limits.rs` — the image path a screenshot takes, and its bounds.
5. `docs/v2/reviews/m211-toolbox-spikes.md` — spike S1, why Chromium cannot start in bubblewrap, and the record format §1 mirrors.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M254_001_P2_INFRA_BROWSER_TOOLS_THAT_WORK_ON_LIGHTPANDA.md` | EDIT | this spec; moves to `active/`, then `done/` at close; the file name predates the Sprite ruling |
| `rustd/crates/harness/af_egress/src/{policy,resolve}/**` | EDIT | the host rule and the address rule exposed to the check, one rule each |
| `rustd/crates/runner/afr_supervisor/src/browser/**` | CREATE | `BrowserBackend`, the Sprite backend's CDP session and the check; the module name is set at build |
| `rustd/crates/daemon/afd_fleet/src/run/browser/**`, `afd_api_runner/src/handler/runner/browser.rs`, `afd_http/src/route/*.rs`, `route_inventory.rs`, `route_meta_total.rs` | CREATE/EDIT | the fenced verbs that create and delete a run's Sprite with the platform token; `INVENTORY` and the counts |
| `rustd/crates/protocol/af_wire/src/browser/**` | CREATE | the verbs' types; the runner protocol `VERSION` moves once |
| `rustd/crates/foundation/af_core/src/{error_code,problem}/**` | EDIT | the codes in Interfaces, each with its `REGISTRY` row and problem entry |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | the maintained CDP client §1 picks |
| `rustd/crates/runner/afr_tools/src/generic/env/{mod,lazy,tests}.rs`, `afr_tools/src/sandbox/browser/**`, `afr_tools/src/runtime/mod.rs`, `afr_tools/src/catalog/mod.rs` | EDIT | `Browser` over the backend; the three refusal texts retire |
| `rustd/guffbolt/src/tools/{browser.rs,env/mod.rs}`, `rustd/guffbolt/README.md`, `rustd/guffbolt/tests/public_api/network/fake_env/browser.rs` | EDIT | typed arguments |
| `deploy/local/bootstrap.ts`, `deploy/local/bootstrap.test.ts` | EDIT | the local platform-admin vault gets the Sprite token, and initial setup names it when absent |
| `docs/architecture/runner_execution.md`, `docs/architecture/runner_protocol.md` | EDIT | the browser, its backend, its check, its limits and its verbs |
| `docs/v2/reviews/m211-toolbox-spikes.md` | EDIT | a dated note under its Disposition pointing at this spec; its record stays |
| `~/Projects/docs` tool pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | the published browser tools |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE. **`domains/http.md`, `domains/http-responses.md`** — HTTP-GARDE, HTTP-NEWCODE, HTTP-REGISTRY, HTTP-OPENAPI for the two verbs. **`domains/auth.md`** — AUTH-NOLOG: the token stays in the daemon and the browser holds no credential.
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-SECRET. **`tests.md`** — TEST-NEG, TEST-REDGREEN, TCF.
- **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND. **`code.md`** — ORP, GRD, FLL; the CDP client comes from a maintained crate, per `.orly/LOCAL.md`.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; `cargo shear` stays clean |
| `make test-unit-all` | yes | the check, lifecycle and argument Dimensions, against a fake CDP endpoint |
| `make test-integration-rustd` | yes | the Sprite verbs, against a fake Sprite API |
| OpenAPI drift and `INVENTORY` counts | yes | regenerate with `--no-banner`; update both route files |
| `af_e2e/tests/layers.rs` | yes | the backend lives in `afr_supervisor` and depends on `harness/` crates only |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the check, the CDP session and the lifecycle each stay one module |

## Prior-Art / Reference Implementations

- **Reference:** `afr_tools/src/egress/mod.rs:195-230`, a chain that re-admits every redirect target through the run's guard; the check admits each paused request the same way.
- **Reference:** M254_002 §1, a fenced verb whose platform key never leaves the daemon; the Sprite verbs keep the token the same way.
- **Divergence:** none from the HTTP standard.

## Sections (implementation slices)

Build order: §1, then §2 and §3, then §4 and §5. §3 needs §2's check before any page loads.

### §1 — Spike: Chromium in a Sprite, checked over CDP

A throwaway program outside the repository answers each question. Discovery records the answers before §2 starts, and a disagreement amends this spec first.

1. Chromium starts headless in a Sprite and answers `Browser.getVersion` over CDP from a runner host.
2. It loads booking.com and one Cloudflare managed-challenge page. Record the status and the text, beside `web_fetch` on the same pages.
3. CDP interception refuses a private address and a host outside a private fleet's allowlist, and the page sees a network error.
4. **Coverage.** Record which request kinds pause at the request stage: documents, subresources, `fetch`, redirects, WebSockets, and service workers' requests. A kind that does not pause is blocked another way or named here as a limit.
5. **Rebinding.** A name answering a public address to the supervisor and a private one to Chromium. Record what the Sprite's own network reaches.
6. **Credential.** Whether the Sprite API issues a credential scoped to one Sprite, so the supervisor never holds the platform token.
7. **Cost and time.** The time to a first page, cold and from a checkpoint, and the price per run. Sprite pricing and limits are unverified until this item reads them.

- **Dimension 1.1** — the spike is recorded → Test `browser_spike_recorded` (manual): Indy reads the record. Expected: Discovery answers items 1 to 7, and this line reads `DONE`. Evidence: this spec, in the Pull Request.

### §2 — Every request passes the fleet's network policy

The Sprite backend enables CDP's `Fetch` domain for every request at the request stage, on the page and every target it attaches. Each paused request passes two rules in order.

**Host rule:** the run's `NetworkPolicy` admits the host, by `af_egress`'s `listed()`. **Address rule:** the supervisor resolves the host, and every address must pass `af_egress::resolve::unblocked`. A private, loopback or metadata address fails, as a name or a literal.

**State authority:** the run's issued `NetworkPolicy`. **Guarded transition:** a paused request continues only on the supervisor's `continueRequest`. **Failure assumption:** a lost CDP session leaves every request paused, so the page loads nothing.

The check adds no header, so the browser holds no credential.

- **Dimension 2.1** — a private address is refused → Test `the_browser_refuses_a_private_address` (unit, `afr_supervisor`, a fake CDP session and a stub resolver): a public fleet's request to a name resolving to `10.0.0.5` → `failRequest`, reason `blocked_address`.
- **Dimension 2.2** — the metadata address is refused → Test `the_browser_refuses_the_metadata_address` (unit): `169.254.169.254` as a literal, and a name resolving to it → both failed.
- **Dimension 2.3** — a private fleet reaches only its allowlist → Test `the_browser_refuses_a_host_a_private_fleet_lacks` (unit): `restricted` with an empty `allow` → a request to `www.booking.com` is failed `host_not_allowed`.
- **Dimension 2.4** — a granted host passes untouched → Test `the_browser_admits_a_granted_host` (unit): `restricted` with `allow: [example.org]` → `continueRequest` with no header added.
- **Dimension 2.5** — a lost session loads nothing → Test `a_lost_cdp_session_leaves_requests_unanswered` (unit): the session drops mid-load → no `continueRequest` is sent, and the call answers `[browser_unavailable] the page closed`.
- **Dimension 2.6** — rebinding stays out → Test `[?]`, named once §1 item 5 records the mechanism: a name answering differently to the supervisor and to Chromium → no connection to the private address.

### §3 — The run's Sprite: created on first use, deleted at run end

On a run's first browser call the supervisor posts a fenced verb. The daemon creates the run's Sprite with the platform token from the platform-admin vault. The supervisor receives only that Sprite's CDP endpoint and its scoped credential (§1 item 6).

The browser holds the open page across calls. The daemon deletes the Sprite when the run ends, held sandbox included, so no browser state crosses runs.

**State authority:** the run's row holds the Sprite's identifier. **Failure assumption:** a daemon that dies before the delete leaves a Sprite the next boot's sweep deletes, keyed by runs that have ended.

- **Dimension 3.1** — a page opens → Test `a_page_opens_and_answers_its_text` (unit, a fake CDP endpoint): `browser_open` → the page's text.
- **Dimension 3.2** — the Sprite goes with the run → Test `a_runs_sprite_is_deleted_when_the_run_ends` (integration, a fake Sprite API): a run that opened a page ends → one delete for its Sprite; a held sandbox changes nothing.
- **Dimension 3.3** — a crashed daemon leaves nothing → Test `a_sprite_left_by_an_ended_run_is_swept_at_boot` (integration): a Sprite identifier on an ended run → deleted at boot.
- **Dimension 3.4** — the token never leaves the daemon → Test `the_sprite_token_never_reaches_a_runner` (integration): a sentinel token → absent from every verb answer, run payload and captured log line.
- **Dimension 3.5** — a refused create is a sentence → Test `a_sprite_the_api_refuses_answers_browser_unavailable` (integration): the fake API answers 429, then 500, then no token in the vault → `[browser_unavailable]` naming the cause; no Sprite row.
- **Dimension 3.6** — a stale run creates nothing → Test `a_browser_verb_with_a_stale_fence_creates_nothing` (integration): a superseded fencing token → the stale-fence code, zero API calls.

### §4 — Typed actions and screenshots

`browser_open` takes `{url}` and `browser` takes the typed actions in Interfaces. `screenshot` answers Chromium's `Page.captureScreenshot` PNG through the `image` path: the longest side scaled to 2048 pixels, at most 5 MiB (`guffbolt/src/tools/limits.rs:41-44`). A Chat model takes no image (`guffbolt-ai/src/wire/route/mod.rs:36-48`), so there `screenshot` refuses with `image_input_unavailable` before the capture.

- **Dimension 4.1** — an unknown action is refused → Test `an_unknown_browser_action_is_refused_with_its_name` (unit, guffbolt): `{action:"hover"}` → `invalid_arguments` naming `hover`.
- **Dimension 4.2** — a malformed action is refused → Test `a_click_without_its_target_is_refused` (unit, guffbolt): `{action:"click"}` → `invalid_arguments` naming `target`, and the backend is not called.
- **Dimension 4.3** — a screenshot is a bounded PNG → Test `a_screenshot_is_a_png_within_the_image_bounds` (unit, a fake CDP endpoint): a 4000-pixel-wide capture → a PNG whose longer side is 2048.
- **Dimension 4.4** — no picture for a model that cannot see one → Test `a_screenshot_on_a_chat_model_is_refused` (unit): the Chat wire → `image_input_unavailable`, and no capture is taken.

### §5 — The three refusal texts retire

`WAITS_FOR_FIRECRACKER` with its uses (`afr_tools/src/generic/env/mod.rs:47-49,304`; `generic/env/lazy.rs:12,95`), its doc line (`afr_tools/src/runtime/mod.rs:128`) and the test `test_browser_tools_refuse_until_firecracker` (`afr_tools/src/sandbox/browser/tests.rs:40,85`) go. A runner whose daemon holds no Sprite token refuses naming that. The catalog's doc comments describe Chromium in a Sprite (`afr_tools/src/catalog/mod.rs:130,132,134`).

- **Dimension 5.1** — the refusal names what is missing → Test `a_browser_without_a_backend_says_so` (unit, `afr_tools`): no backend configured → `[browser_unavailable]` naming the missing backend; no text names Firecracker.

## Interfaces

**Tools.** The names `browser_open`, `browser` and `screenshot` stay.

```
browser_open {url}                              the page's text, as the model reads it
browser {action: "click",  target}              target: a CSS selector
        {action: "type",   target, text}
        {action: "scroll", direction: "up"|"down"}
        {action: "read"} · {action: "back"}
screenshot {}                                   a PNG attached to the next step, through the image path
refusals  [browser_unavailable] <sentence>      no page is open · the page closed · no browser backend · the Sprite was refused
          [host_not_allowed] <host> …           the same text http_request answers
          [image_input_unavailable] …           a model that takes no image
```

**Run verbs.** `POST L/browser` `{fencing_token}` → 201 `{endpoint, credential, expires_at}`; `DELETE L/browser` → 204. The credential is the Sprite's scoped one, never the platform token.

**Codes.** New, numbered at PLAN from the next free in the family: the daemon holds no Sprite token (503); the Sprite API refused (502).

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Server-side request forgery (SSRF) | security | a page sends Chromium to a private or metadata address | `failRequest`; the page sees a network error | `the_browser_refuses_the_metadata_address` |
| Host outside policy | security | a private fleet's page loads a third-party host | failed `host_not_allowed`; the page loads without it | `the_browser_refuses_a_host_a_private_fleet_lacks` |
| Domain Name System (DNS) rebinding | security | a name answers differently to the supervisor and to Chromium | no connection to the private address | `[?]`, after §1 item 5 |
| Token leak | security | the token rides a payload or a log | the token never leaves the daemon | `the_sprite_token_never_reaches_a_runner` |
| Lost session | failure | the CDP connection drops | requests stay paused; `[browser_unavailable] the page closed` | `a_lost_cdp_session_leaves_requests_unanswered` |
| Sprite refused | failure | the API answers 429 or 5xx, or no token | `[browser_unavailable]` naming the cause | `a_sprite_the_api_refuses_answers_browser_unavailable` |
| Leftover Sprite | capacity | a daemon dies before the delete | the boot sweep deletes it | `a_sprite_left_by_an_ended_run_is_swept_at_boot` |
| Stale run | failure | a superseded fence posts | the stale-fence code; no Sprite | `a_browser_verb_with_a_stale_fence_creates_nothing` |
| Malformed action | failure | an unknown or partial action | `invalid_arguments` naming the field | `a_click_without_its_target_is_refused` |

## Invariants

1. One host rule and one address rule decide for every HTTP path — the check calls `af_egress`'s `listed()` and `unblocked`, with no copy (Dimensions 2.1 and 2.3).
2. No request leaves Chromium unchecked — interception is enabled before the first navigation, and a paused request continues only on the supervisor's word (Dimensions 2.3 and 2.5).
3. The platform token never leaves the daemon — only the verb handler loads it, and no wire type has a field for it (Dimension 3.4).
4. No browser outlives its run — the run's end deletes its Sprite, and the boot sweep deletes any left (Dimensions 3.2 and 3.3).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `browser_request_refused` (log) | operator | the check fails a request | `run_id`, `fleet_id`, `reason` (`host_not_allowed`, `blocked_address`), `host` | no path, query, header or body | `the_browser_refuses_a_host_a_private_fleet_lacks` |
| `browser_sprite` (log) | operator | a run's Sprite is created, deleted or swept | `run_id`, `fleet_id`, `change`, `ms` | no token, no credential | `a_runs_sprite_is_deleted_when_the_run_ends` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The spike is recorded before the build (§1) | `git grep -n -e "^- \*\*Dimension 1\.1\*\* — DONE" -- "docs/v2/*/M254_001_*"` | one line | P0 | |
| R2 | The check refuses private, metadata and unlisted hosts (§2) | `make test-unit-all` | exit 0; `the_browser_refuses_a_private_address ... ok`, `the_browser_refuses_the_metadata_address ... ok`, `the_browser_refuses_a_host_a_private_fleet_lacks ... ok` | P0 | |
| R3 | A run's Sprite is created on first use and gone after the run, and its token stays in the daemon (§3) | `make test-integration-rustd` | exit 0; `a_runs_sprite_is_deleted_when_the_run_ends ... ok`, `a_sprite_left_by_an_ended_run_is_swept_at_boot ... ok`, `the_sprite_token_never_reaches_a_runner ... ok` | P0 | |
| R4 | Actions are typed, and a screenshot is a bounded PNG (§4) | `make test-unit-all` | exit 0; `an_unknown_browser_action_is_refused_with_its_name ... ok`, `a_screenshot_is_a_png_within_the_image_bounds ... ok` | P1 | |
| R5 | No runner tool names Firecracker (§5) | `git grep -n -i -w firecracker -- rustd/crates/runner/afr_tools` | 0 matches | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the Firecracker refusal | `git grep -n -w -e WAITS_FOR_FIRECRACKER -e test_browser_tools_refuse_until_firecracker -- rustd` | no output |

**Every hit at `576d0b462`, for SPEC-BLAST,** of `git grep -n -i -w firecracker`.

Production: `afr_tools/src/generic/env/mod.rs:48-49,304`, `generic/env/lazy.rs:12,95`, `afr_tools/src/runtime/mod.rs:128`, `docs/architecture/runner_execution.md:312`. Tests: `generic/env/tests.rs:17,135`, `sandbox/browser/tests.rs:40,85`.

Same spelling, other meaning, kept: the engine's review (`concepts_brief.md`, `docs/reviews/space-sandbox/**`, `m211-toolbox-spikes.md:46-48,106-108`), `rustd/Cargo.toml:996`, `afd_runner/src/reconcile.rs:11,110`, `runner/main/src/main_tests.rs:120` and `scripts/test-runner-startup.sh:219`.

## Out of Scope

- The second backend, our own Firecracker engine on the runner host, is a later milestone. Sprite cost or latency, or a need for a VM per run, triggers it.
- Lightpanda: dropped.
- Web search, page reading through a search vendor and the host grant: M254_002.
- Decrypting HTTPS, and keeping a browser's cookies across runs.
- Renaming or merging the browser tools (Indy: "on the browser side keep it in the current form").

---

## Product Clarity (authoring record)

N/A: no user surface. The Categories are INFRA only; the tools are model-facing.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the spike first, then the check and the Sprite's lifecycle, then the tool surface.
- **Alternatives considered:** our own Firecracker engine first (declined by Indy for now: it is the later second backend); Lightpanda in bubblewrap as a bridge (dropped by Indy: no pixels and an AGPL-3.0 binary); Chromium with `--no-sandbox` inside bubblewrap (declined: it drops Chromium's own sandbox); a forward proxy in front of the Sprite (declined for the first backend: CDP interception needs no route out of our hosts).
- **Patch-vs-refactor verdict:** a **patch** because the tool names stay, `Browser` keeps its shape and one backend is added behind a trait.

## Discovery (consult log)

- > Owner (Indy, Nov 22, 2026): "now i assume the browser isnt working today? how do we make it work? i want the tool perfection to be its own milestone maybe since this PR is huge cleanup" — context: the browser tools answer "no browser" in every real environment.
- > Owner (Indy, Nov 22, 2026): "Keep in this PR" with "the tool can be cut on the browser side keep it in the current form, to get the browser working what do we need to do" — context: the browser tools keep their form; making them work is this milestone.
- > Owner (Indy, Nov 22, 2026): "could we use the light* (slim headless browser is there?) ... a zig based i think ... or any others" and then "lets use lightpanda" — context: the Nov 22 engine, dropped on Nov 26.
- > Owner (Indy, Nov 22, 2026): "write it and push it in this PR" — context: this spec landed, pending, in PR #855.
- > Owner (Indy, Nov 26, 2026): "Essentially add missing features for guffbolt agent harness", "max 2 in 1 milestone for the guffbolt tool updates" and "I need the final scope to fix" — context: M254 holds two Workstreams, this browser spec and M254_002.
- > Owner (Indy, Nov 26, 2026): "Allow it now", "look for the updated lightpanda", "i am thinking to move to the firecracker spec as well" and "Why do you need screenshot" — context: the Lightpanda licence and version, then the engine, then pixels. Lightpanda 1.0.5 (Nov 20, 2026) was the latest stable release and is AGPL-3.0 (`gh api repos/lightpanda-io/browser`).
- > Owner (Indy, Nov 26, 2026): "when will you have this usecase? i am leaning towards a and b" — context: the engines put to him were (a) Chromium in a Fly.io Sprite, (b) our own Firecracker engine and (c) Lightpanda as a bridge. Ruled: a `BrowserBackend` trait, the Sprite first, our engine as a later second backend, Lightpanda dropped, screenshots with Chromium.
- **The first real use case, stated plainly.** No fixture fleet needs a browser today: a `git grep` of `tests/fixtures/fleets/` for browser, click and screenshot finds only `morning-rooster.json:16`, prose about the web app. The likely first case is a coding fleet checking its own preview deploy, a rendered page and a screenshot. Forms and bookings, with a person's approval, come after.
- **Owner decisions on the gap matrix's recommendations** — accepted: every request passes a check that enforces the fleet's `NetworkPolicy` and refuses private and metadata addresses. M254_002 records the search, image and billing rulings; M257_001 records the scheduling, task, model and crate rulings.
- **Consults** — the M254 harness gap matrix (Nov 26, 2026, `576d0b462`) found five contradictions in the Nov 22 text. Three were network gaps: no proxy, no route to the host, and a sandbox that ignores the fleet's toggle.
- **How each contradiction resolves** — a Sprite leaves bubblewrap, and the check reads the fleet's policy. Chromium could not start in bubblewrap, and a Sprite gives it a kernel. The licence question went with Lightpanda.
- **Stale references fixed** — the catalog lines are `:131,133,135`; `SandboxEnv` and `LazySandboxEnv` override `browser()`; every run is offered every tool; `act` is `browser.rs:83`. The matrix's Firecracker list also missed `generic/env/mod.rs:304`, `lazy.rs:12,95` and `generic/env/tests.rs:17,135`.
- **Unverified (`[?]`), each settled by §1** — Sprite pricing, limits, cold start and checkpoint time. Whether the Sprite API issues a credential scoped to one Sprite, and the token's vault name. Which request kinds CDP interception pauses, and the rebinding mechanism with its test name.
- **Model reviews** — none for this amendment.
- **Open blockers** — none before §1; §1's record amends this spec where it disagrees.
- **Deferrals and transfers** — the second backend, our own Firecracker engine, to a later milestone, on Indy's "i am leaning towards a and b" above. Each further one needs an owner quote, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
