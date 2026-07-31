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

# M153_001: JWKS fetch decodes a compressed key set instead of parsing gzip bytes as JSON

**Prototype:** v2.0.0
**Milestone:** M153
**Workstream:** 001
**Date:** Jul 31, 2026
**Status:** DONE
**Priority:** P0 — every authenticated request on dev returns 503; the dashboard is unusable for every signed-in operator
**Categories:** API
**Batch:** B1 — single workstream, no parallel peer
**Branch:** feat/m153-jwks-decompress
**Test Baseline:** unit=3335 integration=510
**Depends on:** M152_001 (this repairs the bounded-transport rewrite that milestone introduced)
**Provenance:** agent-generated (pre-spec, live diagnosis of run 30614520801 + standalone reproduction against the dev identity provider)
**Canonical architecture:** `docs/AUTH.md` §Backend validation (the common path)

---

## Overview

**Goal (testable):** `fetchCapped` returns decoded JSON when the identity provider answers `content-encoding: gzip`, and rejects a key set whose *decompressed* size exceeds the named cap.

**Problem:** Every signed-in operator on `app-dev.agentsfleet.net` lands on "Something went wrong" instead of their workspace. The dashboard entry redirect calls `GET /v1/tenants/me/workspaces`, the daemon answers `503 UZ-AUTH-004` ("Authentication service unavailable"), and the error boundary takes the page. Sign-in itself succeeds, so the failure reads to the user as "logged in, but the product is empty". Command-Line Interface (CLI) login is broken the same way.

