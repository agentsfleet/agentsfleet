# M38_001: Homebox-Audit Drift Detector — Second Flagship Sample

**Prototype:** v2.0.0
**Milestone:** M38
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P2 — secondary sample; platform-ops (M37_001) is the primary dogfood. Homebox-audit proves the product handles a different zombie shape: long-lived, cron-scheduled, diff-based ("report only changes"), minimal operator interaction. Useful for the second customer, not blocking M31 acceptance for the first customer.
**Batch:** B2 — after M34..M37 land (depends on the same infra). Sequence: after M33's first dogfood succeeds, M38 lands to broaden the sample catalog.
**Branch:** feat/m38-homebox-audit (to be created)
**Depends on:** M37_001 (establishes the three-file sample pattern and the dogfood baseline), M33_001 (chat trigger), M35_001 (executor policy with http_request credential templating), M36_001 (live watch for debugging during dogfood). M34_001 (events history) is how we verify it ran.

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` — this sample is an instance of the same event-flow pattern with cron rather than chat as the primary driver.

---

## Overview

**Goal (testable):** `zombiectl zombie install --from samples/homebox-audit` installs a zombie whose SKILL.md declares a daily-drift-detector role. On first chat invocation, the agent calls NullClaw's `cron_add "0 9 * * *" "run daily homebox drift scan"` and immediately runs an initial scan as a baseline. Each subsequent scheduled invocation: pulls current container + TLS state, diffs against yesterday's snapshot stored in `memory_store`, posts ONLY the diffs to Slack. Silent when nothing changed. Over 7 days of dogfood we see (a) the first baseline, (b) at least one "no change" day (silent), (c) at least one real drift day with a useful Slack report.

**Problem:** `samples/homebox-audit/README.md` exists but describes a "quarterly audit" framing that depends on humans remembering to run it. That pattern dies — batch tools with human-cadence triggers don't survive past the first month. Needs a reframe as a continuous drift detector that runs itself and surfaces only what changed.

**Solution summary:** Three files under `samples/homebox-audit/` (SKILL.md + TRIGGER.md + README.md rewrite). SKILL.md describes the drift-detection loop: build a snapshot, compare to prior, report only diffs. Uses `http_request` (to crt.sh for TLS, to nvd.nist.gov or similar for CVE lookups, to Slack for output), `shell` (docker ps, kubectl get — read-only; enforced by the per-zombie tool list + prose allowlist), and `memory_store` / `memory_recall` to persist the prior snapshot. TRIGGER.md declares `chat` trigger (seeds the schedule) + the `cron_*` tool set so the agent can self-schedule. Credentials: docker_socket, kubectl_config (optional), slack_bot_token (shared with platform-ops vault entry). Policy: prose allowlist in SKILL.md under "Tools you can use" — same pattern as platform-ops.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `samples/homebox-audit/SKILL.md` | CREATE | Drift-detection playbook. Prose describes: (1) build snapshot: containers (docker ps + docker inspect), TLS certs (crt.sh query or direct probe via http_request), exposed ports (docker inspect), default-creds heuristics (grep image known-for-defaults list); (2) diff vs yesterday via memory_recall; (3) store new snapshot via memory_store; (4) post only diffs to Slack. Self-schedules via cron_add on first run. |
| `samples/homebox-audit/TRIGGER.md` | CREATE | `trigger.type: chat` (seeds the schedule and allows operator on-demand runs). `tools: [http_request, shell, memory_store, memory_recall, cron_add, cron_list, cron_remove]`. `credentials: [docker_socket, slack]` (+ optional `kubectl_config`). `network.allow: [crt.sh, api.nvd.nist.gov, slack.com]` (+ whatever CVE source the agent elects). `budget.monthly_dollars: 4`. |
| `samples/homebox-audit/README.md` | REWRITE | Operator quickstart: install → first chat ("run the initial scan and schedule daily") → daily silent operation → example Slack report on a drift day. Documents the diff-based design. |

No `src/**` changes — this sample exercises the M31 infra; doesn't add new infra.

---

## Applicable Rules

**FIR** (tools subset of NullClaw primitives). **ORP** (no stale references to quarterly-audit framing outside historical docs). **TST-NAM** (no milestone IDs in any tests).

---

## Sections

### §1 — Sample authoring

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `samples/homebox-audit/SKILL.md` | skill parser | model set; prompt body describes snapshot build, diff, store, report; prose allowlist for shell read-only verbs; budget note ≤$4/mo | integration |
| 1.2 | PENDING | `samples/homebox-audit/TRIGGER.md` | loader | matches declared shape | integration |
| 1.3 | PENDING | `samples/homebox-audit/README.md` | new operator read | install, first scan, daily behavior, example drift output, "how to silence a noisy check" section | manual |

### §2 — Install + first run

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `zombiectl zombie install --from samples/homebox-audit` | CLI | 201 + zombie active | integration |
| 2.2 | PENDING | first chat "run initial scan + schedule daily" | — | agent: (1) runs snapshot via shell+http_request; (2) memory_store the snapshot; (3) cron_add daily; (4) Slack posts "baseline complete, no drift (first run)" | integration |
| 2.3 | PENDING | second chat "re-scan now" | — | agent: snapshot; diff vs memory_recall; if diff: Slack report; if no diff: Slack silent (but activity stream shows the run) | integration |

### §3 — Scheduled operation (via NullClaw cron)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | cron fires next morning (or simulated) | — | synthetic event lands on zombie:{id}:events with actor=`cron:0 9 * * *`; agent runs same loop; posts diff or stays silent | integration (may use accelerated-clock harness) |
| 3.2 | PENDING | week of dogfood (author's homebox) | real passage of time | by day 7: ≥1 baseline run, ≥2 silent days, ≥1 real drift day with useful report | manual (dogfood) |

### §4 — Credential sharing and isolation

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | slack credential shared with platform-ops | single vault entry `slack` | both zombies use it; rotation updates both | manual |
| 4.2 | PENDING | docker_socket scoped to this zombie | — | only this zombie's config.credentials references `docker_socket` | config lint |

---

## Interfaces

All consumed from M31 infra. No new interfaces.

---

## Failure Modes

| Failure | Trigger | Behavior | Observed |
|---|---|---|---|
| No prior snapshot in memory (first run) | fresh install | agent treats snapshot as "vs empty" — Slack posts "baseline complete"; no alarming diff | clean first-run UX |
| memory_store full / slow | many snapshots over time | NullClaw memory has its own retention; agent could trim or keep rolling window (prose decides) | documented pattern: "if memory grows, use cron_remove + cron_add to reset state" |
| shell docker ps fails | docker daemon not reachable | agent reasons from error; reports "scan failed: docker unreachable" to Slack with zero drift claim | honest failure report |
| http_request to crt.sh rate-limited | too many hosts | agent detects 429, backs off within prose guidance, retries next cron tick | eventual consistency |
| Agent drifts into chatty mode (posts when nothing changed) | prompt weakness | Discovery item to tighten SKILL.md wording; add explicit "stay silent when no diff" rule | iterative |

---

## Invariants

| # | Invariant | Enforcement |
|---|---|---|
| 1 | SKILL.md policy keeps shell to read-only verbs | prose allowlist + M35 / future verb gate |
| 2 | Silent on no-diff (the signal-to-noise claim) | dogfood observation over 7 days; captured in §3.2 |
| 3 | Zombie self-schedules on first run (cron_add) | §2.2 integration + `cron_list` check post-run |
| 4 | No webhook trigger in TRIGGER.md | M33 pattern — chat + cron only |

---

## Test Specification

### Unit

| Test | Dim | Expected |
|---|---|---|
| homebox-audit skill parses | 1.1 | ok |
| homebox-audit trigger parses | 1.2 | ok |
| tools subset of NullClaw | inv 2 (M33) | ok |
| no webhook trigger | inv 4 | ok |

### Integration

Per §2 + §3.1. Real pg + Redis + executor. A deterministic snapshot source (mock docker ps output) lets us drive diff/no-diff branches.

### Manual dogfood

§3.2 — run for 7 real days on author's homebox.

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): worktree. | `git worktree list` |
| 2 | Write SKILL.md. | §1.1 |
| 3 | Write TRIGGER.md. | §1.2 |
| 4 | Rewrite README.md. | §1.3 |
| 5 | Integration tests piggyback on M34+M36 harness. | §2 + §3.1 |
| 6 | 7-day dogfood (real time). | §3.2 |
| 7 | CHORE(close): spec → done/, Ripley log, release-doc. | PR green |

---

## Acceptance Criteria

- [ ] Three files at `samples/homebox-audit/`.
- [ ] Install + first chat produces baseline snapshot + Slack post + cron schedule.
- [ ] Subsequent chat detects no-diff and stays silent (Slack-wise); activity stream shows the run.
- [ ] Cron-fired invocation runs the same loop.
- [ ] 7-day dogfood shows ≥1 baseline, ≥2 silent days, ≥1 real drift day.
- [ ] Cost per month ≤ $4 (fits within starter credit alongside platform-ops).

---

## Eval Commands

```bash
test -f samples/homebox-audit/SKILL.md   && echo ok
test -f samples/homebox-audit/TRIGGER.md && echo ok
test -f samples/homebox-audit/README.md  && echo ok

# trigger tools are NullClaw primitives (lookup against v2026.4.9)
grep -A 10 '^tools:' samples/homebox-audit/TRIGGER.md

# no webhook in trigger
grep -E '^\s*type:\s*webhook' samples/homebox-audit/TRIGGER.md && echo FAIL || echo ok

# budget cap
grep -E 'monthly_dollars' samples/homebox-audit/TRIGGER.md \
  | awk -F: '{ print $2 }' | tr -d ' ' \
  | awk '{ if ($1+0 > 4) print "FAIL: " $1; else print "ok: " $1 }'
```

---

## Discovery (fills during EXECUTE)

- Where the CVE lookup goes (api.nvd.nist.gov vs. an aggregator — measure latency + completeness).
- Whether docker inspect output is stable enough to diff structurally, or needs normalization before store.
- How noisy the "default credentials" heuristic is in practice.

---

## Out of Scope

- Automated remediation (v1.1 behind approval gates).
- Multi-host homelab (one docker + one kubectl context MVP; broader later).
- Integration with home-assistant / immich / jellyfin specific schemas — generic containers/TLS/ports is the MVP.
- Image-pull daemon state (would need docker pull, not allowlist-safe).
