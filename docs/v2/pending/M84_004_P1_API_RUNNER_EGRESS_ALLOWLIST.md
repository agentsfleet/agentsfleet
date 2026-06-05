# M84_004: Runner egress allowlist — own-netns child + default-deny DNS-pinning proxy

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 004
**Date:** Jun 05, 2026
**Status:** PENDING
**Priority:** P1 — security boundary. Closes the **tenant-secret exfiltration** path that M84_003 leaves open: an untrusted sandboxed agent holding the lease's own secrets (LLM `api_key`, a GitHub Personal Access Token (PAT), tool secrets) can POST them to any host because the network-enabled tier shares the host network namespace with no egress restriction. Gates network-enabled untrusted/local-runner General Availability (GA).
**Categories:** API
**Batch:** B1 — standalone; sequences after M84_003 (shares the bwrap argv path).
**Branch:** {feat/m84-runner-egress-allowlist — added at CHORE(open)}
**Depends on:** **M84_003 (sandbox env/fd/cap/kill hardening)** — M84_003 removes the *daemon's* `ZOMBIE_RUNNER_TOKEN` from the child (`--clearenv`/`environ_map`); this workstream removes everything *else's* ability to leave the box. They are complementary: M84_003 stops the daemon credential getting *in*; M84_004 stops the tenant credentials getting *out*. M84_004 edits the same `appendBwrap` / network-policy surface, so it lands after M84_003 to avoid a rebase on that file.
**Provenance:** agent-generated — surfaced in the **Orly Chief Technology Officer (CTO) adverse review of M84_003 (Jun 05, 2026)**, code-grounded against `main`. The threat model was re-scoped twice with Indy to the **actual deployment**: baremetal (no Virtual Machine), no cloud metadata service, no co-located Postgres/Redis, no inbound listener — an outbound-only execution node running bwrap/Landlock/NullClaw agents continuously.

> **Provenance is load-bearing.** Every claim below was verified by reading `network.zig`, `sandbox_args.zig`, `runner_network_policy.zig`, `daemon/config.zig`, `zombie_memory.zig`, `engine/runner.zig`, and `build_runner.zig` under an adversarial lens. Re-confirm at PLAN.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Sandbox tiers / §Egress model and [`docs/architecture/data_flow.md`](../../architecture/data_flow.md) (the "never escapes the sandbox" guarantee). This workstream makes the egress half of that guarantee true for the network-enabled tier.

---

> **DEFERRED — behind the untrusted-runner GA trigger (CEO review + Indy, Jun 05, 2026).** v2 launches platform-operated (trusted) runners on usezombie's own baremetal; this own-netns + egress proxy is the unlock for untrusted / customer-operated runners, a **post-launch expansion**. **Do not build for launch.** Launch-time compensating control for the egress/exfil risk: the default LLM provider keys are **$-capped** (a rogue/prompt-injected agent that exfiltrates the key cannot spend beyond the cap; usezombie absorbs that cut), paired with **least-privilege / short-lived tool secrets** (the $-cap bounds LLM spend, not a stolen GitHub token's blast radius). This spec stays in `pending/` as the unlock plan; it lands when usezombie commits to untrusted/customer-operated runners.

## Implementing agent — read these first