**Solution summary:** The JSON Web Key Set (JWKS) transport reads the response body through the decompressing reader instead of the raw one, so a `content-encoding: gzip` key set arrives as JSON. The existing byte cap moves onto the decompressed stream, where it bounds a decompression bomb rather than merely bounding wire bytes. No caller, configuration knob, or public signature changes — the daemon simply verifies tokens again.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m153): JWKS fetch decodes compressed key sets
- **Intent (one sentence):** Signed-in operators reach their workspace again, because the daemon can read the identity provider's signing keys.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/auth/jwks_fetch.zig` — the bounded transport being repaired; the whole diff is this file plus its tests.
2. `src/agentsfleetd/auth/jwks_test.zig` — `PartialJwksServer` / `OverCapJwksServer` are the local-listener harness the new tests mirror; the cap and leak tests already live here.
3. `docs/AUTH.md` §Backend validation — how the verifier sits in the request path and which configuration knobs feed the JWKS Uniform Resource Locator (URL).
4. Zig standard library `std/http/Client.zig` — `Response.reader` is documented to return *compressed* bytes when compression was negotiated; `Response.readerDecompressing` is the decoding counterpart that `Client.fetch` uses.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/auth/jwks_fetch.zig` | EDIT | Body read moves to the decompressing reader; the cap moves onto decompressed bytes and gains a decompression buffer. |
| `src/agentsfleetd/auth/jwks_test.zig` | EDIT | Adds the compressed-transport tests: gzip decode, decompression bomb, cap boundary, malformed and unadvertised encodings, mid-body death, non-200, unparseable Uniform Resource Locator (URL). |
| `docs/AUTH.md` | EDIT | Records how the key set is fetched and why the cap counts decompressed bytes — the durable form of what this milestone learned. |
| `docs/v2/active/M153_001_P0_API_JWKS_COMPRESSED_KEYSET_DECODE.md` | CREATE | This spec. |
| `build.zig.zon` | EDIT | Repoints the `pg` pin at the fork commit that fetches `translate-c` from GitHub instead of codeberg — the unblock for this milestone's own Continuous Integration (CI). |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (the decompression-buffer size and every new literal land as named constants beside `JWKS_MAX_RESPONSE_BYTES`), **ECL** (a cap rejection stays `ResponseTooLarge`, distinct from a transport `FetchFailed` — the two must not collapse), **OWN** (the decompression buffer gets exactly one cleanup path; the existing `Allocating` writer keeps its `defer`), **NDC** (no leftover raw-reader branch once the decompressing path lands), **TVR** (the bomb fixture must actually exceed the cap after inflation, not merely look large), **XCC** (cross-compile both linux targets before commit).
- `~/Projects/dotfiles/dispatch/write_zig.md` — memory safety, `errdefer` placement, file ≤ 350 / function ≤ 50, tagged-union results, cross-compile both linux targets.
- `docs/AUTH.md` — auth-flow surface: read before editing anything the verifier depends on.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — the diff is `*.zig` | Cross-compile `x86_64-linux` and `aarch64-linux`; `errdefer`/`defer` pairing audited on every new allocation. |
| PUB / Struct-Shape | no — `fetchCapped` keeps its signature and `FetchError` keeps its variants | No new `pub` surface; the file's shape verdict is unchanged. |
| File & Function Length (≤350/≤50/≤70) | yes — `fetchCapped` grows by the decompression wiring | The file is well under the cap; if `fetchCapped` approaches 50 lines, the buffer-sizing choice splits into a named helper. |
| UFS (repeated/semantic literals) | yes — a decompression window size is introduced | Named constant beside `JWKS_MAX_RESPONSE_BYTES`, sized from the standard library's flate window constant rather than a magic number. |
| UI Substitution / DESIGN TOKEN | no — no user interface (UI) file is touched | N/A. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no new log line, lifecycle object, error code, or schema change | The existing `jwks_fetch_failed` / `jwks_parse_failed` lines already cover both outcomes; `UZ-AUTH-004` stays the registered code. |

## Prior-Art / Reference Implementations

- **Reference:** Zig standard library `std.http.Client.fetch` — it negotiates encoding, sizes a decompression buffer from the response's `content_encoding`, and reads through `readerDecompressing`. That is precisely the behaviour M152 replaced by hand and must restore, with the cap added on top. Mirror its buffer-sizing switch rather than inventing one.
- **Reference:** `src/agentsfleetd/auth/jwks_test.zig` `OverCapJwksServer` — the existing local-listener shape for cap tests; the bomb server is that server with a pre-deflated body.

## Sections (implementation slices)

### §1 — Decode the key set

The transport reads through the decompressing reader, so a provider that honours the client's own `accept-encoding` header returns something the JSON parser can read. This is the slice that ends the outage: with it, `parseJwks` succeeds, the cache populates, and every authenticated route stops answering 503.

**Implementation default:** keep the client's default `accept-encoding` negotiation and decode the result, rather than suppressing compression with an `omit` header. Suppressing it would also work, but it makes the daemon pay full wire cost on every refresh and leaves the same trap armed for the next reader of this file.

- **Dimension 1.1** — DONE — a `content-encoding: gzip` key set parses into usable keys → Test `jwks fetch decodes a gzip-encoded key set and verifies a real token`
- **Dimension 1.2** — DONE — an uncompressed (`identity`) key set still parses, unchanged → Test `jwks fetch success path delivers the key set byte-intact over loopback` (pre-existing; now the identity-path regression anchor)

### §2 — Keep the read bounded after decoding

M152's cap exists so a config-controlled endpoint cannot make the daemon accumulate without limit. Decoding moves the threat: a small compressed body can inflate past any wire-byte cap. The cap therefore counts decompressed bytes, which bounds both the honest oversize response and the hostile bomb.

- **Dimension 2.1** — DONE — a small gzip body that inflates past the cap is rejected, and the partial accumulation is freed → Test `jwks fetch rejects a compressed body that inflates past the cap`
- **Dimension 2.2** — DONE — an uncompressed body past the cap is still rejected (M152's guarantee, unbroken) → Test `jwks fetch still refuses an oversize uncompressed body as a cap refusal`

### §3 — Prove the failure classes stay distinct

A cap rejection and a transport fault are different operator situations: the first says the provider sent something implausible, the second says the network or provider is down. They already map to different `FetchError` variants, and the decoding path must not collapse them — including when the decompressor itself rejects malformed bytes.

- **Dimension 3.1** — DONE — a body whose declared encoding does not match its bytes fails as a transport fault, never as a silent empty key set → Test `jwks fetch keeps the cap refusal distinct from a transport fault`
- **Dimension 3.2** — DONE — an endpoint that dies mid-body still frees the partial accumulation on the decoding path → Test `jwks fetch frees the partial body when a compressed stream dies mid-body`

## Interfaces

```
src/agentsfleetd/auth/jwks_fetch.zig  (unchanged signature — this is a locked surface)

  pub const FetchError = error{ OutOfMemory, FetchFailed, ResponseTooLarge };
  pub const JWKS_MAX_RESPONSE_BYTES: usize   // now bounds DECOMPRESSED bytes
  pub fn fetchCapped(alloc, url) FetchError![]u8   // returns decoded body; caller frees

Callers unchanged:
  jwks.zig  fetchJwksJson  → OutOfMemory propagates; every other error → VerifyError.JwksFetchFailed
  Observable HTTP behaviour: a healthy provider yields 200 on authenticated routes
  instead of 503 UZ-AUTH-004. No route, status code, or response body shape changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Compressed key set | Provider honours `accept-encoding: gzip` | Body is decoded before parsing; caller observes a normal key set and a 200 on authenticated routes. |
