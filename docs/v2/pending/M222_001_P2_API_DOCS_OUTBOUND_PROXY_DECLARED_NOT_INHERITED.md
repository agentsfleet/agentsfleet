<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M222_001: The daemon reaches a proxy only through a declared knob and host list

**Prototype:** v2.0.0
**Milestone:** M222
**Workstream:** 001
**Date:** Oct 11, 2026
**Status:** PENDING
**Priority:** P2 — operator-facing egress posture; nothing is broken today, but the proxy a vendor call takes is inherited from the process environment
**Categories:** API, DOCS
**Batch:** B1 — single workstream; starts after M218_001 merges
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M218_001 — it lands `afd_identity::direct`, `rustd/clippy.toml`'s `disallowed-methods` list, and the two suites (`schedule_proxy`, `github_proxy`) proving today's environment-proxy behaviour that this spec replaces
**Provenance:** LLM-drafted (claude-opus-5-5, Oct 11, 2026) from Indy's M218 review decision; client facts read from `feat/m218-shared-crates` at `b41496274`
**Canonical architecture:** `docs/architecture/scaling.md` §Tuneup knobs and when to turn them

---

## Overview

**Goal (testable):** With `OUTBOUND_PROXY_URL` set, the scheduler's management calls and the GitHub library fetch reach their hosts through that proxy, every other daemon client connects directly, and `HTTPS_PROXY` in the environment changes no daemon route.

**Problem:** After M218, two `agentsfleetd` clients (the QStash scheduler client and the GitHub library fetch) send through whatever proxy `HTTPS_PROXY`, `HTTP_PROXY` or `ALL_PROXY` names. The four credential-bearing clients ignore those variables. An operator can't tell from the daemon's configuration which calls leave through a proxy. A proxy variable exported for some other tool on the host silently reroutes the scheduler. A deployment that needs a proxy for a vendor call can't name one without also exporting a variable that every process on the host reads.

**Solution summary:** A new owner, `afd_identity::proxy`, beside `afd_identity::direct`, builds the only clients allowed to take a proxy. It takes the proxy URL from a new boot knob, `OUTBOUND_PROXY_URL`, and the passthrough hosts as an explicit list. The list is built at boot from the scheduler's configured API base host and the two GitHub hosts. A request to a listed host goes through the proxy. A request to any other host connects directly. Every environment proxy variable is ignored. `rustd/clippy.toml` adds `reqwest::ClientBuilder::proxy` to `disallowed-methods`, so a proxy added anywhere outside the owner fails `make lint-rustd`. When the knob is unset, every daemon client connects directly. This is a behaviour change for any deployment that relied on `HTTPS_PROXY`, and the changelog says so.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(rustd): the daemon takes a proxy only from OUTBOUND_PROXY_URL`
- **Intent (one sentence):** An operator decides in one knob which proxy the daemon uses, and the code decides which hosts may use it; a variable exported for another tool no longer reroutes vendor calls.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_identity/src/lib.rs` — the crate that gains the `proxy` owner. On the merged base, read its `direct` module first: the one-owner shape this spec mirrors (a builder, a caller-named deadline, the owner's `#[expect(clippy::disallowed_methods)]`).
2. `rustd/crates/agentsfleetd/src/credentials.rs` — `schedule_client`, the scheduler builder this spec rewires; on the merged base, `serve/clients.rs` beside it opens every outbound client at boot.
3. `rustd/crates/agentsfleetd/src/preflight/knobs.rs` — how a boot knob is declared, documented and read through `afd_core::env`.
4. `rustd/crates/afd_library/src/github.rs` — `GithubSource::new`, the GitHub fetch builder, and `validate_redirect`'s two hosts.

