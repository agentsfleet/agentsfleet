<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M213_002: The Rust sandbox reaches exactly what the API assigned it — all egress, none, or the allowlist, enforced by nftables on a veth pair the sandbox cannot touch

**Prototype:** v2.0.0
**Milestone:** M213
**Workstream:** 002
**Date:** Oct 06, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — M213_001 cannot cut over without it: on `allow_all` the Zig child shares the host network today and the Rust sandbox reaches nothing, so every package install inside a sandbox breaks on the day of the cutover
**Categories:** API, DOCS
**Batch:** B1 — ships in M213_001's Pull Request (Indy, Oct 06, 2026)
**Branch:** feat/m213-rust-runner-cutover
**Folded-into:** `M213_001`
**Baseline revision:** dd917b7ef42dcb883b5192fafe894060aab5845d
**Test Baseline:** unit=4441 integration=5203 — Rust unit 4441 passed, 0 failed, 924 ignored (runner 1171 · daemon 132 · daemon libraries 3138); integration through the coverage shards 5203 passed, 0 failed (substrate 4260 + 2 exclusive · runner crates 717 · runner against the daemon 2 · daemon 222); TypeScript app 3642, design-system 647, website 142 passed, cli 1777 passed and 17 skipped, at `dd917b7ef` via PR #736's identical tree. The branch at `d82dea239`: Rust unit 4448 passed, 0 failed, 924 ignored on macOS, and the runner shard 1248 passed on Linux (+77 against 1171); integration 897 + 2 exclusive passed, 0 failed
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M213-dd917b7ef.md`
**Depends on:** M213_001 — the same Pull Request; this spec's §1 lands before M213_001 deletes `src/runner/`, because the Zig egress code is the behaviour reference read from `origin/main` · M211_001 — merged in #732 (`bb0070015`); this spec's `afr_sandbox` paths are re-confirmed against that tree at PLAN
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 06, 2026) from `origin/main` at `8cb2318ec` and `feat/m211-sandbox-tools-and-nested-loops` at `74956c0f7`; every cite below was read from source
**Canonical architecture:** `docs/architecture/runner_fleet.md` §"Egress model — outbound is the only network surface"; `docs/architecture/runner_execution.md` (the per-lease network allowlist bullet and the "No runner crate builds the per-lease network allowlist" non-goal this spec retires)

---

## Overview

**Goal (testable):** `test_sandbox_egress_follows_the_assigned_policy` — on a real kernel, a sandbox under `allow_list_egress` connects to an allow-listed host and fails to reach any other address, port 53 included; under `deny_all_egress` it reaches nothing; under `allow_all` it reaches the host's network; and a runner whose probe passes reports `egress_enforcement: true`, so the daemon stops reading it degraded.
**Problem:** Operators already set egress through the API, in two places: the runner's `assigned_policy` (`PATCH /v1/fleets/runners/{runner_id}`, `network_policy` + `registry_allowlist`) and the fleet's `network.allow` (`PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}`, `trigger_markdown`/`config_json`). The Rust sandbox reads neither. `bubblewrap.rs` passes `--unshare-net` on every launch (M211 branch, `afr_sandbox/src/bubblewrap.rs:65`), so `allow_all` behaves as `deny_all_egress` and an `npm install` inside a sandbox fails. `allow_list_egress` has never been enforced by either runner: the Zig probe pins `EGRESS_ENFORCEMENT_BUILT = false` (`src/runner/engine/capability_probe.zig:20`), the Rust probe pins `egress_enforcement: false` (`afr_supervisor/src/capability.rs:51`), and the published runner page says so (`runners.mdx:101`).
**Solution summary:** The supervisor passes the runner's assigned `network_policy` to the sandbox. `allow_all` shares the host network namespace. `deny_all_egress` keeps today's empty namespace. `allow_list_egress` gives each sandbox its own namespace joined to the host by one veth pair, with default-deny nftables rules in the **host** namespace on the host-side link, admitting only the IPv4 set the supervisor resolved at lease bind. That set comes from the runner's `registry_allowlist` plus the fleet's `network.allow`. The sandbox gets a rendered `/etc/hosts` and a resolver-less `/etc/resolv.conf`; port 53 is dropped. The probe reports `egress_enforcement: true` only once it has built and torn down a scope. No API shape changes: the daemon already demands egress control for `allow_list_egress` (`afd_runner/src/reconcile.rs:225-245`).

## PR Intent & comprehension handshake

- **PR title (eventual):** rides M213_001's: `feat(runner)!: deploy the Rust runner; retire the Zig runner and NullClaw`
- **Intent (one sentence):** The network a fleet's sandbox can reach is the one the operator and the fleet author set through the API, enforced by the kernel, on the Rust runner.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afr_sandbox/src/egress.rs` — the scope as built: its module map, the boot probe, the sweep, and how a sandbox's namespace is found. The behaviour reference it was ported from, `src/runner/network/` on `bb0070015`, is deleted by M213_001 §2; Discovery records where the port diverged and why.
2. `rustd/crates/afd_runner/src/reconcile.rs` — what the daemon already demands: `enforces_egress` and `shares_host_net` define each policy's meaning; `EgressControl` is proven by `report.egress_enforcement`.
3. `rustd/crates/afd_gate/src/policy/shape.rs` — the fleet half: an absent `network` block is an empty allow list, never "reach anything"; `read_only` and `read_post_paths` are Layer 7 (L7) rules only the supervisor's `afr_egress` can enforce.
4. `docs/architecture/runner_fleet.md` §Egress model — resolver-less naming, port 53 drop, rules owned by root outside the child; its Zig names (`uzveth`, `network/AllowList.build`) are restated for Rust in §3.
5. `rustd/crates/afr_sandbox/src/bubblewrap.rs`, `warm_slots.rs`, `probe.rs` on the merged M211 tree — where the namespace flag is set, where a slot outlives a lease, and where host capabilities are probed.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/network.rs`, `network/tests.rs` | CREATE | `Network` (host, isolated, allowlisted) and `Allowlist`: the resolved set, its cap, its rendered `/etc/hosts`, and its names, which a held sandbox is filed under |
| `rustd/crates/afr_sandbox/src/bubblewrap.rs`, `bubblewrap/tests.rs` | EDIT | `NetworkLayout`: `allow_all` drops `--unshare-net` and binds the host's resolver files; an allowlist binds the rendered ones |
| `rustd/crates/afr_sandbox/src/egress.rs`, `egress/{slot,netlink,link,rules,rules/expressions,rules/set,kernel,scope,far,testing,testing/replies}.rs` and each one's `*_tests.rs` | CREATE | The scope: process-wide slot claims, the netlink transport, the veth pair over route netlink, the table over `nf_tables`, the set's in-place refill, the kernel seam, the build and removal, the boot probe and sweep, the fake kernel's replies, and the `test-util` far host the kernel lane reaches |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine.rs`, `bubblewrap_engine/{config,held,names,parts,stderr,sweep}.rs`, `bubblewrap_engine/tests/held.rs` | EDIT / CREATE | The engine renders the resolver files, joins the running sandbox to the host, owns the scope in `Parts` until teardown, refills a held sandbox's set and rewrites its `/etc/hosts` before it thaws, and sweeps egress leftovers at boot; `BubblewrapConfig` and the error-stream drain move to their own files to keep the engine under the length cap |
| `rustd/crates/afr_sandbox/src/{engine,unsandboxed}.rs`, `error/egress.rs`, `examples/kernel_lane/egress_reallow.rs` | EDIT / CREATE | `Sandbox::reallow`, which a sandbox built to no allowlist refuses; egress refusals and netlink steps are typed (`EgressRefusal`, `Step`); the kernel-lane proof of the refill |
| `rustd/crates/afr_sandbox/src/warm_slots.rs`, `warm_slots/tests.rs` | EDIT | Slots are built isolated, so only an isolated request is handed one |
| `rustd/crates/afr_sandbox/src/egress/{lock,lock_tests}.rs`, `egress/rules/{host_chains,host_chains_tests}.rs` | CREATE | The host's egress lock the engine takes before its sweep; the reading of the host's forward chains the probe refuses on |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{egress_owned,egress_closed}.rs` | CREATE | The kernel-lane proofs that the table is the runner's, that a dropping host chain fails the probe, and that port 53, the host and inbound connections stay closed |
| `rustd/crates/afd_core/src/net.rs`, `net/tests.rs` | EDIT | `192.0.0.0/24`, the IETF protocol assignments, joins the blocked table every caller shares |
| `.github/workflows/test-integration-rustd.yml`, `make/{test-unit,test-integration-rustd}.mk`, `scripts/toolbox/install-builder-tools.sh`, `.github/actions/build-toolbox/action.yml` | EDIT / CREATE | The kernel job and its coverage report; one builder-package list for it and the toolbox action |
| `rustd/crates/afr_sandbox/src/probe.rs`, `probe/tests.rs`, `rustd/crates/afr_sandbox/src/lib.rs` | EDIT | `HostProbe.egress` is measured; `egress_testing` is exported under `test-util` |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{egress,lane,main,trials}.rs` | CREATE / EDIT | The real-kernel proof for all three policies; the lane refuses a host that does not forward IPv4 |
| `rustd/crates/afr_sandbox/Cargo.toml`, `rustd/Cargo.toml`, `rustd/Cargo.lock`, `make/test-unit.mk` | EDIT | The four MIT `rust-netlink` crates; the `test-util` feature the kernel lane requires and runs with |
| `rustd/crates/afr_egress/src/{lib,allowlist,admission,network,resolve,testing}.rs`, `admission/allowlist_tests.rs`, `network/tests.rs`, `resolve/tests.rs`, `testing/resolver.rs` | EDIT / CREATE | One reading of a `network.allow` entry (`allowlist_host`) for the kernel set and `http_request` admission (Indy, "One parser"); one resolver and one blocked-address rule (`unblocked`) for both egress paths, so a host with any blocked address is refused on each |
| `rustd/crates/afr_supervisor/Cargo.toml`, `rustd/crates/afr_supervisor/src/egress.rs`, `egress/tests.rs` | CREATE | The registry baseline or the default registry set, merged with the fleet's hosts unless `read_only`, resolved to IPv4 with the host resolver; a fleet host that resolves to an address `afd_core::net::is_blocked` refuses is refused |
| `rustd/crates/afr_supervisor/src/error/raise.rs`, `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs`, `lease_loop/egress_tests.rs` | EDIT / CREATE | `EgressBlocked` and its own startup-refusal sentence, chosen beside the egress sentence the other refusals end with |
| `docs/architecture/lease_flow.md` | EDIT | The sandbox command line shows `allow_all` as no `--unshare-net`, not a `--share-net` flag |
| `rustd/crates/afr_supervisor/src/heartbeat.rs`, `capability.rs`, `holds/built_under.rs`, `lease_loop.rs`, `lease_loop/{lessee,refusal,hold,refill_tests}.rs`, `worker_pool.rs`, `error.rs`, `test_support/{rig,resolver,sandbox}.rs` and their tests | EDIT / CREATE | The assignment carries the egress; a lease resolves it before the hold check and passes it to the engine; a hold is filed under the names it reaches, never reused under another egress, and takes the lease's addresses on resume; `egress_enforcement` comes from the probe |
| `rustd/crates/agentsfleet_runner/tests/run.rs`, `tests/fake_daemon.rs` | EDIT | The runner binary settles a lease under each of the three policies |
| `ui/packages/app/components/domain/fleetFailureCopy.tsx`, `fleetFailureCopy.test.ts` | EDIT | The dashboard names the egress refusal the supervisor reports |
| `rustd/crates/afr_tools/src/sandbox/git.rs`, `rustd/crates/afr_tools/src/runtime.rs` | EDIT | The `git` tool's refusal no longer says the sandbox has no network; it names the runner as the only one reaching the remote |
| `playbooks/lib/runner/prepare.sh`, `playbooks/lib/runner/runner_test.sh` | EDIT | Host preparation turns IPv4 forwarding on, persistently, and checks it |
| `docs/architecture/runner_fleet.md`, `docs/architecture/runner_execution.md` | EDIT | §Egress model describes the Rust scope; the non-goal "No runner crate builds the per-lease network allowlist" leaves |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (table, chain, set and link name prefixes, the `/30` base, the allowlist cap, as single named constants shared by builder, sweep and tests); NDC and NLR (the constant `egress_enforcement: false` and the unconditional `--unshare-net` leave, not sit beside the new path); NLG (no "legacy network mode"); OBS (each refusal names its reason); TST-NAM; TCF.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — every new fallible signature; no hand-written error type.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runner page's egress lines on M213_001's docs branch.
- `AGENTS.orly.md` §Hard Safety — "Install-process launches in core paths": netlink from Rust, never the `nft` or `ip` binaries.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — new module | `egress` split by concern (plan, link, rules, names) from the start |
| UFS | yes | Names and limits declared once in `egress.rs` |
| LOGGING | yes | One structured line per scope build, refusal and sweep, no allowlist contents beyond host count |
| Architecture consult | yes | `runner_fleet.md` §Egress model rewritten in the same Pull Request |
| Schema guard | no | No schema change |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/network/` on `origin/main` — the same design (own namespace, veth, host-side nftables, resolve at setup, resolver-less) built for Zig and never switched on. Mirrored by behaviour; one divergence: the inference endpoint leaves the merged set, because the Rust agent loop calls providers from the supervisor, outside the sandbox (`docs/architecture/runner_execution.md` crate graph).

## Sections (implementation slices)

### §1 — The sandbox honours the assigned policy

The heartbeat's `assigned_policy` already reaches `afr_supervisor` and today only labels the self-test (`heartbeat.rs:89-91`). It now reaches the sandbox engine. `allow_all` drops `--unshare-net` and binds the host's own `/etc/hosts` and `/etc/resolv.conf`, so names resolve as on the host; `deny_all_egress` keeps the empty namespace; and `allow_list_egress` keeps the empty namespace until §2 joins it to the host. A change of assignment applies to sandboxes built after it: a held sandbox is keyed by the egress it was built under, and a warm slot is built isolated, so neither is handed to a lease asking for another network. **Implementation default:** an absent assignment is `FAIL_CLOSED_DEFAULT` (`afd_wire/src/runner.rs:70`) because a runner must never open egress on missing input.

- **Dimension 1.1** — DONE — Each policy produces its namespace flag, and no assignment produces the fail-closed one → Test `test_bubblewrap_network_flag_per_policy`
- **Dimension 1.2** — DONE — A reassignment never hands a lease a sandbox built under the previous policy: not a held sandbox, not a warm slot → Test `test_reassignment_retires_held_sandboxes`
- **Dimension 1.3** — DONE — On a real kernel, `allow_all` reaches a host-network listener and `deny_all_egress` reaches nothing → Test `test_kernel_allow_all_and_deny_all`
- **Dimension 1.4** — DONE — The runner binary still settles a lease under each of the three policies (regression on M213_001 Dimension 1.4) → Test `test_runner_binary_runs_a_lease_per_policy`

### §2 — `allow_list_egress` is enforced by the kernel

At lease bind the supervisor merges the runner's `registry_allowlist` — or, when it is empty, the default registry set the wire names — with the fleet's `network_policy.allow`, and resolves each name to IPv4 addresses with the host resolver. When the fleet sets `read_only`, its hosts stay out of the merged set and are reached only through `http_request`, because nftables cannot enforce a method. A slot's veth pair, `afv<slot>` on the host and its peer created straight into the sandbox's namespace, gets a point-to-point `10.69.<slot>.0/30`. The slot's table, `inet afegress<slot>`, lives in the host namespace and is owned by the netfilter socket that built it (`NFT_TABLE_F_OWNER`), which the scope keeps for its life: no other process may delete it, a host `nft flush ruleset` passes it by, and the kernel removes it when that socket closes, a crashed runner's included. Every base chain accepts by policy and every drop names the slot's link, so one table never touches another sandbox's or the host's traffic. Forward drops ports 53, accepts the set, and drops the rest from the link; return traffic is admitted by connection state; input drops everything from the link; postrouting masquerades the `/30`. The table is built in one batch before the link exists, and is removed with the sandbox. A held sandbox is filed under the names it reaches, not their addresses: on resume, while it is still frozen, its set is emptied and refilled with the lease's addresses in one batch through the socket that owns the table, and its `/etc/hosts` is rewritten in place; a refused refill ends the hold and the lease builds fresh. The sandbox gets `/etc/hosts` with each name and its resolved address, and a `/etc/resolv.conf` naming no resolver. An unresolvable or IPv6-only name, a set over the cap or any netlink failure refuses the lease, and the refusal names its reason. So does a host the fleet names that resolves to any address `afd_core::net::is_blocked` refuses, the predicate `http_request` and the daemon's endpoint check share; registry hosts are the operator's and exempt. Before its boot sweep the engine takes the host's egress lock, a `flock` on `/run/agentsfleet/egress.lock`, so a second runner process on the host refuses at boot instead of deleting the first one's live links. The boot sweep deletes every table and link carrying the runner's prefix before the first lease, skipping a slot a live scope in the same process holds. The probe reads `net.ipv4.ip_forward`, refuses a host whose own forward base chain drops by policy (ufw's or Docker's, naming the chain), and builds and removes a whole scope inside two namespaces made for it, never the host's; only success reports `egress_enforcement: true`. **Implementation:** `netlink-sys`, `netlink-packet-core`, `netlink-packet-route` and `netlink-packet-netfilter` (MIT, `rust-netlink`), pure Rust, so the static musl binary keeps zero `NEEDED` entries (M213_001 Dimension 1.1).

- **Dimension 2.1** — DONE — Merge and resolve: registry hosts plus fleet hosts, deduplicated in first-seen order, `read_only` fleet hosts excluded, an empty result admitting nothing → Test `test_egress_plan_merges_and_excludes_read_only`
- **Dimension 2.2** — DONE — The rendered `/etc/hosts` names every merged host and `/etc/resolv.conf` names no server → Test `test_egress_resolver_files_render`
- **Dimension 2.3** — DONE — On a real kernel, the sandbox connects to an allow-listed address and fails on a non-listed address, a link-local address and port 53 → Test `test_kernel_allow_list_admits_only_the_set`
- **Dimension 2.4** — DONE — The sandbox cannot widen its own rules: `nft flush ruleset` inside it changes nothing on the host → Test `test_kernel_sandbox_cannot_flush_host_rules`
- **Dimension 2.5** — DONE — IPv6-only name, set over the cap and netlink failure each refuse the lease with a named reason → Test `test_egress_setup_failures_refuse_the_lease`
- **Dimension 2.6** — DONE — A sandbox's table and link go with it before its slot is reused, and the boot sweep removes a crashed runner's tables and links → Test `test_egress_release_and_boot_sweep`
- **Dimension 2.7** — DONE — The probe reports `egress_enforcement: true` only after a scope builds and tears down, and the daemon then reads an `allow_list_egress` runner healthy → Test `test_egress_probe_reports_enforcement`
- **Dimension 2.8** — DONE — A host the fleet names that resolves to a blocked address (loopback, private, the tailnet's shared range, link-local and the metadata service, the IETF protocol assignments `192.0.0.0/24`, reserved, or an IPv6 spelling of one) refuses the lease with its own sentence, naming the host and never the address; a registry host at a private address is admitted → Test `test_a_fleet_host_at_a_blocked_address_refuses_the_lease`
- **Dimension 2.9** — DONE — The table belongs to the socket that built it: another process's deletion by name is refused, its ruleset flush passes the table by, and the allowlist still holds → Test `test_kernel_host_cannot_delete_a_live_table`
- **Dimension 2.10** — DONE — The probe reports no enforcement on a host whose own forward chain drops by policy, and names the chain → Test `test_egress_probe_refuses_a_dropping_forward_chain`
- **Dimension 2.11** — DONE — A second runner process on the host refuses at boot rather than sweeping the first one's live scopes → Test `test_a_second_holder_is_refused_until_the_first_lets_go`
- **Dimension 2.12** — DONE — On a real kernel an allowlisted sandbox gets no answer from port 53 of a listed address, cannot reach the runner's host, and takes no connection from outside, each beside a control that does connect → Test `test_kernel_allow_list_closes_dns_the_host_and_inbound`
- **Dimension 2.13** — Continuous Integration runs the kernel lane as root under coverage on every Pull Request, and the Rust floor is graded over its report with the shards' → Test `test_ci_measures_the_kernel_lane`
- **Dimension 2.14** — DONE — A held allowlisted sandbox whose names resolve to new addresses keeps its hold: before it thaws, its set holds exactly the new addresses and its `/etc/hosts` names them; a sandbox that refuses the refill is ended and the lease builds fresh → Test `test_kernel_allow_list_refills_in_place`

### §3 — The pages say what the runner does

`runner_fleet.md` §Egress model describes the Rust scope with its Rust names, keeping the dated decisions. `runner_execution.md` drops the non-goal "No runner crate builds the per-lease network allowlist". On M213_001's docs branch, `runners.mdx` stops saying allowlist egress is not enforced, and names the host requirement (nftables in the kernel) and what a degraded runner shows when the probe fails.

- **Dimension 3.1** — DONE — The architecture check passes and no page says allowlist egress is unenforced → Test `test_egress_pages_describe_enforcement`
- **Dimension 3.2** — The docs branch carries the runner page change, and its checks pass → Test `test_docs_branch_carries_egress_page`

## Interfaces

```
Unchanged API:   PATCH /v1/fleets/runners/{runner_id}  assigned_policy.{network_policy, registry_allowlist}
                 PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}  network.allow / read_only (TRIGGER.md)
