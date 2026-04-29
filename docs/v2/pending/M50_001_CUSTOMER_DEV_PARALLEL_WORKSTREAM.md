# M50_001: Customer-Development Parallel Workstream

**Prototype:** v2.0.0
**Milestone:** M50
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING

> **Slot history:** M50 was previously assigned to "ARCHITECHTURE.md cross-reference + post-launch reflection." That spec was folded into M51 on Apr 25, 2026 (see M51's "Note on M50 fold" section). The slot was reassigned to this customer-development workstream the same day, after a /plan-ceo-review surfaced that customer-dev needed first-class status to hit the Day-50 validation bar. Both decisions land in the same `docs/v2-spec-backlog` branch.
**Priority:** P1 — validation-blocking. The Day-50 success bar (1 external team using on real event for ≥1 week) requires external operators identified, contacted, and committed to install slots BEFORE the substrate ships. Sequential customer-dev (start at Week 6, find users in 14 days) compresses the validation window past breaking. This workstream runs in parallel from Week 1 so the wedge framing is validated while the substrate is still mutable.
**Categories:** RESEARCH, DOCS
**Batch:** B1 — runs in parallel with M40-M48 substrate. ~2-3 hours/week founder time. Non-blocking on engineering work.
**Branch:** No code branch. Artifacts live in `~/.gstack/projects/usezombie/customer-dev/` (local) and Ripley's Log entries (durable record).

**Canonical architecture:** `docs/ARCHITECHTURE.md` (TOC) → `docs/architecture/high_level.md` §1 differentiation pillars + §5.1 platform-ops persona + `docs/architecture/office_hours_v2.md` P1 persona expansion (AI-infra / GPU-clouds / regulated mid-market / 10x indies / agentic-OS operators).

---

## Implementing agent — read these first

This workstream is founder-led, not agent-led. The "implementing agent" here is the author working ~2-3 hours/week. Agent assistance is for drafting outreach copy, summarizing call notes, and synthesizing weekly progress, not for sending DMs or running calls.

Required reading before Week 1 outreach:

1. `docs/architecture/office_hours_v2.md` Persona section (lines 36-40) — the explicit P1 + P2 personas and the SaaS-fatigue signal.
2. `docs/architecture/high_level.md` §1 differentiation (3 pillars: OSS + BYOK + markdown-defined) — the actual claim being tested.
3. `docs/architecture/high_level.md` §5.1 platform-ops persona — what the zombie does in concrete terms.
4. The author's existing E2E Networks SRE workflow notes (mental model only, not in repo) — Customer Zero's lived experience, source of credibility in outreach.

---

## Overview

**Goal (testable, Day-35):** 3 named operators (real humans, real companies, real handles) have committed to a Day-35 install slot for `/usezombie-install-platform-ops` on their own infra. "Committed" = (a) accepted a calendar invite, (b) confirmed they can paste a Slack webhook URL on the call, (c) agreed to keep the install on for ≥1 week.

**Goal (testable, Day-50):** 1 of those 3 has installed, kept the install on for ≥1 week, and reported value from a real production event. Names + the event details land in the Ripley's Log.

**Problem:** The substrate workstream (M40-M48) takes 5-6 weeks. The original plan started customer-development at Week 6. That gives 14 days from cold outreach to install-and-soak before the Day-50 bar. Two failure modes that path opens:

1. **Shipping into silence.** Substrate ships, no external operator has touched it, the wedge framing might be wrong, and 5-6 weeks have burned. The signal arrives too late to course-correct cheaply.
2. **No buffer for the messy middle.** External operators don't install on demand. They need 2-3 touch points, an async demo, a peer they trust nudging them. 14 days is not enough.

**Solution summary:** Run customer-dev as a first-class parallel workstream from Week 1. Founder time: ~2-3 hours/week. Output: a curated cold-DM list (15 named operators), a 60-second async demo asset (whatever exists today, even rough), weekly outreach cadence, and a Ripley's Log entry every Friday tracking conversions.

This is not "marketing." This is operator-discovery — finding the 3 humans who would actually install platform-ops on their infra by Day 35. The willfulness move that the office_hours doc explicitly named as a fallback if Day 50 fails. M50 makes it the plan, not the fallback.

---

## Files Changed (blast radius)

This workstream produces artifacts, not code. No source files change.

| File / artifact | Action | Why |
|---|---|---|
| `~/.gstack/projects/usezombie/customer-dev/cold-dm-list.md` | NEW | The 15 named operators: handle, company, persona match, shared connection (if any), outreach status |
| `~/.gstack/projects/usezombie/customer-dev/demo-asset-v1.mp4` (or `.gif`) | NEW (Week 2) | 60-second async demo of Customer Zero's existing flow. Rough is fine. |
| `~/.gstack/projects/usezombie/customer-dev/outreach-templates.md` | NEW | Cold-DM v1 + follow-up + "demo ready" templates. Iterated weekly. |
| `~/.gstack/projects/usezombie/customer-dev/call-notes/{date}-{handle}.md` | NEW (per call) | Notes from each operator conversation: their workflow today, their current pain, their interest level, the next ask |
| `docs/nostromo/LOG_{date}_M50_CUSTOMER_DEV.md` | NEW (Friday weekly) | Ripley's Log entry: this week's outreach count, conversations, conversion to Day-35 install slot, blockers |
| `docs/ARCHITECHTURE.md` | NO EDIT here | M51's cross-reference pass picks up persona learnings post-launch |

> **Privacy:** Operator names + handles stay LOCAL to `~/.gstack/`. Nothing committed to the repo until they've explicitly said yes to public attribution. The Ripley's Log entries reference operators by initials or company until written consent.

---

## Sections (implementation slices)

### §1 — Week 1: Build the cold-DM list (one-time, ~3 hours)

Source the 15 named operators from:

- **AI-infra startups** (founders or platform engineers): scrape recent YC W26 / S26 batches, filter to "AI infra" / "GPU" / "agent infra" tags. Target 5 names.
- **GPU cloud operators**: smaller players (Lambda Labs / RunPod / Vast.ai team members on Twitter, plus E2E Networks peers). Target 3 names.
- **Regulated mid-market** (companies that won't pipe ops into another SaaS for compliance reasons): fintech / healthtech platform engineers visible on Twitter or in public Slack communities. Target 4 names.
- **10x indie engineers / agentic-OS operators**: people building solo-or-small-team AI products who tweet about ops pain. Target 3 names.

For each name, capture in `cold-dm-list.md`:
```
- handle: @example
  company: Example Corp (AI infra YC W26)
  persona: P1 (AI-infra)
  shared signal: tweets about CD pipeline pain Apr 12
  contact path: Twitter DM | warm intro via X | conference connection
  status: not contacted
```

**Disqualifiers:** anyone already on a competitor (Sonarly / incident.io); anyone whose company has >50 engineers (wrong persona); anyone the author has no plausible authentic outreach hook for.

### §2 — Week 1-2: Outreach v1 (cold DMs)

Send 5 DMs/week. Author tone, not corporate. Each DM:

1. **Names a specific recent tweet/post** of theirs that signals the pain (not generic).
2. **States Customer Zero's lived workflow in one sentence.** "I'm an SRE at E2E Networks; I waste an hour every deploy failure correlating logs across Grafana / Zabbix / Django logs / GitHub Actions."
3. **Names what's being built**, briefly. "Building a markdown-defined zombie that wakes on the GH Actions failure and posts the diagnosis to Slack with evidence."
4. **Asks for a 20-min async exchange**, not a meeting. "Would you reply to a 60-second Loom of how it currently works at E2E? No commitment beyond 'yes/no, would this help your team?'"

Track in `outreach-templates.md`. Iterate the template every Friday based on response rate.

### §3 — Week 2: Demo asset v1 (~2 hours)

Record a 60-second screen recording (or animated GIF, or 5-screenshot PDF — whichever is fastest) of Customer Zero's CURRENT manual workflow during a deploy failure. Even rough. The point is to show the pain, not the polish. "This is what I do today" is more compelling than "this is what we're building."

Embed in outreach-templates.md as the asset DM'd to operators who reply "yes, send the demo."

> **Implementation default:** if recording the manual flow is hard, do a Loom of the existing platform-ops sample running locally, even if substrate is incomplete. Operator pattern-matches on "this is the shape" not "this works end-to-end today." Be honest in the DM about what's working vs aspirational.

### §4 — Week 3-5: Weekly cadence

Every Monday: send 5 fresh DMs. Reply to anyone from the prior week.

Every Wednesday: do any 20-min async or live calls with operators who said yes.

Every Friday: write the Ripley's Log entry. Track: this week's DMs sent, replies received, calls booked, calls held, install-slot commits secured.

Founder time: 2-3 hours/week max. If it's eating more, the wedge framing might already be wrong — surface that to the author, do not spend more hours.

### §5 — Week 5-6: Convert to Day-35 install slots

By Week 5, the substrate is ~2 weeks from ship. Operators who responded warmly get a follow-up: "Substrate ships ~May 30. Want a 30-min slot the week of June 6 to install platform-ops on your repo? I'll be on the call to debug if anything breaks."

Goal: 3 confirmed install slots before substrate ships.

### §6 — Week 6-8: Install + soak

For each confirmed slot: get them to `/usezombie-install-platform-ops` running on their repo. Stay on the call until first Slack post lands. Send a follow-up after 7 days asking if they kept it on and what value they saw.

Concierge-grade. The Day-50 bar accepts concierge installs.

### §7 — Week 8: Day-50 Ripley's Log entry

Final Ripley's Log entry under M50: how many of the 3 stayed on, what value they reported, what they wished worked differently. Names + company in the log if they consented to public attribution; initials + persona-tag otherwise.

If 0 of 3 converted, surface to the author within 24h. Per office_hours: pivot to operator-discovery (cold DMs to ~15 more named SREs) before more code.

---

## Interfaces

```
External (Twitter / email / shared Slack):
  Cold DM template → operator reply → demo asset → 20-min async → install-slot commit

Internal artifacts:
  ~/.gstack/projects/usezombie/customer-dev/cold-dm-list.md      (status tracker)
  ~/.gstack/projects/usezombie/customer-dev/call-notes/*.md       (per-conversation)
  ~/.gstack/projects/usezombie/customer-dev/outreach-templates.md (iterated weekly)
  docs/nostromo/LOG_*_M50_CUSTOMER_DEV.md                          (weekly cadence)

Conversion metric (Day-35):
  3 named operators with confirmed install slots in calendar.

Conversion metric (Day-50):
  1 operator kept install on ≥7 days, reported value from real production event.
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Reply rate <10% on cold DMs after Week 2 | Wrong persona OR wrong pain framing OR wrong asset | Stop, rewrite the DM template + persona target. Don't send 30 more cold DMs at the same template; the data point is real. |
| 0 install slots booked by Week 5 | Wedge framing not resonating | Surface to author within 48h. Pivot decision: change the wedge framing OR delay launch by 2 weeks to retry outreach with different positioning. |
| Operator says yes to install but ghosts on the day | Common. Concierge norm. | Send 1 follow-up after 24h, 1 after 72h. If still no response, deprioritize and add 1 fresh name to the cold-DM list to maintain pipeline. |
| Operator installs but churns within 3 days | Substrate bug, wedge mismatch, or operator-side context shift | First 24h: get on a call, find out why. If substrate bug, file as P0. If wedge mismatch, capture in the Ripley's Log — that's signal. |
| Author runs out of time during a sprint week | Founder time pressure | Skip the week's outreach but DO write the Friday Ripley's Log entry noting the skip. Don't pretend the week happened. |

---

## Invariants

1. **No operator names in the public repo without explicit consent.** Local `~/.gstack/` only.
2. **Friday Ripley's Log entry happens every week, even if the week was zero-output.** The log is the source of truth for whether this workstream is on track.
3. **Founder time stays ≤3 hours/week.** If outreach is eating more, the framing is broken — fix the framing, don't add hours.
4. **Concierge installs count.** The Day-50 bar does not require self-serve. Author-driven first install is fine.
5. **Reply quality > reply count.** 1 deep "yes, this is exactly my pain" beats 5 shallow "interesting, send me more info."

---

## Test Specification

This workstream's tests are observational, not automated. The Friday Ripley's Log is the test artifact.

| Test (observational, weekly) | Asserts |
|---|---|
| `test_friday_log_exists_for_week_N` | A `LOG_*_M50_CUSTOMER_DEV.md` file exists in `docs/nostromo/` for each Friday between Week 1 and Week 8. No skipped weeks unwritten. |
| `test_dm_count_meets_cadence` | Each weekly log reports ≥5 DMs sent (or explicit reason for skip). |
| `test_pipeline_size_growing` | By Week 4, the cold-DM list has ≥10 operators in active conversation status (not "not contacted"). |
| `test_day_35_commits` | By the Day-35 substrate ship date, ≥3 confirmed install slots booked in calendar. |
| `test_day_50_conversion` | By Day 50, ≥1 operator has kept the install on ≥7 days and reported value from a real event. Logged with names (with consent) or initials (without). |

---

## Acceptance Criteria

- [ ] `~/.gstack/projects/usezombie/customer-dev/cold-dm-list.md` exists with 15 named operators by end of Week 1
- [ ] First demo asset (`demo-asset-v1.mp4` or equivalent) shipped by end of Week 2
- [ ] Friday Ripley's Log entries for every week between Week 1 and Week 8 (no gaps)
- [ ] By Week 5: ≥3 install slots confirmed in calendar for the Week 6-8 window
- [ ] By Day 50: ≥1 external operator has kept platform-ops install on for ≥7 days and reported value from a real production event
- [ ] If Day 50 produces 0 installed-and-used external operators: a documented decision on whether to pivot the wedge framing OR continue with a different outreach strategy. No silent abandonment.

---

## Out of Scope

- **Paid customer-development** (consultants, user-research firms, recruited interviews via UserInterviews.com) — author-driven cold outreach only in v1. Paid CD is a fallback if Day-50 fails.
- **Conference presence / sponsorships** — distribution play, not customer-discovery.
- **Content marketing** (blog posts, technical articles) — separate workstream; M51 owns the docs.usezombie.com positioning content.
- **Sales pipeline / CRM tooling** — 15 names in a markdown file is the right tool for this stage. Don't graduate to HubSpot / Salesforce.
- **Public attribution of operators without consent** — operator privacy stays opt-in, always.
- **Anything that pulls founder time above 3 hours/week** — substrate work is the day job; this workstream is the parallel discovery channel.

---

## Note on parallelism

This workstream runs alongside M40-M48 substrate. It does NOT block any engineering milestone. It does NOT require engineering review. It is a founder-owned discovery channel whose only artifact in the main repo is the weekly Ripley's Log.

The CHORE(open) for M50 is lightweight: move this spec to `docs/v2/active/`, set Status: IN_PROGRESS, no worktree needed. The CHORE(close) at Day 50 (or Day 60 if soak extends) reviews whether the wedge framing held.