These pointers resolve on `main` today. M218_001 adds what this spec builds on: the workspace `clippy.toml` with its `disallowed-methods` list, and the single-test binaries `schedule_proxy` (agentsfleetd) and `github_proxy` (afd_library), which keep environment writes sound under edition 2024. Read all three on the merged base at CHORE(open).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_identity/src/proxy.rs` | CREATE | The owner: the passthrough host list, the proxied client builder, the knob's parsed value type |
| `rustd/crates/afd_identity/src/lib.rs` | EDIT | Export the `proxy` module |
| `rustd/crates/afd_identity/src/direct.rs` | EDIT | Doc comment points at `proxy` as the only other way to build a client |
| `rustd/crates/afd_identity/tests/proxied_client.rs` | CREATE | Listed host via proxy, unlisted host direct, environment ignored |
| `rustd/crates/afd_identity/Cargo.toml` | EDIT | `url` dependency if the parsed type needs it (workspace pin, no new version) |
| `rustd/clippy.toml` | EDIT | Add `reqwest::ClientBuilder::proxy`; reword the `Client::builder` reason to name both owners |
| `rustd/crates/agentsfleetd/src/preflight/knobs.rs` | EDIT | Declare `OUTBOUND_PROXY_URL_KNOB` |
| `rustd/crates/agentsfleetd/src/preflight.rs` | EDIT | Read and validate the knob at boot |
| `rustd/crates/agentsfleetd/src/preflight/config.rs` | EDIT | Carry the parsed proxy on the boot config |
| `rustd/crates/agentsfleetd/tests/preflight_optional.rs` | EDIT | Knob unset, valid, wrong scheme, userinfo present |
| `rustd/crates/agentsfleetd/src/credentials.rs` | EDIT | `schedule_client` builds through `afd_identity::proxy`; its `#[expect]` goes |
| `rustd/crates/agentsfleetd/src/serve/clients.rs` | EDIT | Hand the boot proxy to the scheduler builder; module doc names the declared route |
| `rustd/crates/agentsfleetd/src/serve/runtime.rs` | EDIT | Pass the parsed proxy from config into `Clients::open` |
| `rustd/crates/agentsfleetd/tests/schedule_proxy.rs` | EDIT | Prove the scheduler goes through the knob's proxy and ignores `HTTPS_PROXY` |
| `rustd/crates/afd_library/src/github.rs` | EDIT | `GithubSource::new` takes the proxy; its `#[expect]` goes |
| `rustd/crates/afd_library/src/catalogue/import.rs` | EDIT | Thread the proxy to `GithubSource::new` |
| `rustd/crates/afd_library/tests/github_proxy.rs` | EDIT | Prove the GitHub fetch goes through the knob's proxy and ignores `HTTPS_PROXY` |
| `rustd/crates/afd_library/src/github/tests.rs` | EDIT | Constructor call sites |
| `rustd/crates/afd_library/src/github/tests/transport.rs` | EDIT | Constructor call sites |
| `docs/architecture/scaling.md` | EDIT | `OUTBOUND_PROXY_URL` row in §Tuneup knobs |
| `docs/architecture/runner_fleet.md` | EDIT | One line under §Traps: the daemon-side proxy is not the runner sandbox's egress, which still takes no forward proxy |

The `GithubSource::new` and `schedule_client` caller lists come from `git grep -n 'GithubSource::new\|schedule_client'` on `feat/m218-shared-crates`. Re-run the grep on the merged base at CHORE(open) and extend this table before any edit.

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — NDC (the two `#[expect]` annotations and the `proxied_schedule_client` test seam die when their reason dies), NLR (the environment-proxy wording in `clients.rs`, `credentials.rs` and `clippy.toml` is rewritten, not left beside the new rule), NLG (no fallback to `HTTPS_PROXY` when the knob is unset; no alias knob), UFS (knob name, host names and log event names are named constants), OWN (one owner builds proxied clients), PSR (parse the knob with `url::Url`, never by hand), ECL (a refused proxy connect stays distinguishable from a vendor refusal in the scheduler error), TCF (each proxy test fails when the route is wrong, proven by a red run).
- `.orly/dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — the knob's parse failure is a new `ErrorKind` variant through `afd_core::error_shell!`, never a hand-written error type.
- `.orly/docs/LOGGING_STANDARD.md` — the boot line naming the proxy host carries no userinfo and uses `error_code` on refusal.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `write_rust` | yes — every edit is `*.rs` | Owner module, `garde`-free (a boot knob, not request input), error kinds via `error_shell!` |
| `write_any` — LOGGING | yes — new boot log line | Event name constant; proxy logged as scheme + host + port only |
| `write_any` — UFS | yes | Knob name, GitHub host constants reused from `afd_library`, no inline literals |
| `write_any` — MILESTONE ID | yes | No `M222` in test names or code comments |
| `write_documentation` | yes — architecture docs edited | `DOCUMENTATION_RULES.md` voice; the docs-repo page is a separate branch |
| File & Function Length (≤350/≤50/≤70) | yes | `proxy.rs` holds the builder and host list only; parse lives in `preflight.rs`'s existing knob readers |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_identity/src/direct.rs` — same owner shape: a builder function, a caller-named deadline, a `clippy.toml` entry that makes the owner the only call site.
- **Reference:** reqwest 0.13.5 `Proxy::custom` (workspace pin, `rustd/Cargo.toml`) — the per-URL proxy decision, applied after `ClientBuilder::no_proxy()` so no environment proxy survives. The implementing agent reads the pinned source for builder ordering before writing the owner.

