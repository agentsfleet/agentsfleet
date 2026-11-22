# M254_001: The browser tools open, act on and photograph real pages, on Lightpanda

**Prototype:** v2.0.0

**Milestone:** M254

**Workstream:** 001

**Date:** Nov 22, 2026

**Status:** PENDING

**Priority:** P2, the browser tools are published but every real environment answers "no browser"

**Categories:** INFRA

**Batch:** B1, after M253 merges

**Depends on:** M253_004, which keeps `browser_open`, `browser` and `screenshot` as they are and moves search into the executor

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 22, 2026) from Indy's decisions of Nov 22, 2026

**Canonical architecture:** `docs/architecture/runner_execution.md`; this spec adds the browser to the executor there

---

## Overview

**Goal (testable):** A fleet's `browser_open` on `https://example.com` answers the page's text from inside its sandbox, through the egress guard, and `screenshot` answers a PNG of it.

**Problem:**
- **The tools answer nothing.** `browser_open`, `browser` and `screenshot` call `Env::browser()`, whose default answers "has no browser in this environment" (`rustd/guffbolt/src/tools/env/mod.rs:336`). No real environment overrides it; only the test `FakeBrowser` implements `Browser` (`rustd/guffbolt/tests/public_api/network/fake_env/mod.rs:45`).
- **agentsfleet still publishes them.** `browser` and `browser_open` are catalog entries (`rustd/crates/runner/afr_tools/src/catalog/mod.rs:120,122`), so a fleet can name a tool that can only fail.
- **`act` is untyped.** Its arguments are free JSON (`rustd/guffbolt/src/tools/browser.rs:59`).

**Solution summary:**
- **Lightpanda drives pages.** A headless browser written in Zig that speaks the Chrome DevTools Protocol (CDP), about 123 MB on a 100-page run against about 2 GB for headless Chrome (one review's figure). It runs inside the run's sandbox.
- **Chromium only for pixels.** Lightpanda renders no pixels, so `screenshot` starts `chrome-headless-shell` on first use and only then.
- **The executor owns the browser.** Three executor methods, `browser/open`, `browser/act` and `browser/screenshot`, drive it with `chromiumoxide`. `SandboxEnv` and `LocalEnv` implement guffbolt's `Browser` over them.
- **The guard sees every request.** The browser's traffic goes through the runner's egress guard as a proxy, so the allowlist and SSRF rules match `http_request`'s.

## Sections (implementation slices)

### §1 — Spike: Lightpanda under chromiumoxide

Drive Lightpanda 1.x with `chromiumoxide` from a throwaway program: open a page, read its text, click a link, fill a form, and try a screenshot. Then open GitHub's repository and pull-request pages and one single-page app a fleet visits. Record what works in Discovery before §2 starts.

- **Dimension 1.1** — the driver fits → Test: the spike's record in Discovery. Expected: open, read, click and type work; screenshot fails as expected; the real-site results are listed.

### §2 — The executor runs the browser

The executor starts Lightpanda per run, on first use, and holds the page. `browser/open`, `browser/act` and `browser/screenshot` are executor methods, bounded by the frame size the runner sends (M253_004 §0.3). The browser is killed at run end.

- **Dimension 2.1** — a page opens → Test `a_page_opens_and_answers_its_text` (unit, a local page server). Expected: the page's text.
- **Dimension 2.2** — the browser dies with the run → Test `a_runs_browser_is_gone_after_the_run` (unit). Expected: no browser process remains.

### §3 — The sandbox holds the browser

The Lightpanda binary is bound read-only into the bubblewrap sandbox. Landlock and seccomp gain only what it needs, listed with the reason for each. Runner provisioning (`deploy/baremetal/`) installs a pinned Lightpanda and `chrome-headless-shell`.

- **Dimension 3.1** — it runs confined → Test `the_browser_runs_inside_the_sandbox` (Linux lane). Expected: it opens a page; a read outside `/workspace` is refused.

### §4 — The guard sees every request

The browser starts with `--proxy-server` at the runner's egress guard. `file://` and `chrome://` are refused.

- **Dimension 4.1** — a private address is refused → Test `the_browser_cannot_reach_a_private_address` (Linux lane). Expected: refused, as `http_request` refuses it.

### §5 — Screenshots on Chromium, only when asked

`screenshot` starts `chrome-headless-shell` on first use, opens the same URL with the same cookies, and answers a PNG through the existing image path (`[tools] max_image_dimension_px`).

- **Dimension 5.1** — a screenshot is a PNG → Test `a_screenshot_is_a_png_of_the_page` (Linux lane). Expected: a PNG within the configured dimension.
- **Dimension 5.2** — no screenshot, no Chromium → Test `a_run_that_takes_no_screenshot_starts_no_chromium` (unit). Expected: no Chromium process.

### §6 — `act` takes typed arguments

`browser`'s arguments become a typed schema: `click`, `type`, `scroll`, `read`, `back`, each with its fields. The tool names stay as they are.

- **Dimension 6.1** — a malformed action is refused → Test `an_unknown_browser_action_is_refused_with_its_name` (unit). Expected: `invalid_arguments` naming the action.

## Out of Scope

- Renaming or merging the browser tools (Indy: "on the browser side keep it in the current form").
- A hosted browser service.

## Discovery (consult log)

- > Owner (Indy, Nov 22, 2026): "now i assume the browser isnt working today? how do we make it work? i want the tool perfection to be its own milestone maybe since this PR is huge cleanup" — context: the browser tools answer "no browser" in every real environment.
- > Owner (Indy, Nov 22, 2026): "Keep in this PR" with "the tool can be cut on the browser side keep it in the current form, to get the browser working what do we need to do" — context: the file-tool cut stays in M253_004; the browser tools keep their form; making them work is this milestone.
- > Owner (Indy, Nov 22, 2026): "could we use the light* (slim headless browser is there?) ... a zig based i think ... or any others" and then "lets use lightpanda" — context: Lightpanda drives pages; `chrome-headless-shell` is only the screenshot fallback.
- > Owner (Indy, Nov 22, 2026): "write it and push it in this PR" — context: this spec lands, pending, in PR #855.
- **Unverified (`[?]`):**
  - Lightpanda under `chromiumoxide`; its published integrations are Puppeteer, Playwright and chromedp. §1 settles it.
  - Which real sites Lightpanda renders well enough; one comparison says some single-page apps fail. §1 settles it.
  - Usage claims (Keenable, DeveloperHub.io, more than 30 million pages a day for some customers) are Lightpanda's own.
- **Open blockers** — none; §1 runs first.