| Decompression bomb | Hostile or compromised provider Uniform Resource Locator (URL) returns a small body inflating past the cap | Read stops at `JWKS_MAX_RESPONSE_BYTES` of *decompressed* output; `ResponseTooLarge` → `VerifyError.JwksFetchFailed` → 503 `UZ-AUTH-004`; accumulation freed. |
| Oversize identity body | Provider returns an uncompressed body past the cap | Unchanged from M152: rejected at the cap, accumulation freed. |
| Malformed compressed body | Encoding header disagrees with the bytes | Decompressor read error → `FetchFailed`; never a silently empty or truncated key set. |
| Endpoint dies mid-body | Connection drops after a partial compressed body | `FetchFailed`; the partially-written body is freed on the decoding path exactly as on the raw path. |
| Provider unreachable | Network fault or bad issuer configuration | `FetchFailed` → stale-serve if a cache exists, else 503 `UZ-AUTH-004` — unchanged behaviour. |

## Invariants

1. The cap counts bytes the caller could receive — decompressed output, never wire bytes — enforced by the accumulation check running on the decoded stream, proven by Dimension 2.1.
2. A cap rejection and a transport fault never collapse into one variant — enforced by `FetchError` remaining a three-variant set with distinct return sites, proven by Dimensions 2.1 and 3.1 asserting different variants.
3. Every allocation on the fetch path has exactly one cleanup path — enforced by `defer` on the allocating writer and the decompression buffer, proven by the testing allocator's leak detector in Dimensions 2.1 and 3.2.
4. `fetchCapped` never returns bytes the JSON parser cannot read for a well-formed provider response — enforced by Dimension 1.1 parsing the returned body through the production `parseJwks`, not a test-local parser.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | Repair restores the pre-M152 behaviour of existing `jwks_fetch_failed` / `jwks_parse_failed` warn lines; no event is added, renamed, or removed | not applicable | no token, key material, or response body is logged — the existing lines carry an error name only | `test_jwks_fetch_decodes_gzip_keyset` (absence of the failure line is the signal) |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `jwks fetch decodes a gzip-encoded key set and verifies a real token` | Loopback listener serves the fixture key set gzip-encoded with `content-encoding: gzip`; a real token verifies through the live `Verifier` and yields subject `user_test`. Covers the transport and the layer the middleware actually calls in one walk, so no separate verifier row is needed. |
| 1.2 | unit | `jwks fetch success path delivers the key set byte-intact over loopback` | Pre-existing test, unchanged: the same key set served uncompressed still verifies a real token — the identity path did not regress. |
| 2.1 | unit | `jwks fetch rejects a compressed body that inflates past the cap` | 300 KiB of one repeated byte, gzipped to a few hundred wire bytes (asserted under the cap on the wire), returns `ResponseTooLarge`; the testing allocator reports no leak. |
| 2.2 | unit | `jwks fetch still refuses an oversize uncompressed body as a cap refusal` | The over-cap uncompressed server yields `ResponseTooLarge` asserted directly on `fetchCapped` — M152's guarantee, and the variant the `Verifier` would otherwise hide. |
| 3.1 | unit | `jwks fetch keeps the cap refusal distinct from a transport fault` | A response declaring `content-encoding: gzip` whose body is plain JSON returns `FetchFailed` — not `ResponseTooLarge`, and not a silently-accepted key set. |
| 3.2 | unit | `jwks fetch frees the partial body when a compressed stream dies mid-body` | A listener promising a full gzip body then hanging up halfway returns `FetchFailed` with no leak reported. |
| — | regression | pre-fix run of the four new tests | Reverting `jwks_fetch.zig` to the pre-fix transport fails all four new tests (gzip decode dies on `UnexpectedToken`; the bomb and malformed cases return raw bytes; the truncated case leaks) — the tests pin the fix rather than passing either way. |
| — | regression | existing `jwks_test.zig` suite | Every pre-existing JWKS test stays green; the M152 partial-body and over-cap leak tests are unchanged in behaviour. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A gzip-encoded key set parses and a real token verifies; the outage path is closed (§1) | `zig build test-auth --summary all` | exit 0, all tests pass | P0 | ✅ `236 pass (236 total)` |
| R2 | The cap bounds decompressed bytes, not wire bytes (§2) | `zig build test-auth --summary all` | exit 0; the bomb test passes | P0 | ✅ bomb gzips to under the cap on the wire and still returns `ResponseTooLarge` |
| R3 | Failure classes stay distinct (§3) | `zig build test-auth --summary all` | exit 0; malformed encoding is `FetchFailed`, not `ResponseTooLarge` | P0 | ✅ both asserted on distinct variants via `fetchCapped` directly |
| R4b | The new tests fail against the pre-fix transport | restore the pre-fix `jwks_fetch.zig`, run `zig build test-auth` | tests fail, then pass once restored | P0 | ✅ `228 pass, 4 fail (232 total); 3 leaks` pre-fix → `236 pass` restored |
| R5 | Diff-scoped mutation on changed lines | flip `>`→`>=`, drop the buffer free, shrink the decode window | each mutant killed | P0 | ✅ 3/4 killed; M5 (window on the identity path) justified equivalent — no observable behaviour change |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 4/4 listed after adding `docs/AUTH.md` + the spec (review finding) |
| S1 | Unit tests pass | `make test-unit-agentsfleetd` | exit 0 | P0 | ✅ exit 0; `zig build test` → `2053/2351 passed (298 skipped)`, 0 failed |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ exit 0 (first run hit `lint-cli` 127 — unhydrated worktree, not the diff) |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `Full integration suite passed` |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | ✅ `memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)` |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ both targets exit 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (4048 commits scanned) |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -vE '\.md$\|_test\.zig$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output. Command corrected to exclude `_test.zig`, matching the repo's own gate (`lint-zig.py`); see Discovery for the follow-up flag |
| S9 | Orphan sweep | `grep -n "response.reader(" src/agentsfleetd/auth/jwks_fetch.zig` | 0 matches | P0 | ✅ 0 |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| raw-reader body path | `grep -n "response.reader(" src/agentsfleetd/auth/jwks_fetch.zig` | 0 matches |