## Sections (implementation slices)

### §1 — One owner builds every proxied client

`afd_identity::proxy` exposes a passthrough host list and a builder. The builder starts from `afd_identity::direct(timeout)`, so environment proxies are already off. It then adds one `Proxy::custom` that returns the configured proxy URL for a request whose host is on the list and `None` for any other host. With no proxy configured it returns the direct builder unchanged. `rustd/clippy.toml` adds `reqwest::ClientBuilder::proxy`, and the owner carries the one `#[expect]`. **Implementation default:** the host list is built in code at boot from configured values, never read from its own environment knob, because Indy's decision is that the code declares which calls may route through a proxy.

- **Dimension 1.1** — A request to a listed host arrives at the proxy as an absolute-form request line → Test `a_listed_host_goes_through_the_proxy`
- **Dimension 1.2** — A request to an unlisted host from the same client connects directly, and the proxy sees nothing → Test `an_unlisted_host_connects_directly`
- **Dimension 1.3** — With `HTTPS_PROXY` and `ALL_PROXY` pointing at a second listener, a proxied client sends nothing there → Test `an_environment_proxy_reroutes_nothing`
- **Dimension 1.4** — A `.proxy(` call outside the owner fails `make lint-rustd`, naming `disallowed_methods` (a seeded red run, recorded in Discovery) → Test `seeded_client_builder_proxy_fails_lint`

### §2 — `OUTBOUND_PROXY_URL` is a validated boot knob

The knob is declared in `preflight/knobs.rs` and read through `afd_core::env`. It parses with `url::Url`. Unset means no proxy. The scheme must be `http` or `https`. A URL carrying userinfo is refused, because proxy credentials need a vault reference that this spec doesn't build (Out of Scope). Any refusal stops boot and names the knob without echoing the value. **Implementation default:** refuse userinfo rather than redact it, because a credential in a plain environment knob breaks the vault rule (RULE VLT) whatever the logs do.

- **Dimension 2.1** — The knob unset yields no proxy, and boot proceeds → Test `the_proxy_knob_unset_means_no_proxy`
- **Dimension 2.2** — `socks5://` or an unparseable value refuses boot with the knob name in the error → Test `a_proxy_knob_with_another_scheme_refuses_boot`
- **Dimension 2.3** — `http://user:pass@proxy:3128` refuses boot, and neither `user` nor `pass` appears in the error or the log → Test `a_proxy_knob_with_credentials_refuses_boot_without_echoing_them`
- **Dimension 2.4** — A valid knob logs one boot line with scheme, host and port → Test `a_valid_proxy_knob_logs_its_host_once`

### §3 — The scheduler and the GitHub fetch take the declared proxy

`schedule_client` builds through `afd_identity::proxy` with the host from the configured scheduler API base, which `QSTASH_URL` or the default sets. `GithubSource::new` builds through it with `api.github.com` and `codeload.github.com`. Both lose their `#[expect(clippy::disallowed_methods)]`. The M218 suites `schedule_proxy` and `github_proxy` are rewritten so they prove the knob's route and also prove that `HTTPS_PROXY` alone routes nothing. The four `afd_identity::direct` clients (broker, plane, outbound, vendor exchange) are untouched and stay direct with the knob set. **Implementation default:** a GitHub redirect to `codeload.github.com` stays on the proxy because both hosts are listed; `validate_redirect` still refuses any other host.