1. `src/runner/engine/network.zig` — `PolicyMode` (`deny_all` / `registry_allowlist`) and `appendNetworkArgs`; today `registry_allowlist` emits `--share-net` and only `log.debug`s the allowlist (lines ~68-76). This is the surface that changes from "share host netns + log" to "own netns + brokered egress".
2. `src/runner/engine/runner_network_policy.zig` — `REGISTRY_ALLOWLIST` (the log-only host list). Becomes the seed for the **enforced** allowlist, single-sourced (RULE UFS).
3. `src/runner/sandbox_args.zig` — `appendBwrap`; where `--unshare-all` / `--share-net` are decided. The child must keep an **unshared** net namespace on every sandboxed tier.
4. `src/runner/daemon/config.zig` — `network.policyFromMap` (`RUNNER_NETWORK_POLICY`, parent-only read, line ~70); where an operator-declared `allow_hosts` set is parsed and carried.
5. `src/runner/engine/runner.zig` §4 (memory) + `build_runner.zig` (`.engines = "base,sqlite"`) — proof the child holds **no** database credential and makes **no** datastore connection (Invariant 3); durable memory is the control plane's job over HTTPS.
6. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m84): runner egress allowlist — own-netns child + default-deny proxy`
- **Intent (one sentence):** A prompt-injected sandboxed agent can only reach the hosts the operator explicitly allowed (its inference endpoint/gateway plus declared `allow_hosts`); every other destination — arbitrary exfil targets included — is dropped at the kernel/proxy, not merely logged.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm the egress enforcement point** (own-netns + forward proxy vs in-netns nftables) and that the resolved allowlist includes the configured inference endpoint(s)/gateway — a too-tight list silently breaks every lease (the agent cannot reach its Large Language Model (LLM)); a too-loose one re-opens exfil.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE UFS** — the allowlist seed (`REGISTRY_ALLOWLIST`), the proxy address/port, the `HTTPS_PROXY`/`HTTP_PROXY` env names, and any bwrap net flags are single-sourced named constants, reused by builder + tests — never re-spelled.
  - **RULE NLG** — pre-2.0: the `--share-net` behaviour is **replaced**, not shimmed; no "legacy network mode" framing.
  - **RULE NDC / NLR** — no dead code; the log-only `log.debug("allowlist_host", …)` line is removed when its enforcement lands (it becomes real), not left beside the new path.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets.
- **`docs/AUTH.md`** — the inference `api_key` and tool secrets cross into the child on the lease; this workstream constrains where the child can *send* them, and must not change how they are delivered.
- **`docs/LOGGING_STANDARD.md`** — any `egress_denied` / `egress_allowed` emit follows the logfmt envelope; never log a secret or a full URL with query.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets. |
| UFS | **yes** — allowlist + proxy address + proxy env names | Named constants in `network.zig` / `runner_network_policy.zig`, reused in tests. |
| LENGTH (≤350/≤50/≤70) | **maybe** — `network.zig` grows | Extract a netns/proxy-args emit helper so `appendNetworkArgs` stays ≤50 lines. |
| LOGGING | **yes** — new egress decision emit | Envelope unchanged; host only, never the secret or full URL. |
| LIFECYCLE | **maybe** — if the proxy is a managed child/handle | If a per-host proxy process/socket is owned, `errdefer`-close on the setup path; prefer a long-lived host daemon over per-lease processes (Failure Modes). |
| SCHEMA / ERROR REGISTRY | **maybe ERROR REGISTRY** — a distinct `egress unavailable` failure class | If the lease fails closed on a missing proxy, register/reuse the sandbox-establish failure code; no schema. |

---

## Overview

**Goal (testable):** On a network-enabled sandboxed tier, the child runs in its **own** network namespace and can `connect()` only to the operator-allowed hosts (inference endpoint/gateway + declared `allow_hosts`); a `connect()`/HTTPS request to any other host — `attacker.example`, a raw IP, link-local — is **denied at the enforcement point**, proven by a negative test, while a legitimate LLM call and an allowed-host tool call still succeed.

**Problem:** The only network-enabled tier (`registry_allowlist`) emits bubblewrap `--share-net` — the child **joins the host network namespace** — and the allowlist is **log-only** (`network.zig:68-76`, `runner_network_policy.zig`). So a sandboxed agent has **full host egress**. The agent legitimately holds the tenant's inference `api_key` and tool secrets (e.g. a GitHub PAT) in its address space; a prompt injection (platform-ops agents read untrusted issue/PR text by design) can exfiltrate them to any host. M84_003's `--clearenv` removes only the *daemon's* token — the *tenant's own* secrets are untouched. There is no middle ground today: a tier is either no-network (`deny_all`, empty netns) or all-network.

**Solution summary:** Stop sharing the host network namespace. The sandboxed child keeps an **unshared** net namespace whose only reachable next hop is a **DNS-pinning forward proxy** that enforces a **default-deny hostname allowlist** (the configured inference endpoint(s)/gateway + the operator's declared `allow_hosts`). Everything not on the list is dropped. The operator's `allow_hosts` declaration (the thing they thought they were configuring) becomes a real kernel/proxy boundary instead of a log line. Legitimate inference and allowed-host tool traffic are unchanged; arbitrary exfil and lateral reach are removed.

**Prioritization.** This is the **#1 residual risk for the real deployment** after M84_003: on a baremetal outbound-only node there is no metadata endpoint and no co-located datastore to attack, so lateral movement is moot — the surviving threat is secret exfiltration over the wire, and it shares the same channel as the agent's legitimate inference call, which is exactly why a blanket block is impossible and a hostname allowlist is required.

---

## Prior-Art / Reference Implementations

- **Untrusted-code egress firewall pattern** — give the workload its own network namespace with no default route, and a single forward proxy as the only next hop that allowlists by Server Name Indication (SNI) / `CONNECT` host (squid/tinyproxy `CONNECT` allowlists; the egress-proxy pattern used in front of microVM/agent sandboxes). bubblewrap has no Layer-7 (L7) filtering of its own — it can only share or unshare the namespace — so the proxy is mandatory; reuse the existing `appendBwrap` arg-emit style for the netns/route flags.
- **DNS pinning** — the proxy resolves the allowed name itself and pins the connect IP to that resolution, so a forged-SNI / direct-IP / DNS-rebinding attempt to a non-allowed address is refused. Mirrors standard Server-Side Request Forgery (SSRF) proxy hardening.
- **`REGISTRY_ALLOWLIST`** (`runner_network_policy.zig`) — the existing host list is the seed for the enforced set; do not re-spell it.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/engine/network.zig` | EDIT | `appendNetworkArgs`: never `--share-net`; keep the child's net namespace unshared and wire the brokered-egress path (own-netns + proxy next hop). Remove the log-only allowlist emit. |
| `src/runner/engine/runner_network_policy.zig` | EDIT | Promote `REGISTRY_ALLOWLIST` to the **enforced** seed; add the proxy address + env-name constants (UFS). |
| `src/runner/sandbox_args.zig` | EDIT | Ensure the child keeps an unshared net namespace on every sandboxed tier; pass the proxy env (`HTTPS_PROXY`/`HTTP_PROXY`) into the child's allowlisted environment (coordinated with M84_003 §1). |
| `src/runner/daemon/config.zig` | EDIT | Parse an operator-declared `allow_hosts` set alongside `RUNNER_NETWORK_POLICY` (parent-only); carry it to the builder. |
| `src/runner/engine/network_test.zig` (+ a runner integration test) | EDIT/CREATE | Unit golden-arg tests; **Linux-only integration tests** (allowed host reachable, denied host refused, raw-IP refused, link-local refused, proxy-down fail-closed) on the M84_003 runner integration lane. |
| `make/test-integration.mk` | EDIT | Register the egress integration tests on the `test-integration-runner` lane (created in M84_003). |
| `docs/architecture/runner_fleet.md` | EDIT (small) | Document the egress model under §Sandbox tiers. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one atomic workstream (B1) — own-netns + proxy + config plumbing share the network path; separate tests. It is a **refactor of the network tier** (the `--share-net` model is replaced), behaviour-preserving for the legitimate inference/allowed-host path.
- **Alternatives considered:** (a) **In-netns nftables rules instead of a proxy** — viable for IP allowlists but `allow_hosts` are *names* on rotating Content Delivery Networks (CDNs) (github.com, api.anthropic.com); name-based egress is an L7 concern → a proxy is the correct layer. nftables can complement it (drop everything except the proxy hop) but cannot be the name-allowlist itself. (b) **Keep `--share-net`, add host-global firewall rules** — rejected: rules on a shared namespace mutate the *host's* networking and cannot be per-lease. (c) **Force `deny_all` until enforcement exists** — rejected as the end state: the agent's core function (the LLM call) needs egress, so `deny_all` breaks every network-enabled lease; it remains only as the honest *interim* posture (Out of Scope / Failure Modes) until this lands.