## Out of Scope

- The `cpu` controller missing from the bare-metal runner host's `cgroup.subtree_control` — a separate host-provisioning failure in the `deploy-worker-dev` job, unrelated to the auth path; needs its own spec.
- The stale `// reachable: no — CLI/API-key surface, not fetched by ui/packages/app` annotation on `UZ-AUTH-004` in `error_entries.zig` — a correctness wart in the registry's reachability notes, outside this diff's Files Changed scope.
- Any change to how the user interface (UI) surfaces an authentication outage. The dashboard error boundary behaved correctly here: it refused to show a misleading "create a workspace" empty state and retried instead.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy signs in at `app-dev.agentsfleet.net` and the fleet wall for their workspace renders, instead of "Something went wrong".
2. **Preserved user behaviour** — sign-in, CLI `login`, tenant Application Programming Interface (API) keys, and runner tokens all keep working exactly as they do; this restores a path rather than changing one.
3. **Optimal-way check** — this is the most direct route: one file, the reader call the standard library already documents as correct. The gap to unconstrained-optimal is that the daemon has no startup probe that would have caught a dead key-set fetch before traffic hit it; `doctor` has the check but nothing runs it on boot. Acceptable now because the repair is urgent and the probe is a separate, larger question.
4. **Rebuild-vs-iterate** — iterate. The bounded-transport shape M152 introduced is right; it has one wrong call in it.
5. **What we build** — the decoding read, the cap moved onto decompressed bytes, and the six tests that pin both.
6. **What we do NOT build** — a boot-time key-set probe (needs its own design), a retry/backoff change (the existing stale-serve and rate-limit behaviour is correct), and any change to compression negotiation on other outbound fetches in the daemon.
7. **Fit with existing features** — this compounds with every authenticated surface; the one thing it must not destabilize is the cap M152 added, which is why §2 exists as its own slice rather than as a footnote to §1.
8. **Surface order** — neither; this is a daemon-internal repair with no new surface. The user-visible outcome arrives through surfaces that already exist.
9. **Dashboard restraint** — nothing new to hide; no control or claim is added.
10. **Confused-user next step** — an operator seeing the 503 today has `agentsfleetd doctor`, whose `oidc_jwks_reachability` check names the failing Uniform Resource Locator (URL). That remains the self-serve move.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections split by *guarantee*, not by file: decode it (§1), keep it bounded (§2), keep the failure classes honest (§3). The split exists because the naive fix — swap one reader call — silently converts a wire-byte cap into no meaningful cap at all. Separating §2 forces that consequence to be tested rather than assumed.
- **Alternatives considered:** (a) suppress compression by sending `accept-encoding: omit`, making the raw reader correct again. Rejected: it costs full wire bytes on every refresh and leaves the identical trap for the next person who re-enables negotiation. (b) Revert M152's transport rewrite and return to `client.fetch`. Rejected: it would restore the unbounded read that milestone deliberately closed.
- **Patch-vs-refactor verdict:** this is a **patch** because the surrounding design is sound and the defect is one call plus the cap placement that follows from it. The genuinely larger question this exposes — that no boot-time probe verifies the identity provider is readable before the daemon serves traffic — is named in Out of Scope rather than mud-patched in here.