- **Dimension 3.1** — With the knob set, a scheduler management call arrives at the proxy → Test `the_scheduler_goes_through_the_declared_proxy`
- **Dimension 3.2** — With only `HTTPS_PROXY` set, the scheduler connects directly → Test `the_scheduler_ignores_an_environment_proxy`
- **Dimension 3.3** — With the knob set, the GitHub library fetch and its codeload redirect arrive at the proxy → Test `the_github_fetch_goes_through_the_declared_proxy`
- **Dimension 3.4** — With only `HTTPS_PROXY` set, the GitHub fetch connects directly → Test `the_github_fetch_ignores_an_environment_proxy`
- **Dimension 3.5** — With the knob set, the vendor exchange client still connects directly → Test `a_credential_bearing_client_never_takes_the_proxy`
- **Dimension 3.6** — A client that will not build still refuses boot once `Clients::open` takes the proxy (regression) → Test `a_vendor_client_that_cannot_build_refuses_boot`

### §4 — The route is documented where operators look

`docs/architecture/scaling.md` §Tuneup knobs gains the `OUTBOUND_PROXY_URL` row: what it routes, what it never routes, that `HTTPS_PROXY` is ignored, and that credentials in the URL refuse boot. `docs/architecture/runner_fleet.md` §Traps gains one line separating this daemon-side proxy from the runner sandbox's egress, which keeps "no forward proxy". The changelog entry and the docs-repo page land on `chore/m222-outbound-proxy-changelog` in `~/Projects/docs`.

- **Dimension 4.1** — The knob row exists and names both routed surfaces → Test `the_scaling_doc_names_the_proxy_knob`
- **Dimension 4.2** — The runner trap line distinguishes the daemon proxy → Test `the_runner_trap_separates_the_daemon_proxy`

## Interfaces

```
// afd_identity::proxy
pub struct Passthrough { /* hosts: Vec<String>, ASCII-lowercased at build */ }
impl Passthrough {
    pub fn new(hosts: impl IntoIterator<Item = impl Into<String>>) -> Self;
}

pub struct OutboundProxy { /* url: url::Url, scheme http|https, no userinfo */ }
impl OutboundProxy {
    pub fn parse(value: &str) -> Result<Self>;   // the knob's only parser
}

/// direct(timeout), plus the proxy for listed hosts when one is configured.
pub fn proxied(
    timeout: Duration,
    proxy: Option<&OutboundProxy>,
    passthrough: &Passthrough,
) -> reqwest::ClientBuilder;

// agentsfleetd boot knob
OUTBOUND_PROXY_URL = "http://egress-proxy.internal:3128"   // unset → no proxy
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Wrong scheme | `OUTBOUND_PROXY_URL=socks5://…` | Boot refuses, `error_code` names the knob; process exits non-zero before serving |
| Credentials in URL | `http://u:p@host` | Boot refuses; the error and the log carry the knob name, never the userinfo |
| Unparseable value | `OUTBOUND_PROXY_URL=::` | Boot refuses with the knob name |
| Proxy unreachable | Proxy host down at request time | The scheduler write fails within `SCHEDULE_TIMEOUT` with a connect error and the tenant's schedule write returns its existing failure; the GitHub import fails with its existing `Github` error; neither falls back to direct |
| Environment proxy only | `HTTPS_PROXY` set, knob unset | Every daemon client connects directly, as a test asserts; the changelog names the change |
| Unlisted host through proxied client | A future caller sends to a host off its list | Direct connect, never the proxy, as Dimension 1.2 asserts |

## Invariants

1. Only `afd_identity::proxy` adds a proxy to a reqwest client — `clippy.toml` `disallowed-methods` on `reqwest::ClientBuilder::proxy`, denied in `make lint-rustd`.
2. No daemon client reads an environment proxy — every builder starts from `afd_identity::direct`'s `no_proxy()`, and `reqwest::Client::new`/`builder` stay disallowed outside the owners.
3. A credential-bearing client never takes the proxy — the four are built by `vendor_exchange_client`, which calls `direct` and never `proxied`; Dimension 3.5 fails if that changes.
4. Proxy credentials never enter configuration — `OutboundProxy::parse` refuses userinfo, and it is the knob's only parser.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `outbound_proxy_configured` (boot log line) | ops | Boot with a valid `OUTBOUND_PROXY_URL` | scheme, host, port, passthrough host count | no userinfo (refused at parse), no path | `a_valid_proxy_knob_logs_its_host_once` |
| `outbound_proxy_refused` (boot log line, `error_code`) | ops | Boot with an invalid knob | knob name, refusal reason class | value never echoed | `a_proxy_knob_with_credentials_refuses_boot_without_echoing_them` |

