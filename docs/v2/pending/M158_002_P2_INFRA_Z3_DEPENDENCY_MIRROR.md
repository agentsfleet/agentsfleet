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

# M158_002: Every Zig build dependency resolves from GitHub, and a gate keeps it that way

**Prototype:** v2.0.0
**Milestone:** M158
**Workstream:** 002
**Date:** Aug 06, 2026
**Status:** PENDING
**Priority:** P2 — tooling reliability; no user-facing surface changes, but a single third-party host can currently red every Zig lane at once.
**Categories:** INFRA
**Batch:** B1 — standalone; no sibling workstream shares these files.
**Branch:** feat/m158-z3-mirror
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** None
**Provenance:** LLM-drafted (Claude Opus 5, Aug 06, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Storage map — R2 is the sole bundle content store, and `z3` is the client that reaches it.

---

## Overview

**Goal (testable):** No entry in `build.zig.zon` resolves from a host other than `github.com`, the `z3` dependency is pinned to the current upstream head through the `agentsfleet/z3` mirror, and a governance gate fails the build if a non-GitHub dependency host is ever reintroduced.

**Problem:** Every Zig lane fetches `z3` from `codeberg.org`. When that host resets a connection, all of them go red at once with `error: HTTP response read failure: ConnectionResetByPeer` at `build.zig.zon:70` — unit tests, integration, cross-compile, memory-leak, and the dev deploy's compile step. On the merge of Pull Request (PR) #589 this took out `test`, `test-integration`, and `deploy (dev)` simultaneously, while `cross-compile` passed on the same commit against the same tarball — proving the failure is host-side, not content-side. The observable cost is that a red board says nothing about the change under test, and the standing remedy is "re-run and hope".

**Solution summary:** Mirror the upstream `z3` repository into the organisation at `github.com/agentsfleet/z3`, then repoint `build.zig.zon` at the mirror. The host change is provably inert: the mirrored archive of the currently pinned commit hashes to the byte-identical Zig package digest, so the `.hash` line does not move and Zig itself rejects any future drift. Separately, advance the pin to the current upstream head, which is one commit newer. Finally, add a `lint-governance` sub-gate asserting every `.url` in the manifest points at `github.com`, so this cannot silently regress.

## PR Intent & comprehension handshake

- **PR title (eventual):** Mirror the z3 build dependency onto GitHub and gate the host
- **Intent (one sentence):** A third-party host outage should never be able to red the entire Zig board, so every build dependency now resolves from GitHub and a gate keeps it that way.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

**Handshake (filled at PLAN):** The point is not "make the flake go away" — GitHub can reset a connection too. The point is to stop **one** host from being a shared single point of failure for lanes that are otherwise independent, and to consolidate onto the host every other dependency already uses and that the runners sit adjacent to.

`ASSUMPTIONS I'M MAKING:`
1. A frozen mirror is intended, not a tracking one. `agentsfleet/z3` is a point-in-time copy; picking up future upstream work means re-pushing the mirror and re-pinning the Secure Hash Algorithm 1 (SHA-1) commit identifier deliberately.
2. Upstream `codeberg.org/fellowtraveler/z3` stays authoritative. The mirror is not a fork — no patches land on it. This differs from `agentsfleet/nullclaw`, `agentsfleet/pg.zig`, `agentsfleet/http.zig`, and `agentsfleet/zig-yaml`, which all carry fork patches; the manifest comment must say so, or a later reader will assume the mirror is patched.
3. The Massachusetts Institute of Technology (MIT) licence permits redistribution provided the `LICENSE` file travels with the copy. A `git clone --mirror` preserves it verbatim.
4. Advancing the pin is a genuine dependency upgrade and is therefore graded separately from the host swap, in its own commit, so a red lane is attributable to one or the other.

## Implementing agent — read these first

1. `build.zig.zon` — the manifest's own convention: every pinned or forked dependency carries a comment saying **why** it is pinned and **when** the pin may be dropped. `z3` is the only entry with no comment, and after this workstream it needs one that distinguishes a mirror from a fork.
2. `make/quality.mk` §`_legacy_symbols_check` / `_legacy_noun_check` / `_runner_isolation_check` — the exact shape a governance sub-gate takes here: a `_`-prefixed target, a grep with comment lines filtered out, a `✗` message naming the rule and the offending lines, and membership in the `lint-governance` prerequisite list. §3 mirrors this shape rather than inventing one.
3. `src/lib/s3/r2.zig` — the only production consumer of `z3`. It uses `S3Client.init`, `putObject`, `getObject`, and `deinit` and nothing else; this bounds the blast radius of the version bump.
4. `docs/architecture/fleet_bundles.md` §Storage map — establishes R2 as the sole bundle content store, i.e. what `z3` is load-bearing for.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig.zon` | EDIT | The `z3` entry's `.url` moves to the GitHub mirror, the `.hash` advances with the version bump, and the entry gains the why-and-when-to-drop comment every other pinned dependency already carries. |
| `make/quality.mk` | EDIT | Adds `_dependency_host_check` and lists it in `lint-governance`, turning "dependencies resolve from GitHub" from a convention into a build failure. |
| `docs/v2/pending/M158_002_P2_INFRA_Z3_DEPENDENCY_MIRROR.md` | CREATE | This spec; moves to `active/` at CHORE(open) and `done/` at CHORE(close). |

External to the repository, and therefore not a diff row: the `github.com/agentsfleet/z3` mirror repository, created public from a `git clone --mirror` of upstream with branches and tags pushed and the reserved `refs/pull/*` refs omitted.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **UFS** (the two commit SHA-1 identifiers and the allowed host string appear in the manifest and the gate; each occurs once per site, so no named constant is warranted — the gate's host pattern is the single definition of "allowed host"); **NDC** (no dead code — the gate must be reachable from `lint-governance`, not merely defined); **NRC** (the manifest comment states why the mirror exists and when it may be dropped, not what the two lines below it say); **ORP** (no orphan sweep needed — nothing is renamed or deleted); **XCC** (cross-compile before commit, because the dependency graph the daemon links against changes).
- `dispatch/write_shell.md` — the `_dependency_host_check` recipe is shell: quoted expansions, no untrusted `eval`, and the `$$` escaping the surrounding Makefile requires.
- `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` §RULE GLS — a gate dispatcher must not glob itself; the host check reads `build.zig.zon` explicitly rather than globbing manifests.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — the dependency graph the Zig build links against changes | Cross-compile both Linux targets (`x86_64-linux`, `aarch64-linux`) plus the host build; the pinned `.hash` is the content backstop that turns a wrong pin into a build failure rather than a silent swap. |
| PUB / Struct-Shape | no | No Zig source is added or changed; no new public surface. |
| File & Function Length (≤350/≤50/≤70) | no | `build.zig.zon` grows by a comment block; `make/quality.mk` gains one recipe. Neither file approaches the cap. |
| UFS (repeated/semantic literals) | yes | Each commit identifier appears exactly once, in the manifest entry it pins. The allowed-host string is defined once, inside the gate that enforces it. |
| UI Substitution / DESIGN TOKEN | no | No TypeScript, no components, no styles. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | No runtime code path, no allocator wiring, no error codes, no schema. The gate's output is build-time `echo`, not application logging. |

## Prior-Art / Reference Implementations

- **Reference:** `agentsfleet/zig-yaml`, pinned at `build.zig.zon` — the organisation already resolves a third-party Zig dependency from a copy under its own account rather than from the upstream host. This workstream applies that same shape to `z3`, with one deliberate divergence: `zig-yaml` is a **fork carrying patches**, whereas `agentsfleet/z3` is an **unmodified mirror**. The manifest comment must state the difference so a later reader does not go hunting for fork patches that do not exist.
- **Reference:** `make/quality.mk` §`_runner_isolation_check` — the closest existing gate in kind: it asserts a property of the *build graph* (the runner depends only on `nullclaw`) rather than of application code, using a grep over build files with comment lines filtered. §3 mirrors its structure, message style, and `lint-governance` membership.

## Sections (implementation slices)

### §1 — The host swap, proved inert

Repoint the `z3` entry at the `agentsfleet/z3` mirror **without changing which bytes are compiled**. This slice exists on its own so that the "no longer depends on codeberg" outcome is verifiable in isolation: the mirrored archive of the currently pinned commit hashes to the identical Zig package digest, so `.hash` does not move, and a green build is itself the proof that the mirror is faithful.

**Implementation default:** the archive-tarball Uniform Resource Locator (URL) form `https://github.com/agentsfleet/z3/archive/{commit}.tar.gz` rather than the `git+https://…#{commit}` form used by the fork pins, because Zig hashes the extracted tree — so the archive form reproduces the existing digest exactly, while the `git+` form would change it and forfeit the proof.

- **Dimension 1.1** — `build.zig.zon` contains no `codeberg.org` reference. → Verified by `test_no_codeberg_reference`
- **Dimension 1.2** — The mirrored archive of commit `4553a640` hashes to the digest already pinned, `z3-0.5.0-N25-cBA7AgAS6j3pBZYNnK0NAFgm_hpNQn4odoFjbcRS`. → Verified by `test_mirror_hash_matches_pinned`
- **Dimension 1.3** — The `z3` entry carries a comment explaining that it is an unmodified mirror (not a fork), naming upstream, and stating when the mirror may be dropped. → Verified by `test_z3_entry_documented`

### §2 — The version bump, graded separately

Advance the pin from `4553a640` to the current upstream head `7f64763`, the single commit ahead of it. Upstream publishes no tags or releases, so a commit identifier is the only pinnable unit.

This is a real upgrade, not a host change, and it is deliberately a **separate commit** so an attributable bisect exists. The upstream commit changes ownership semantics in the `z3` result types: `parseHeadObject` moves from taking `*S3Client.Response` to taking it by value and owning it, result structs gain a retained `http_response` field, and several header fields stop being duplicated in favour of aliasing into that retained response until `deinit`. None of those types are on the path this repository uses — `r2.zig` calls only `putObject`, `getObject`, `init`, and `deinit` — and the two changes that do reach us are benign: `RequestError` gains variants (the call sites use a blanket `catch`, so a wider error set cannot break them) and `getAmzValue` takes its receiver by value (never called here). That reasoning is what makes the bump *plausible*; the build and the memory-leak lane are what make it *true*.

- **Dimension 2.1** — The daemon, runner, and library graphs build against the bumped dependency. → Verified by `test_build_against_bumped_z3`
- **Dimension 2.2** — Both Linux cross-compile targets succeed against the bumped dependency. → Verified by `test_cross_compile_both_targets`
- **Dimension 2.3** — The R2 client's own behaviour is unchanged, including its idle-connection-reuse setting, and leaks no memory across the graphs that link it. → Verified by `test_r2_behaviour_unchanged` and `test_no_leaks_after_bump`

### §3 — The gate that keeps the host from coming back

Add `_dependency_host_check` to `make/quality.mk` and list it in `lint-governance`. It reads `build.zig.zon`, extracts every `.url` value, ignores commented lines, and fails naming the offending entries if any host other than `github.com` appears.

Without this slice §1 is a one-time cleanup that the next dependency addition can silently undo. With it, "every dependency resolves from GitHub" is an invariant the build enforces, which is what the Invariants section below requires — enforcement by code, never by review discipline.

**Implementation default:** allow `github.com` only, rather than an allowlist of several hosts, because the manifest today has exactly one host and a single-host rule is the strictest statement of the invariant. Widening it later is a deliberate edit to the gate, which is the point.

- **Dimension 3.1** — `lint-governance` runs `_dependency_host_check`, and it passes on the repointed manifest. → Verified by `test_host_gate_passes_clean_tree`
- **Dimension 3.2** — The gate **fails**, naming the offending line, when a non-GitHub dependency host is present. → Verified by `test_host_gate_rejects_foreign_host`
- **Dimension 3.3** — The gate does not fire on a commented-out or prose mention of another host. → Verified by `test_host_gate_ignores_comments`

## Interfaces

```
build.zig.zon — .dependencies.z3 (the pinned entry; shape unchanged, values move)
    .url  : "https://github.com/agentsfleet/z3/archive/{commit}.tar.gz"
    .hash : "z3-0.5.0-…"        # Zig-computed digest of the extracted tree

make/quality.mk
    _dependency_host_check      # no arguments; reads build.zig.zon from the repo root
                                # exit 0 → every .url host is github.com
                                # exit 1 → prints "✗ …" plus one line per offending entry
    lint-governance: … _dependency_host_check      # gains the new prerequisite
```

The `z3` module import name (`@import("z3")`) and the `S3Client` surface `r2.zig` consumes are **not** changed by this workstream and may not be changed without amending this spec.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Mirror drift | The mirror repository is force-pushed, rewritten, or repointed at different content | Zig compares the extracted tree against the pinned `.hash` and refuses to build; the operator sees a hash-mismatch error naming the dependency, not a silent content swap. |
| Mirror deleted or made private | The repository is removed or its visibility flips | The fetch fails with a `404` at the manifest line, naming the URL. Recovery is documented in the manifest comment: upstream remains authoritative and the mirror is reconstructible by `git clone --mirror` from it. |
| Foreign host reintroduced | A future dependency is added pointing at a non-GitHub host | `_dependency_host_check` fails `lint-governance`, and therefore `lint-zig` and the `lint` Continuous Integration (CI) lane, naming the offending line. |
| Gate defined but unreachable | `_dependency_host_check` is added but omitted from `lint-governance` | Caught by `test_host_gate_rejects_foreign_host`, which invokes the umbrella target rather than the sub-target, so a gate wired nowhere cannot pass this workstream. |
| Bumped dependency breaks a consumer | The upstream head changes an Application Programming Interface (API) `r2.zig` actually uses | The build fails at the call site during §2, whose commit is separate from §1 — so the host change stays landed and only the bump is reverted or fixed. |
| Upstream host outage during a fetch | `codeberg.org` is unreachable while the mirror is being refreshed | Affects only a deliberate mirror refresh performed by a human, never a build: after §1 no build path contacts a non-GitHub host. |

## Invariants

1. **Every dependency in `build.zig.zon` resolves from `github.com`.** — Enforced by `_dependency_host_check`, a prerequisite of `lint-governance` → `lint-zig` → the `lint` CI lane. Not review discipline: a foreign host fails the build.
2. **The mirror's content equals what was reviewed.** — Enforced by Zig's package digest: `.hash` is compared against the extracted tree on every fetch, so drift is a build failure, not a quiet substitution.
3. **The pinned commit is immutable.** — Enforced by pinning a full SHA-1 commit identifier rather than a branch or tag, so the mirror's `main` moving cannot change what is compiled.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | Build-time dependency resolution only; no runtime code path, user action, or operator surface is added, renamed, or removed | not applicable | not applicable | not applicable |

Metrics review: no analytics or funnel playbook update required — this workstream adds no runtime code and therefore no event can be emitted from it.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_no_codeberg_reference` | Grepping `build.zig.zon` for `codeberg.org` returns zero matches. |
| 1.2 | unit | `test_mirror_hash_matches_pinned` | `zig fetch` against the mirror archive of `4553a640` prints exactly the digest already pinned in the manifest — the host moved, the bytes did not. |
| 1.3 | unit | `test_z3_entry_documented` | The lines preceding the `z3` entry mention the upstream URL and the word `mirror`, distinguishing it from the fork pins above it. |
| 2.1 | integration | `test_build_against_bumped_z3` | `make build` (daemon, runner, library graphs) exits 0 with the bumped `.hash` in place. |
| 2.2 | integration | `test_cross_compile_both_targets` | `zig build -Dtarget=x86_64-linux` and `-Dtarget=aarch64-linux` each exit 0. |
| 2.3 | unit | `test_r2_behaviour_unchanged` | The pre-existing `"R2 disables idle HTTP connection reuse"` test still passes against the bumped dependency — the connection pool's free size still equals `R2_IDLE_CONNECTION_LIMIT`. Regression row: this behaviour predates the workstream and must not change. |
| 2.3 | integration | `test_no_leaks_after_bump` | `make memleak` exits 0, covering the retained-response ownership change in the bumped dependency across the graphs that link it. |
| 3.1 | unit | `test_host_gate_passes_clean_tree` | `make lint-governance` exits 0 on the repointed manifest and its output contains the host-check success line. |
| 3.2 | unit | `test_host_gate_rejects_foreign_host` | Negative path: with a non-GitHub `.url` planted in `build.zig.zon`, `make lint-governance` exits non-zero and its output names the planted line. The manifest is restored afterwards and `git diff --quiet build.zig.zon` confirms it. |
| 3.3 | unit | `test_host_gate_ignores_comments` | Negative-control path: with a non-GitHub host present only inside a `//` comment, `make lint-governance` exits 0 — the gate matches dependency entries, not prose. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No build dependency resolves from a non-GitHub host (§1) | `grep -c 'codeberg\.org' build.zig.zon \|\| true` | `0` | P0 | |
| R2 | The mirror is byte-faithful at the previously pinned commit (§1) | `zig fetch https://github.com/agentsfleet/z3/archive/4553a640ec867ab0355a97e5513ce4ec69a90d49.tar.gz` | `z3-0.5.0-N25-cBA7AgAS6j3pBZYNnK0NAFgm_hpNQn4odoFjbcRS` | P0 | |
| R3 | The pin sits at the current upstream head (§2) | `grep -c '7f64763e186ebe348989ae229b7551cb6ec79ee0' build.zig.zon` | `1` | P1 | |
| R4 | A foreign dependency host fails the build (§3, negative) | `sed -i.bak 's\|https://github.com/agentsfleet/z3/archive\|https://codeberg.org/fellowtraveler/z3/archive\|' build.zig.zon && make lint-governance; echo "exit=$?"; mv build.zig.zon.bak build.zig.zon` | `exit=2` (non-zero), output names the offending line | P0 | |
| R5 | The manifest is unmodified after the negative test (§3) | `git diff --quiet build.zig.zon; echo "exit=$?"` | `exit=0` | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks (dependency ownership semantics changed) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig dependency graph touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `codeberg.org` (as a dependency host) | `grep -rn "codeberg\.org" build.zig.zon` | 0 matches |

The string survives deliberately in the `pg` entry's comment, which narrates a past mis-pin — prose about history, not a resolvable host. The gate must not fire on it, which is exactly Dimension 3.3.

## Out of Scope

- **Mirroring the remaining dependencies.** Every other entry already resolves from `github.com`; there is nothing to move. The gate added here is what covers the next one.
- **Tracking upstream `z3` automatically.** No scheduled job, no bot. Refreshing the mirror and advancing the pin stays a deliberate human-initiated act, because the pin is what makes builds reproducible.
- **Caching Zig packages in Continuous Integration (CI).** A warm package cache would blunt fetch flakes generally, across all hosts. It is a larger change to every Zig lane's workflow and is a candidate follow-up, not this workstream.
- **Retry logic around dependency fetches.** Zig owns the fetch; wrapping the build in a retry loop would mask real failures alongside transient ones.
- **Upstreaming anything to `fellowtraveler/z3`.** The mirror carries no patches, so there is nothing to send.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A lane goes red, and the engineer reading it knows immediately that it is about their change. Today the same red board might mean nothing more than a third-party host hiccuped, and the first move is a re-run rather than a diagnosis.
2. **Preserved user behaviour** — Every build, test, and deploy command keeps working exactly as before, and the compiled artifact is unchanged by §1. Bundle upload and download through R2 behave identically; a change there would be a redesign, not this.
3. **Optimal-way check** — The unconstrained-optimal shape is that no build ever reaches the network, via a vendored or fully cached dependency set. The gap accepted here is that builds still fetch — just from one host, the same one the runners already sit next to, and the one every other dependency uses. Vendoring was rejected under item 6.
4. **Rebuild-vs-iterate** — Iterate. Nothing about the dependency wiring is wrong in shape; one entry points at a host that is a shared single point of failure. Determinism actually improves, since the pinned digest is unchanged by §1 and enforced by Zig on every fetch.
5. **What we build** — A public mirror repository; a repointed manifest entry with the comment the file's convention requires; a governance sub-gate; a version bump.
6. **What we do NOT build** — Vendoring the source into the tree (adds third-party code to review and makes upstream updates manual diffs); a private mirror (would need fetch credentials plumbed into five-plus workflow surfaces, and Zig's package fetch has no clean credential hook); an automatic upstream-tracking job (defeats the reproducibility the pin exists for); CI package caching (a broader change, listed as a follow-up in Out of Scope).
7. **Fit with existing features** — Compounds with the fork-pin convention already in the manifest, and with `lint-governance`, which is where build-graph invariants already live. The feature it must not destabilize is bundle storage: `z3` is the client behind R2, the sole content store for fleet bundles per `docs/architecture/fleet_bundles.md`, which is why §2 is graded by the memory-leak lane and not by inspection.
8. **Surface order** — N/A — no user surface. Build configuration and a lint gate only.
9. **Dashboard restraint** — N/A — no user surface; nothing is displayed.
10. **Confused-user next step** — Self-serve by construction: a fetch failure names the URL and the manifest line, and the gate's failure message names the offending entry and the rule. The manifest comment states that upstream remains authoritative and how the mirror is reconstructed, so an engineer who finds the mirror missing has the recovery path in the file they are already reading.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Three Sections split by what can fail independently, not by file. §1 changes the host and proves nothing else moved; §2 changes the content and is graded by build plus the memory-leak lane; §3 makes the property permanent. Because §1 and §2 land as separate commits, a red lane is attributable by bisect to either "the mirror is wrong" or "the upgrade is wrong" — a distinction that is lost if the manifest is edited once.
- **Alternatives considered:** (a) *Vendor `z3` into the tree* — removes every external host, but takes ownership of third-party source and turns upstream updates into manual diffs; rejected as disproportionate to a fetch flake. (b) *Do only the host swap, skip the bump* — the smallest possible change and genuinely sufficient for the stated problem; rejected because the head is one commit ahead with no consumer-visible break, and re-doing the mirror refresh later costs more than carrying it now. (c) *Cache Zig packages in CI* — addresses fetch flakiness across all hosts rather than one, but touches every Zig lane's workflow; deferred to Out of Scope as a follow-up rather than bundled here.
- **Patch-vs-refactor verdict:** this is a **patch** because the problem is one manifest line pointing at one host, and the solution is proportionate: one line moved, one comment added, one gate. The one place a refactor would be tempting — CI-wide package caching — is named as a follow-up rather than silently mud-patched into this diff.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