---

## Sections (implementation slices)

### §1 — The sandboxed child runs in its own network namespace (no `--share-net`)

The child must never join the host network namespace. On a network-enabled tier it keeps an **unshared** net namespace whose only route out is the egress proxy; with no proxy reachable it has no egress at all (fail-closed). This removes the "full host egress" property at its root — direct connects to anywhere (host loopback, the host's Local Area Network (LAN), the wider internet) fail because there is no route except via the broker.

- **Dimension 1.1** — no sandboxed tier emits `--share-net`; the child's net namespace is unshared on `deny_all` AND the network-enabled tier → Test `test_no_share_net_on_any_sandboxed_tier`
- **Dimension 1.2** — with the proxy unreachable, the child has **no** egress (fail-closed, not fall-open) → Test `test_egress_fails_closed_without_proxy`

### §2 — Default-deny DNS-pinning egress proxy (the hostname allowlist enforced)

A forward proxy is the child's only next hop. It allows a `CONNECT`/request **iff** the target host is on the resolved allowlist, resolves the name itself, and pins the connection to its own resolution (no forged-SNI / raw-IP / rebinding bypass). Everything else is dropped and logged (host only).

- **Dimension 2.1** — a request to an allowed host (the configured inference endpoint) succeeds → Test `test_allowed_host_reachable`
- **Dimension 2.2** — a request to a non-allowed host is denied at the proxy → Test `test_denied_host_refused`
- **Dimension 2.3** — a direct connect to a raw IP (allowed host's IP spoofed, or any IP) is refused (DNS-pin) → Test `test_raw_ip_connect_refused`
- **Dimension 2.4** — link-local / loopback / private-range targets are denied (defense-in-depth even on baremetal where no metadata exists) → Test `test_link_local_and_private_denied`

### §3 — Operator-declared `allow_hosts` → enforced policy

The allowlist the operator declares (install config / `RUNNER_NETWORK_POLICY` + an `allow_hosts` set) is the source of truth the proxy enforces, seeded by `REGISTRY_ALLOWLIST` and always including the configured inference endpoint(s)/gateway. This is the parent/daemon's responsibility — never read from the child's environment (the child cannot widen its own allowlist).

- **Dimension 3.1** — the resolved allowlist always contains the configured inference endpoint/gateway; a lease whose inference host is absent fails closed at setup, not mid-run → Test `test_inference_host_always_allowed`
- **Dimension 3.2** — `allow_hosts` is a parent-only read; nothing in the child's environment or lease can extend it → Test `test_allowlist_not_child_extendable`

### §4 — Bypass-resistance + honest residual channels

The allowlist caps the *blast radius*; it is not a complete exfil seal, and the spec says so out loud so operators do not over-trust it.

- **Dimension 4.1** — an allowed host that itself accepts attacker-readable writes (e.g. github.com for a platform-ops agent) is still an exfil channel **by design** — documented, with the real mitigation being short-lived / least-privilege tenant secrets, not the network layer → recorded in Discovery + an operator note (no code).
- **Dimension 4.2** — unknown / encrypted-SNI (Encrypted Client Hello, ECH) and DNS-over-HTTPS (DoH) to a non-allowed resolver are denied (the proxy cannot see an encrypted name → default-deny) → Test `test_unknown_or_encrypted_sni_denied`

---

## Interfaces

> **Illustrative — exact flags / proxy mechanism verified at PLAN.** Contract, not implementation.

```
# Network policy (parent-resolved, carried to the builder)
#   - PolicyMode stays { deny_all, <network_enabled> }; network_enabled NO LONGER means --share-net.
#   - allow_hosts: operator-declared set, seeded by REGISTRY_ALLOWLIST, always ∪ {inference endpoint/gateway}.
# Child environment (allowlisted, coordinated with M84_003 §1):
#   HTTPS_PROXY / HTTP_PROXY -> the egress proxy address (the ONLY reachable next hop)
# Enforcement contract:
#   child connect(target) succeeds  IFF  target host ∈ resolved allow_hosts
#                                   AND  connect IP == proxy's own resolution of that host
#   else  -> denied at the proxy (logged: host only, never secret/full-URL)
#   proxy unreachable -> child has NO egress (fail-closed)
```

Contract: the legitimate path (inference endpoint + declared `allow_hosts`, proxy up) is observably unchanged — the agent's LLM call and allowed tool calls still work; only non-allowed destinations are newly refused.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Allowlist too tight | inference endpoint/gateway omitted from `allow_hosts` | lease fails **closed at setup** (not mid-run); operator widens the declared list. Caught by `test_inference_host_always_allowed`. |
| Allowlist too loose | an exfil-capable host added | by design the operator's call; §4.1 documents that write-capable allowed hosts remain channels — mitigate via least-privilege secrets, not the proxy. |
| Proxy unavailable | egress proxy down / not yet started | child has **no** egress (fail-closed); lease classified a sandbox/egress failure, never falls back to open net. `test_egress_fails_closed_without_proxy`. |
| Forged SNI / raw IP / DNS rebinding | agent connects by IP or spoofs a name | proxy pins to its own resolution → refused. `test_raw_ip_connect_refused`. |
| Encrypted-SNI / DoH tunnel | agent hides the destination name | proxy cannot see the name → default-deny. `test_unknown_or_encrypted_sni_denied`. |
| High lease churn | non-stop agents → per-lease netns/proxy setup cost | prefer a **long-lived host proxy** with per-lease identity over a per-lease proxy process; measure setup latency on the Linux lane. |
| Operator runs `deny_all` | no network configured | unchanged — empty netns, no proxy; the agent has no egress (correct for non-network agents). |

---

## Invariants

1. **No host-netns sharing** — no sandboxed tier emits `--share-net`; the child's net namespace is always unshared. Enforced by `test_no_share_net_on_any_sandboxed_tier`.
2. **Default-deny egress** — a destination not on the resolved allowlist is dropped at the proxy; the child's only reachable next hop is the proxy. Enforced by `test_denied_host_refused` + `test_egress_fails_closed_without_proxy`.
3. **No datastore credential in the child** — the runner is built `base,sqlite` (`build_runner.zig`); the child holds no database connection string and opens no datastore socket; durable memory is the control plane's HTTPS responsibility (`zombie_memory.zig` postgres path is inert in the runner). **Never build the runner with `-Dengines=postgres`.** Enforced by a build-config assertion + the absence of a DB host in the child allowlist.
4. **DNS-pin** — an allowed connection's IP must equal the proxy's own resolution of the allowed name; raw-IP / forged-SNI / rebinding is refused. Enforced by `test_raw_ip_connect_refused`.
5. **Allowlist is parent-owned** — nothing in the child's environment or lease can widen `allow_hosts`. Enforced by `test_allowlist_not_child_extendable`.
6. **Legitimate path unchanged** — inference endpoint + declared `allow_hosts` (proxy up) produce an identical observable outcome; a golden-arg/integration test pins it.

---

## Test Specification (tiered)

> **Lane:** the Linux-only integration tests run on the **`test-integration-runner`** lane created in M84_003 (`zig build --build-file build_runner.zig test-integration`) — they create net namespaces, drive a real proxy, and assert kernel-level connect refusal, a privileged-Linux environment. `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS). macOS dev-loop proof = cross-compile the runner TEST graph for both linux targets.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_no_share_net_on_any_sandboxed_tier` | builder output contains no `--share-net`; net namespace unshared on every sandboxed tier |
| 1.2 | integration-runner | `test_egress_fails_closed_without_proxy` | proxy absent → child `connect()` to any host fails; lease classified egress-failure, not open-net |
| 2.1 | integration-runner | `test_allowed_host_reachable` | allowlist=\{inference host\}; child reaches it; request succeeds |
| 2.2 | integration-runner | `test_denied_host_refused` | child request to a non-allowed host → refused at proxy; `egress_denied` logged (host only) |
| 2.3 | integration-runner | `test_raw_ip_connect_refused` | child connect to a raw IP → refused (DNS-pin), no bypass of the name allowlist |
| 2.4 | integration-runner | `test_link_local_and_private_denied` | child connect to 127.0.0.1 / 169.254.x / RFC1918 → refused |
| 3.1 | unit + integration-runner | `test_inference_host_always_allowed` | configured inference endpoint present in resolved allowlist; missing → fail-closed at setup |
| 3.2 | unit | `test_allowlist_not_child_extendable` | an `allow_hosts`-like value in child env/lease does NOT widen the enforced set |
| 4.2 | integration-runner | `test_unknown_or_encrypted_sni_denied` | encrypted/absent SNI or DoH to a non-allowed resolver → default-deny |

- **Regression:** existing runner suite (`make test-unit-zigrunner`) + a legitimate end-to-end lease (inference + an allowed tool host) still pass — the network-enabled agent still works.
- **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [ ] No sandboxed tier emits `--share-net`; child net namespace unshared — verify: `test_no_share_net_on_any_sandboxed_tier`
- [ ] Allowed inference host reachable; non-allowed host refused — verify: `test_allowed_host_reachable` + `test_denied_host_refused`
- [ ] Raw-IP / link-local / private-range / encrypted-SNI denied — verify: `test_raw_ip_connect_refused` + `test_link_local_and_private_denied` + `test_unknown_or_encrypted_sni_denied`
- [ ] Proxy-down fails closed (no open-net fallback) — verify: `test_egress_fails_closed_without_proxy`
- [ ] Inference endpoint always in the resolved allowlist; allowlist not child-extendable — verify: `test_inference_host_always_allowed` + `test_allowlist_not_child_extendable`
- [ ] Child holds no datastore credential; runner not built with postgres engine — verify: `grep -n 'engines' build_runner.zig` shows `base,sqlite`; no DB host in the child allowlist
- [ ] `make lint` clean · `make test-unit-zigrunner` + `make test-integration-runner` pass · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] `runner_fleet.md` documents the egress model

---

## Eval Commands (post-implementation)

```bash
# E1: no --share-net anywhere in the network builder
git grep -n 'share-net' src/runner && echo "FAIL: share-net still present" || echo "PASS"
# E2: runner unit + app suites (legitimate path unchanged)
make test-unit-zigrunner 2>&1 | tail -5 && make test 2>&1 | tail -5
# E3: egress integration lane (allowed/denied/raw-IP/link-local/proxy-down)
make test-integration-runner 2>&1 | tail -10
# E4: dev-loop proof — Linux-only bodies compile
zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux 2>&1 | tail -3
# E5: runner is base,sqlite (no DB credential in the child)
grep -n 'engines' build_runner.zig
# E6: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — none expected.**

| File to delete | Verify |
|----------------|--------|
| N/A — refactor of the network path, no file removed | — |

**2. Orphaned references.** The log-only `log.debug("allowlist_host", …)` emit and `--share-net` literal are removed when enforcement lands; grep must show zero remaining `share-net` in `src/runner` (E1).

---

## Discovery (consult log)

- **Origin (Jun 05, 2026):** Orly CTO adverse review of M84_003. Pinned residual: `registry_allowlist` emits `--share-net` (`network.zig:68-76`) = full host egress; allowlist log-only (`runner_network_policy.zig`). M84_003 §5.1 characterizes this gap with `test_registry_allowlist_egress_unrestricted_today`; this workstream closes it (that pinned test flips when this lands).
- **Threat re-scoping with Indy (Jun 05, 2026):** the deployment is **baremetal, not a VM** → no cloud metadata service (`169.254.169.254` IMDS is cloud-only). **No co-located Postgres/Redis** and **no inbound listener** on the runner box (`control_plane_client.zig` is outbound `std.http.Client.fetch`; no `listen`/`bind`). So lateral movement and loopback-to-datastore are moot; the surviving threat is **outbound secret exfiltration**, which shares the inference channel — hence a hostname allowlist, not a blanket block.
  - **Memory path confirmed (Jun 05, 2026):** the runner is built `.engines = "base,sqlite"` (`build_runner.zig`); `zombie_memory.zig`'s postgres path is inert (`findBackend("postgres")` → null). NullClaw memory in the child is ephemeral workspace SQLite; durable zombie memory is the control plane's Postgres reached over the HTTP API (`src/zombied/.../memories`). The untrusted child never holds a DB DSN → Invariant 3.
  - **Indy decision (verbatim, Jun 05, 2026):** _"update the M84_003 spec, and the egress spec"_ — context: file this egress workstream as its own spec for an adversarial review with Codex; reword M84_003 §5.1 to stop implying the tenant's own secrets are protected under the network-enabled tier.
- **Deferrals** — none yet. Any "deferred to follow-up" needs an Indy-acked verbatim quote here.
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr` results.}

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits negative/regression coverage vs this Test Specification. | Clean. Iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs Invariants, Failure Modes, `dispatch/write_zig.md`, `docs/architecture/runner_fleet.md`. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Runner unit | `make test-unit-zigrunner` | {paste snippet} | |
| App suite (regression) | `make test` | {paste snippet} | |
| Egress integration | `make test-integration-runner` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (TEST graph) | `zig build --build-file build_runner.zig test-integration -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- **M84_003's process-boundary hardening** (env/fd/cap/kill) — that is the sibling spec; this one owns only network egress.
- **Inbound network policy** — the runner box has no listener; nothing to filter inbound.
- **Cloud metadata / IMDS blocking** — not applicable to the baremetal deployment (no metadata service exists); the link-local deny (Dim 2.4) covers it for free if a node ever runs in a VM.
- **Rotating / scoping tenant secrets** so a leak is lower-impact — the real mitigation for the §4.1 allowed-host exfil channel, but a credential-model change, not a runner network patch. Names the follow-up.
- **Interim posture before this lands:** because the inference call needs egress, `deny_all` is the only fully-safe stopgap and it disables network agents; until this workstream ships, the compensating control is **short-lived / least-privilege tenant secrets** (documented, not enforced here).
