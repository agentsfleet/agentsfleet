# Prompt — author the execution-substrate abstraction spec

Ephemeral. Paste the block below to a fresh agent. Delete once the spec exists.

---

You are authoring a milestone spec for the `agentsfleet` repository. Use the
`orly-spec-new` skill — it owns the authoring order and the template the SPEC
TEMPLATE GATE enforces (`audits/spec-template.sh --staged`). Read
`AGENTS.orly.md` and `CLAUDE.md` first; they carry the safety rules, the
dispatch router, and the lifecycle this repository gates on.

**Do not write code. The deliverable is one spec file, nothing else.**

## What the milestone is

Make the execution substrate pluggable. Today the control plane can only
describe a Linux host running `bubblewrap`. It must be able to describe a host
running Firecracker microVMs, a host that *is* a virtual machine, and — with a
caveat below that is the hardest part of this spec — serverless execution on
Cloudflare Workers / Durable Objects.

Working title: `M182_001` — confirm the number against `docs/v2/pending/` and
`docs/v2/done/`, and against whatever Indy says, before you commit to it.

## Where to start reading, in this order

1. `docs/architecture/runner_fleet.md` — the lease lifecycle, `LEASE_TTL_MS` /
   `MAX_RUNTIME_MS`, money gates, reclaim bounds, and the runner state model.
   This is the document the milestone is really about.
2. `docs/architecture/data_flow.md` §C. EXECUTE — the twelve hot-path writes.
3. `rustd/crates/afd_wire/src/runner.rs` — the frozen wire types. Read
   `SandboxTier`, `NetworkPolicy`, `CapabilityReport`, `AssignedPolicy`,
   `SelftestReport`. **This file is the thing the milestone changes.**
4. `rustd/crates/afd_fleet/src/runner/reconcile.rs` — where the verdict is
   decided. Already refactored to speak in `Guarantee`s; read the module
   documentation, it explains the seam this milestone widens.
5. `rustd/crates/afd_fleet/src/runner/policy.rs` and `bounds.rs` — the other two
   files that name Linux mechanisms.
6. `src/runner/` (Zig) — the runner side. This is where a new substrate driver
   would live, and you must scope how much of it this milestone owns.
7. `docs/v2/active/M177_001_P0_API_RUNNER_CONTROL_PLANE_PARITY.md` — its
   Discovery section carries a "Substrate coupling localised at EXECUTE" entry
   written specifically to hand this milestone its starting point.

## The state of the world you are specifying against

`agentsfleetd` is being ported from Zig to Rust across M175–M181:

```
M175  Rust workspace scaffold, wire types frozen        done
M176  daemon substrate — boot, pools, auth primitives   done
M177  runner control plane parity                       IN PROGRESS (§1 done)
M178  tenant / workspace surface                        pending
M179  admin / operator surface                          pending
M180  signed ingress, cron, connectors                  pending
M181  cutover + soak — the Zig daemon is retired here   pending
```

Until M181 there are **two daemons**. A wire change before it means changing
both, plus the runner. That is the single most important sequencing fact in
this spec, and the reason the recommendation below is what it is.

## The coupling, precisely

Exactly four files in `rustd/` name a Linux mechanism:

```
rustd/crates/afd_wire/src/runner.rs              ← the wire (frozen by M175)
rustd/crates/afd_fleet/src/runner/reconcile.rs   ← the verdict
rustd/crates/afd_fleet/src/runner/policy.rs      ← row → wire decoding
rustd/crates/afd_fleet/src/runner/bounds.rs      ← what a host may assert
```

The shape is already right — policy flows **down**, capability flows **up**, and
`reconcile` is a pure function over `(assigned, achievable)`. What is wrong is
that both vocabularies are Linux-specific, and that the daemon *second-guesses
the substrate*: it checks for `cpu`/`memory`/`pids` cgroup controllers and a
`bubblewrap` binary by name.

The direction, already half-taken in `reconcile.rs`:

| today (mechanism) | outcome (guarantee) | bubblewrap | Firecracker | VM | CF Worker |
|---|---|---|---|---|---|
| `landlock` | filesystem isolation | landlock | guest rootfs | guest disk | platform |
| `seccomp` | syscall filtering | seccomp | vmm + guest | guest | platform |
| `cgroup_controllers` | resource limits | cgroup v2 | vCPU/mem caps | hypervisor | platform quota |
| `bubblewrap` | process containment | bwrap | microVM | VM | isolate |
| `egress_enforcement` | egress control | veth + nft | tap + nft | vSwitch | outbound rules |

`afd_fleet::runner::reconcile::Guarantee` already exists with those five
variants, and `Guarantee::proven_by` is the **one** substrate-aware function in
the crate — it maps a guarantee onto today's Linux booleans. When the wire
carries guarantees directly, `proven_by` is deleted. **That refactor is already
landed, at zero wire and zero behaviour cost.** Your spec starts from it.

The wire-level shape to evaluate (this is a proposal, not a decision):

- `SandboxTier` → an **isolation class**: what the tenant is promised, not how
  it is built. Candidate vocabulary: `none | process | kernel | machine |
  managed`.
- `CapabilityReport` → `{ substrate, guarantees, mechanisms }`, where
  `mechanisms` is diagnostic prose an operator reads and nothing branches on.
- `reconcile` → `required_guarantees(class) ⊆ reported_guarantees`, naming the
  first missing **guarantee**.