Unchanged wire:  CapabilityReport.egress_enforcement (now measured) · ExecutionPolicy.network_policy
Sandbox engine:  launch takes the assigned NetworkPolicy; lease bind takes the merged, resolved allowlist
Host objects:    one nftables table and one veth pair per slot, both named with the runner's prefix and slot index
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Host lacks nftables or netlink permission | Kernel without `nf_tables`, or the unit drops the capability | Probe fails; `egress_enforcement: false`; the daemon reads an `allow_list_egress` runner degraded with `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE`; no lease runs open |
| Name does not resolve or resolves IPv6 only | Typo in `network.allow`, or a v6-only host | Lease refused; the runner's log names the host, and the event settles failed with the fixed egress sentence, which names none |
| Allowlist over the cap | Large `registry_allowlist` plus fleet hosts | Lease refused before any rule is installed |
| Fleet host resolves inside | `network.allow` names an internal host, or a public name's DNS points at a private or reserved address | Lease refused at startup with "the fleet allows an egress host at a private or reserved address"; the log names the host, never the address; registry hosts are exempt |
| Netlink call fails mid-build | Kernel error or a race with the sweep | Partial objects torn down; lease refused; the sandbox never starts with a half-built scope |
| Runner crashes holding scopes | Kill or power loss | The kernel removes each owned table when the runner's sockets close; the boot sweep deletes the links, and any unowned table an earlier build left, before the first lease |
| Host flushes its ruleset | `nft flush ruleset`, or a restart of Debian's `nftables.service`, while sandboxes run | The flush passes every owned table by; each sandbox stays held to its allowlist (Dimension 2.9) |
| Host firewall drops forwarding | ufw's `DEFAULT_FORWARD_POLICY="DROP"`, Docker's `FORWARD` chain | Probe fails naming the chain; the runner reads degraded and takes no `allow_list_egress` lease, instead of taking leases whose connections all die (Dimension 2.10). Chains of the legacy `iptables` backend are not read |
| A second runner process starts on the host | A hand-started `agentsfleet-runner run` beside the service, or the kernel lane on a serving host | The second refuses at boot on the host's egress lock; the first's scopes are untouched (Dimension 2.11) |
| Host IP of an allowed name rotates during a lease | CDN churn | The connection fails; the next lease resolves again (the name layer is out of scope) |
| An allowed name resolves elsewhere while its sandbox is held | CDN churn between leases | The hold is kept: before it thaws, its set is refilled in one batch and its `/etc/hosts` rewritten; a refused refill logs `sandbox_refill_failed`, ends the hold and the lease builds fresh (Dimension 2.14) |
| Sandbox tries to edit rules | Hostile code inside | Its namespace holds no rules; the host table is unreachable from it (Dimension 2.4) |
| Assignment missing | Heartbeat reply without a policy | Fail-closed default; no egress opens |