No product analytics change; no funnel playbook update.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration (loopback) | `a_listed_host_goes_through_the_proxy` | proxy listener on loopback, host `scheduler.invalid` listed → listener reads `GET http://scheduler.invalid/… HTTP/1.1` |
| 1.2 | integration (loopback) | `an_unlisted_host_connects_directly` | same client, unlisted loopback origin → origin answers 200, proxy listener accepts zero connections |
| 1.3 | integration (own binary) | `an_environment_proxy_reroutes_nothing` | `HTTPS_PROXY`/`ALL_PROXY` → second listener; request to unlisted host → second listener accepts zero connections |
| 1.4 | manual | `seeded_client_builder_proxy_fails_lint` | `make lint-rustd` exits non-zero with `disallowed_methods` naming `ClientBuilder::proxy`; seed removed; recorded in Discovery |
| 2.1 | unit | `the_proxy_knob_unset_means_no_proxy` | empty env source → config proxy `None`, boot config builds |
| 2.2 | unit | `a_proxy_knob_with_another_scheme_refuses_boot` | `socks5://h:1`, `::` → error naming `OUTBOUND_PROXY_URL` |
| 2.3 | unit | `a_proxy_knob_with_credentials_refuses_boot_without_echoing_them` | `http://u:secretpw@h:1` → error; rendered error and captured log contain neither `u:` nor `secretpw` |
| 2.4 | unit | `a_valid_proxy_knob_logs_its_host_once` | `http://h:3128` → exactly one `outbound_proxy_configured` line with host `h`, port `3128` |
| 3.1 | integration (own binary) | `the_scheduler_goes_through_the_declared_proxy` | knob → loopback proxy, scheduler base `http://scheduler.invalid` → proxy reads the request line |
| 3.2 | integration (own binary) | `the_scheduler_ignores_an_environment_proxy` | `HTTPS_PROXY` → listener, knob unset → listener accepts zero connections |
| 3.3 | integration (own binary) | `the_github_fetch_goes_through_the_declared_proxy` | knob → loopback proxy → proxy reads requests for `api.github.com` and `codeload.github.com` |
| 3.4 | integration (own binary) | `the_github_fetch_ignores_an_environment_proxy` | `HTTPS_PROXY` only → listener accepts zero connections |
| 3.5 | integration (own binary) | `a_credential_bearing_client_never_takes_the_proxy` | knob set, vendor exchange client to a loopback origin → proxy accepts zero connections |
| 4.1 | manual | `the_scaling_doc_names_the_proxy_knob` | `grep -c 'OUTBOUND_PROXY_URL' docs/architecture/scaling.md` ≥ 1 |
| 4.2 | manual | `the_runner_trap_separates_the_daemon_proxy` | `grep -c 'OUTBOUND_PROXY_URL' docs/architecture/runner_fleet.md` ≥ 1 |
| 3.6 | integration | `a_vendor_client_that_cannot_build_refuses_boot` | existing `serve/clients.rs` test: a builder that will not build still refuses boot with the new `open` signature (regression) |

Every "accepts zero connections" assertion is proven able to fail by a red run that lists the host, recorded in Discovery (RULE TCF).

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Proxied clients route listed hosts only (§1) | `cd rustd && cargo test -p afd_identity --all-features --test proxied_client` | exit 0 | P0 | |
| R2 | The knob refuses bad values without echoing them (§2) | `cd rustd && cargo test -p agentsfleetd --all-features --test preflight_optional` | exit 0 | P0 | |
| R3 | Scheduler and GitHub fetch take the declared proxy, ignore the environment (§3) | `cd rustd && cargo test -p agentsfleetd --all-features --test schedule_proxy && cargo test -p afd_library --all-features --test github_proxy` | exit 0 | P0 | |
| R4 | No proxy added outside the owner (§1) | `git grep -n '\.proxy(' -- rustd/crates ':!rustd/crates/afd_identity/src/proxy.rs' ':!**/tests/**'` | 0 matches | P0 | |
| R5 | Operators find the knob (§4) | `grep -c 'OUTBOUND_PROXY_URL' docs/architecture/scaling.md docs/architecture/runner_fleet.md` | both counts ≥ 1 | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.orly/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `.orly/dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{N}_{NNN} R{n}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; the two M218 proxy suites are rewritten in place.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `proxied_schedule_client` (M218 test seam) | `git grep -n 'proxied_schedule_client' -- rustd` | 0 matches |
| environment-proxy `#[expect]` reasons | `git grep -n 'honours an environment proxy' -- rustd` | 0 matches |