## The hard part — read this twice

`SandboxTier` is not the real problem. **The lease model is.**

```
 bubblewrap    runner process on a host, forks sandboxed children   fits today
 Firecracker   runner on a host, spawns microVMs                    fits today
 VM            runner in / managing a VM                            fits today
 CF Workers /  DOES NOT FIT — there is no long-lived host to enrol,
 Durable Obj   no agt_r on disk, no heartbeat, nothing to renew
```

The entire runner protocol assumes a process that **stays alive and renews**:

```
   register ──► heartbeat ──► lease ──► activity ──► renew ──► report
                  every 10s              fencing_seq, 30s TTL
```

A Worker is invoked, runs, and dies. So Cloudflare is not a `SandboxTier`
variant. It is one of two things, and **deciding which is the central question
this spec exists to answer**:

```
(a) BROKER
    daemon ──lease──► broker runner ──dispatch──► Worker
                          ▲                          │
                          └────────── result ────────┘
                          │
                          └──report──► daemon

    + zero wire change to the runner plane
    + fencing tokens, TTL, reclaim, billing all unchanged
    + the substrate becomes invisible to the daemon
    − the broker is an availability dependency and a new failure domain
    − a Worker's own crash is observable only through the broker

(b) PUSH PLANE
    daemon ──POST work──► Worker endpoint
              ▲                  │
              └──── callback ────┘

    + genuinely serverless; no host to operate
    − a new control protocol: no heartbeat, no renew, no lease TTL
    − fencing has to be re-derived (what stops a duplicate delivery
      double-charging, and what supersedes a slow Worker?)
    − new billing hooks: the twelve hot-path writes assume a lease row
    − at-most-once vs at-least-once becomes a product decision, not a detail
```

Whichever you specify, the spec must state what happens to: `fencing_seq`
monotonicity, the at-most-one-lease-per-fleet invariant, `MAX_RUNTIME_MS`, the
reclaim sweep, and the two billing debit points. Those are M177's Invariants 1–3
and they do not get to become vague.

## The recommendation you are starting from (challenge it if you disagree)

**Land this as M182, immediately after M181 cutover, and take the broker shape
for Cloudflare first.**

- A wire change is cheapest when there is exactly one daemon to move. Before
  M181 there are two, plus a runner.
- The `Guarantee` seam is already in place, so nothing is blocked by waiting.
- The broker gets Workers executing against today's protocol with no new
  fencing story; the push plane can be specced separately once real usage shows
  whether the broker's single point of failure actually bites.

If your reading of `runner_fleet.md` says otherwise, say so in the spec's
**Decomposition & alternatives** section with the reasoning — that section
exists for exactly this.

## Open questions to resolve with Indy before finalising

These were raised and are **unanswered**. Do not invent answers; put them to
Indy and record the verbatim response in the spec's Discovery section (the
format is `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <item>`).

1. **Cloudflare: broker or push plane?** (a), (b), both-brokered-first, or park
   Cloudflare entirely and spec Firecracker/VM only.
2. **Sequencing:** M182 after cutover, or earlier?
3. **Is the isolation class a tenant-visible promise?** Can a tenant *require*
   `machine` isolation for billing or compliance reasons, or are they only
   *told* which class they got? This decides whether the class reaches the
   tenant API and the pricing model, or stays operator-facing.
4. **Is substrate a placement input or a reported fact?** Does an operator
   assign a runner its substrate, or does the runner report it and the daemon
   place work by class? This decides whether `AssignedPolicy` grows a substrate
   field or only `CapabilityReport` does.
5. **How much of the runner side does this milestone own?** A Firecracker driver
   in `src/runner/` is a large piece of work with its own risks (image supply,
   boot latency, networking). The spec must either scope it in explicitly or
   name the follow-up milestone that owns it.

## Constraints the spec must respect

- **No time, effort, hour, or day estimates anywhere.** Priority (P0–P3) is the
  only sizing signal; Dependencies are the only sequencing signal. The template
  gate enforces this.
- **Error codes:** `UZ-*` codes are single-sourced in the Zig registry. A new
  code fires the ERROR REGISTRY gate. If the design needs one, say so
  explicitly and name the registry edit.
- **Schema:** if row shapes change, the SCHEMA GUARD gate fires and the spec
  needs a migration story. Prefer additive columns.
- **Rust discipline:** `dispatch/write_rust.md` is mandatory reading if the spec
  prescribes any Rust shape — one error type per crate with a `Result` alias,
  `#[from]` composition, no `map_err(|e| Mine(e.to_string()))`, parse-don't-
  validate, illegal states unrepresentable. The M177 port has worked examples of
  all of it.
- **File and function length:** ≤350 lines per file, ≤50/≤70 per function.
- **Docs:** public endpoint, command, flag, or behaviour changes require a
  matching branch in `~/Projects/docs`; never edit that repository through this
  worktree.

## What good looks like

A reviewer should be able to read the spec and answer, without opening any
source: what a tenant is promised; what a runner reports; what refuses a lease
and with which code; what happens to a lease when its substrate dies; which of
the twelve hot-path writes change; and what an operator does differently on the
day it ships. If the Cloudflare question is deferred rather than answered, the
spec must say so in **Out of Scope** with the reason and the milestone that
picks it up — a deferral with an Indy-acked quote, not a silence.