## Discovery (consult log)

- **Consults** — Architecture: `docs/AUTH.md` §Backend validation gained a "How the key set is fetched" subsection; no `docs/architecture/**` diff, because this repairs an existing flow rather than defining one. Gate-flag triage (mechanical, auto-applied): `zlint unsafe-undefined` on the `std.http.Decompress` out-parameter → `SAFETY:` comment added. Gate-flag triage (judgment, **open for Indy**): two more call sites use the same raw-reader shape — `src/agentsfleetd/cron/QStashClient.zig:238` (reads JSON from QStash; breaks identically if that provider ever compresses) and `src/agentsfleetd/fleet_library/github_net.zig:128` (downloads a tarball, so raw bytes look deliberate). Both sit outside this spec's Files Changed scope and were left untouched pending Indy's fix-or-defer call.
- **Metrics review** — no analytics or funnel playbook update required: this repair adds, renames, and removes no event. The existing `jwks_fetch_failed` / `jwks_parse_failed` warn lines already cover both outcomes and carry an error name only, no key material.
- **Skill-chain outcomes** — `/write-unit-test` (Hardening mode): diff ledger resolved; two gaps found and closed (cap boundary at exactly `JWKS_MAX_RESPONSE_BYTES`, and an unadvertised encoding). Diff-scoped mutation on changed lines: 3/4 killed, M5 justified equivalent. Red-green proof: the pre-fix transport fails 4 of the new tests and leaks; restored, all pass. `/write-integration-test`: tiers T2/T3/T7/T8/T9 not applicable (outbound client — no inbound request lifecycle, no datastore, no streaming, no API surface); T1/T4/T6 met over real loopback HTTP with real compression; T4 audit found two uninjected branches (non-200, unparseable URL) and both were closed. T5 ≥100-connection parallelism not applicable: the fetch is single-flighted and cached for six hours, and that contention path already has a test. gstack `/review`: run single-reviewer — specialist subagent fan-out deliberately skipped, since this session is under a standing instruction not to spawn agents unattended. One scope finding (`docs/AUTH.md` missing from Files Changed) fixed; one stale module doc-comment fixed.
- **Dependency unblock (Indy-directed)** — every Zig job on this Pull Request (PR) failed at dependency fetch, not on the diff: `codeberg.org` shed load, refusing anonymous git clones (`remote: Bye` / 403) while its web tier stayed up. Indy chose to mirror rather than wait. The fix lives in `agentsfleet/pg.zig` (commit `b85d608`), which is our own fork: one line, `translate_c` now fetched from `github.com/ziglang/translate-c` at the identical commit `57c559cf`. Equivalence is proven by the package hash being byte-identical either way — `translate_c-0.0.0-Q_BUWlX1BgCD1wo6uo97prlp9VJ4gxAjwN_vZ7nsSjGN`. This repository changes only the pin, now expressed as a tag plus commit (`?ref=v0.0.0-af.3#1dea0f9`), matching how `nullclaw` is already pinned. The commit is mandatory: `zig fetch` given `?ref=<tag>` alone resolved to the upstream base `52b9f8a`, silently yielding plain upstream with no pool-acquire patch — caught because the package hash changed, which is the backstop. **Remaining exposure:** `z3` (`build.zig.zon:62`) is still fetched from codeberg by tarball; that endpoint recovered first and is not currently blocking, so it was left alone rather than widened into this diff.
- **Deferrals** — none taken. Two items are **flagged for Indy, not deferred** (no ack quote exists or is claimed): the two adjacent raw-reader call sites above, and `jwks_test.zig` now at 1141 lines. The repo's own length gate exempts `_test.zig`, so nothing is red, but the file was already 859 lines before this milestone and is a split candidate. Rubric row S8's command was corrected to match that exemption rather than the gate being changed. **Changelog:** no `<Update>` written — the defect is internal-only. It existed on dev for roughly four hours, the repository carries no tags, and `api.agentsfleet.net` does not resolve, so no released version ever contained it.