## Out of Scope

- Proxy authentication. A proxy needing credentials gets a vault-referenced secret knob in its own spec; until then a URL with userinfo refuses boot.
- Routing the four credential-bearing clients through a proxy. They stay direct by design (`afd_identity::direct`).
- Any runner-side or sandbox proxy. `docs/architecture/runner_fleet.md` keeps "no forward proxy" for runner egress.
- An operator-editable host list. The list is declared in code; widening it is a code change with review.
- Per-surface proxies (a different proxy per vendor).

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator behind a corporate egress proxy sets `OUTBOUND_PROXY_URL`, boots the daemon, sees one `outbound_proxy_configured` line naming the proxy host, and a tenant's schedule write succeeds through it while the proxy log shows no credential-bearing vendor call.
2. **Preserved user behaviour** — A deployment with no proxy variables behaves exactly as today. Tenants' schedule writes and library imports keep their existing error shapes.
3. **Optimal-way check** — The unconstrained optimum is a per-surface declared route with authenticated proxies. One proxy plus a code-declared host list delivers moment #1. The gap is proxy auth, which is listed Out of Scope with its own spec.
4. **Rebuild-vs-iterate** — Iterate. M218 already centralised client building in `afd_identity::direct`, and this adds the second owner beside it.
5. **What we build** — `afd_identity::proxy`, one knob, two rewired builders, one `clippy.toml` line, two architecture doc rows, changelog.
6. **What we do NOT build** — Proxy auth, because it needs a vault design. Credential-bearing routes, because they are direct by rule. An operator host list, because the code owns the route. Runner proxies, because the architecture refuses them.
7. **Fit with existing features** — It builds on M218's `direct` owner and `disallowed-methods` gate. It must not destabilise the scheduler write path, so the 10 s `SCHEDULE_TIMEOUT` stays.
8. **Surface order** — Operator environment knob only; no CLI or UI surface.
9. **Dashboard restraint** — N/A — no UI surface.
10. **Confused-user next step** — The boot refusal names `OUTBOUND_PROXY_URL` and the reason class. `docs/architecture/scaling.md` §Tuneup knobs and the docs-repo page say that `HTTPS_PROXY` is ignored and what to set instead.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** owner → knob → callers → docs, so the owner and its lint exist before any caller moves, and each caller's tests flip from "honours the environment" to "honours the knob" in one Section.
- **Alternatives considered:** (a) keep honouring `HTTPS_PROXY` but restrict hosts with a custom matcher. This was rejected because the route would still be inherited from the environment. (b) Read the host list from a second knob. This was rejected because Indy's decision is that the code declares which calls route through a proxy.
- **Patch-vs-refactor verdict:** this is a **patch** because it adds one owner beside an existing one and rewires two builders; no client topology changes.

## Discovery (consult log)

- **Consults** — Indy (Oct 11, 2026, M218 review, recorded in the M218 checkpoint): "Proxy — test, then add": tests proving the QStash scheduler and GitHub fetch honour `HTTPS_PROXY` land in M218; an explicit proxy (`proxy.rs` with a passthrough-host list) is a new follow-up spec. Agent choices made at authoring, open to Indy at PLAN: knob name `OUTBOUND_PROXY_URL`; the passthrough list means the hosts that **may** use the proxy, all others direct; userinfo refused rather than redacted; `HTTPS_PROXY` ignored once this lands, which is a behaviour change for deployments that relied on it. Architecture consult: `docs/architecture/runner_fleet.md` §Traps forbids a forward proxy for runner egress; this spec is daemon-side and adds a line saying so.
- **Metrics review** — two boot log lines added (`outbound_proxy_configured`, `outbound_proxy_refused`); no analytics/funnel playbook update required, no product event exists for boot configuration.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