## Invariants

1. A sandbox under `allow_list_egress` never runs before its scope is built; a build error refuses the lease — enforced by the lease bind returning the scope a launch requires as an argument.
2. Egress rules live only in the host namespace and belong to the runner — enforced by the scope taking the host namespace handle, the owner flag on its table, and the kernel-lane tests that flush from inside and delete from outside.
3. A missing or unparsable assignment never opens egress — enforced by `FAIL_CLOSED_DEFAULT` and a unit test on the absent case.
4. `egress_enforcement` is `true` only after a measured probe — the constant leaves; the field is set from the probe result alone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `egress_bind_refused` log line | ops | The supervisor cannot bind the lease's egress (a host does not resolve, resolves to a blocked address, or the set is over the cap) and the lease is refused | error code, reason, lease id, host count, detail | no host names beyond the one refused; no addresses | `an_unresolvable_egress_host_ends_the_lease_at_startup`, `a_fleet_host_at_a_blocked_address_ends_the_lease_at_startup` |
| `egress_scope_refused` log line | ops | The sandbox cannot build a bound lease's scope and the lease is refused | error code, refusal, reason, slot index, host count | no host names; no addresses | `test_egress_setup_failures_refuse_the_lease` |
| `egress_scope_refilled` log line | ops | A held sandbox's set takes the next lease's addresses | slot index, host count | no host names; no addresses | `test_a_scope_takes_new_addresses_in_place` |
| `sandbox_refill_failed` log line | ops | A held sandbox refuses the next lease's addresses; the hold ends and the lease builds fresh | error code, reason, lease id | no host names; no addresses | `a_hold_that_refuses_its_new_address_falls_back_fresh` |
| `egress_enforcement` in the capability report | ops | Every report | boolean | none needed | `test_egress_probe_reports_enforcement` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_bubblewrap_network_flag_per_policy` | `allow_all` → no `--unshare-net`; isolated (the supervisor's answer to `deny_all_egress` and to no assignment) and allowlisted → `--unshare-net` once; `test_resolver_files_follow_the_network` binds the host's files, the rendered ones, or neither |
| 1.2 | unit | `test_reassignment_retires_held_sandboxes` | a hold built under `allow_all`, assignment becomes `deny_all_egress` → the lease gets a fresh sandbox; `test_warm_slot_serves_only_its_own_network` → a host or allowlisted request starts cold |
| 1.3 | integration (kernel lane) | `test_kernel_allow_all_and_deny_all` | host listener on a veth-less address: `allow_all` connects, `deny_all_egress` gets `ENETUNREACH` |
| 2.1 | unit | `test_egress_plan_merges_and_excludes_read_only` | `["a","b"]` + `["b","c"]` → `a,b,c`; same with `read_only` → `a,b`; empty registry → the default registry set, which an empty fleet list leaves as it is |
| 2.2 | unit | `test_egress_resolver_files_render` | `a→10.0.0.1` → hosts line `10.0.0.1 a`; `resolv.conf` has no `nameserver` |
| 2.3 | integration (kernel lane) | `test_kernel_allow_list_admits_only_the_set` | listener on an allowed address connects; non-listed, `169.254.169.254` and `:53` time out or are refused |
| 2.4 | integration (kernel lane) | `test_kernel_sandbox_cannot_flush_host_rules` | flush from inside → host table unchanged; the non-listed address still fails |
| 2.5 | unit | `test_egress_setup_failures_refuse_the_lease` | supervisor: unresolvable, v6-only and cap + 1 → three distinct reasons; sandbox: each netlink step refused → the refusal names the step, what was built is removed, the log counts hosts and names none |
| 2.6 | integration (kernel lane) | `test_egress_release_and_boot_sweep` | the table and link exist while the sandbox lives and are gone after it; a prefixed table and link left by a killed run → gone after the next engine's sweep; `test_the_sweep_removes_only_leftovers` (unit) skips a slot a live scope holds |
| 2.7 | unit + integration | `test_egress_probe_reports_enforcement` | probe success → `true`; injected failure → `false`; daemon reconcile over the report → healthy vs degraded |
| 2.8 | unit | `test_a_fleet_host_at_a_blocked_address_refuses_the_lease` | `test_v4_protocol_assignments_are_blocked_to_their_edges` → `192.0.0.0`–`192.0.0.255` and their v6 spellings blocked, `192.0.1.0` and `192.0.2.1` allowed; fleet hosts answering `169.254.169.254`, `10.1.2.3`, `100.101.102.103`, `127.0.0.1`, `::ffff:169.254.169.254` and a public address beside `10.1.2.3` → each `is_egress_blocked`, the text names the host and not the address; `test_a_registry_host_at_a_private_address_is_admitted` → a registry host at `10.1.2.3` is in the set; `a_fleet_host_at_a_blocked_address_ends_the_lease_at_startup` → the report's detail is the blocked-host sentence and no sandbox is built |
| 2.9 | unit + integration (kernel lane) | `test_kernel_host_cannot_delete_a_live_table` | a live scope's table, deleted by name from another socket → `EPERM`; a ruleset flush narrowed to its name → success and the table still listed; the sandbox still reaches the listed address and not the unlisted one; `test_the_table_is_owned_by_its_builder` (unit) → the new-table message carries `NFT_TABLE_F_OWNER` |
| 2.10 | unit + integration (kernel lane) | `test_egress_probe_refuses_a_dropping_forward_chain` | a namespace that forwards IPv4 and holds a forward chain with policy drop → probe reports no enforcement; the chain removed → enforcement; `test_the_probe_refuses_a_host_whose_forward_chain_drops` (unit) → the reason names `ip filter FORWARD` and `inet ufw forward`, no scope is built; `test_a_dropping_forward_chain_is_named_in_both_ipv4_families` and `test_chains_that_cannot_drop_the_sandboxs_forwarding_pass` cover the chain reading |
| 2.11 | unit | `test_a_second_holder_is_refused_until_the_first_lets_go` | two opens of one lock file → the second refused with "another runner process"; the first closed → the lock is free; `test_an_unopenable_lock_file_is_an_error` → a lock path under a file is an error |
| 2.12 | integration (kernel lane) | `test_kernel_allow_list_closes_dns_the_host_and_inbound` | UDP to the listed address's port 53 → no answer, to its own port → the greeting; a host listener the lane reaches → unreachable from the sandbox; a sandbox listener that reads its own greeting over loopback → the far host's connection fails and the listener counts 0 inbound |
| 2.13 | manual | `test_ci_measures_the_kernel_lane` | this Pull Request's `coverage (kernel)` job green, and `test-integration-rustd` graded over `lcov-kernel.info` with the shards' reports; run link pasted in Session Notes |
| 2.14 | unit + integration (kernel lane) | `test_kernel_allow_list_refills_in_place` | a live scope refilled with a new address → the sandbox reaches it and no longer reaches the old one; `test_a_refill_empties_the_set_then_fills_it_in_one_batch`, `test_a_refill_to_nothing_only_empties_the_set` and `test_a_refill_is_acknowledged_or_refused_whole` pin the batch; `an_allowlist_files_its_key_by_name_not_address` → two resolutions of one name share a hold key; `a_name_that_moved_keeps_the_hold_and_takes_its_new_address` → the hold is reused, refilled with the moved address before it thaws; `a_hold_that_refuses_its_new_address_falls_back_fresh` → `sandbox_refill_failed`, a fresh sandbox; `test_rewritten_names_land_in_the_file_the_sandbox_reads` → the bound inode holds the new names; `test_a_sandbox_built_to_no_allowlist_refuses_new_addresses` → `EgressRefusal::NoScope` |
| 3.1 | unit | `test_egress_pages_describe_enforcement` | `scripts/check_architecture_doc.sh` exit 0; grep for "not enforced" over `docs/architecture` → 0 |
| 3.2 | manual | `test_docs_branch_carries_egress_page` | docs Pull Request link with checks green, pasted in Session Notes |
| 1.4 | integration | `test_runner_binary_runs_a_lease_per_policy` | M213_001's fake-daemon lease, once per policy → one settled report each, no `run_refused` line |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | All three policies hold on a real kernel (§1, §2) | `make test-runner-kernel` | exit 0, `egress` cases listed as passed | P0 | |
| R2 | The probe is measured, not pinned (§2) | `grep -rn 'egress_enforcement: false' rustd/crates/afr_supervisor/src --include='*.rs' \| grep -v tests \| wc -l` | `0` | P0 | |
| R3 | No shell-out to `nft` or `ip` (§2) | `grep -rnE 'Command::new\("(nft\|ip)"\)' rustd/crates \| wc -l` | `0` | P0 | |
| R4 | Pages describe enforcement (§3) | `bash scripts/check_architecture_doc.sh && ! grep -rqi 'allowlist egress is not enforced' docs/architecture` | exit 0 | P0 | |
| R5 | Dev runner assigned `allow_list_egress` reads healthy and runs a lease that installs a package from the registry baseline (§2) | manual — `agentsfleet runners list` output and the thread link in Session Notes | healthy row, settled thread | P0 | |
| R6 | Diff stays inside Files Changed (this spec and M213_001 together) | `git diff --name-only origin/main...HEAD` | 0 paths missing from the two tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted by this spec; `src/runner/network/` leaves with M213_001's §2.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| Pinned enforcement | `grep -rn 'egress_enforcement: false' rustd/crates/afr_supervisor/src --include='*.rs' \| grep -v tests` | 0 matches |
| The non-goal | `grep -n 'No runner crate builds the per-lease network allowlist' docs/architecture/runner_execution.md` | 0 matches |

## Out of Scope

- The name layer for rotating CDN addresses (DNS-answer snooping into the same set), per `runner_fleet.md` §Egress model — a later spec.
- IPv6 egress — refused, as the Zig design refused it.
- New API fields or a new allowlist surface — both knobs exist (Interfaces).
- Closing the allow-listed write-capable host as an exfiltration channel — a credential-model change.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator assigns `allow_list_egress` with `registry.npmjs.org` in the registry list; the runner shows healthy, a fleet's `npm install` works, and a `curl` to any other host inside the same sandbox fails.
2. **Preserved user behaviour** — Runners on `allow_all` keep reaching the network as they do on Zig; runners on `deny_all_egress` keep reaching nothing; both API calls keep their shapes.
3. **Optimal-way check** — Kernel rules at the host side of a veth are the narrowest enforcement that needs no proxy and no resolver; the gap to the optimum is the name layer, acceptable while allowlists are small and stable.
4. **Rebuild-vs-iterate** — Port of a finished design that was never switched on; no redesign.
5. **What we build** — The policy flag, the scope, the probe, the sweep, the two page changes.
6. **What we do NOT build** — See Out of Scope.
7. **Fit with existing features** — Composes with `afr_egress`, which still owns every credentialed call and every L7 rule; must not destabilise warm slots.
8. **Surface order** — API already exists; runner first, published page in the same window.
9. **Dashboard restraint** — N/A — no dashboard change; the runner list already shows degraded and its reason.
10. **Confused-user next step** — A degraded runner names `REASON_EGRESS_ENFORCEMENT_UNAVAILABLE`; the runner page names the kernel requirement; a refused lease's host is named in the runner's log, and the event carries the fixed egress sentence.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a separate spec in M213_001's Pull Request — the cutover would regress `allow_all` without §1, and the Zig reference leaves in the same diff.
- **Alternatives considered:** folding it into M213_001 (Indy's first answer, superseded the same day by "create a new spec"); a later Pull Request after the cutover (rejected: `allow_all` hosts would lose the network between the two).
- **Patch-vs-refactor verdict:** this is a **patch** because the API, wire and daemon already carry the policy; only the runner's enforcement is missing.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 06, 2026): "I think the allowlist also is managed via the api, just create a new spec and make it depend on M213 and they must ship in 1 PR". Source-verified: `PATCH /v1/fleets/runners/{runner_id}` takes `assigned_policy` (`public/openapi.json`); `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` takes `trigger_markdown` and `config_json`, whose `network` block `afd_gate/src/policy/shape.rs:18` reads. Agent choices recorded for review: `read_only` fleet hosts stay out of the kernel set; the inference endpoint leaves the merged set.
- **Port divergences from the Zig reference (Oct 08, 2026, agent)** — (1) the Zig per-worker tables each carried a `policy drop` forward chain, which drops every other worker's and the host's forwarded traffic; here every chain accepts by policy and every drop names the slot's link. (2) Return traffic is admitted by connection state, not by source address, so an allow-listed server cannot open a connection inward and path-MTU errors still arrive. (3) A scope is built and removed with its sandbox; a held sandbox's set is emptied and refilled in place on resume (Dimension 2.14). (4) The probe builds its scope in namespaces of its own, so `agentsfleet-runner probe` beside a live runner neither meets nor removes its scopes; only the engine's boot sweep touches the host namespace. (5) Slots are claimed from a process-wide bitmap, so two engines in one process never share a slot's names.
- **Internal addresses (decided)** — the question: an entry in a fleet's `network.allow` resolved to a link-local (`169.254.169.254`) or private address was admitted as resolved, as the Zig reference did (`AllowList.zig` admitted `my.private.registry`). Indy (in-session, Oct 08, 2026): "Okay go, what are the internal addressed you will add, are these static?" Built as Dimension 2.8: fleet-supplied hosts are held to `afd_core::net::is_blocked`, the compiled-in table the daemon's endpoint check and `http_request` already share, so the ranges change only with the code; the addresses are resolved per lease. Registry hosts are exempt because the operator sets them. `192.0.0.0/24`, first a reviewer's suggestion: Indy (in-session, Oct 08, 2026) chose "Add it (Recommended)" for fleet hosts, `http_request` and the daemon's endpoint check alike.
- **Host-level egress gaps (decided)** — the review found that a host `nft flush ruleset` deleted every live table, that the probe passed a host whose own forward chain drops, and that a second runner process swept the first one's scopes. Indy (in-session, Oct 08, 2026) chose "Fix all three (Recommended)": built as Dimensions 2.9–2.11. The kernel-level trials the same review asked for (UDP to port 53, the host's own listener, an inbound connection) are Dimension 2.12. The kernel lane runs in CI: asked whether it needs a nested virtual machine, Indy (Oct 08) read the answer that a hosted Ubuntu runner is one, with root, and chose "Add the job (Recommended)" (Dimension 2.13).
- **Hold refresh and the egress event (decided)** — the review found that a held allowlisted sandbox was discarded whenever DNS answered a rotated address, since its hold key carried the addresses, and that the supervisor's refused bind and the sandbox's refused build both logged `egress_scope_refused`. Indy (in-session, Oct 08, 2026) chose "Refresh in place (Recommended)", built as Dimension 2.14, and "Split the egress event": the supervisor's event is `egress_bind_refused`.
- **Metrics review** — No analytics or funnel playbook update required; the signals are an operator log line and an existing report field.
- **Skill-chain outcomes** — one PR with M213_001, so one chain: M213_001 Discovery, Skill-chain outcomes, records the unit-test audit, the kernel lane standing in for the integration test, and the review dispositions.
- **Deferrals** — none.
