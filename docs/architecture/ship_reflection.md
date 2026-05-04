# 14. Ship Reflection (post-launch, Q2 2026)

> Parent: [`README.md`](./README.md). Sibling of [`bastion.md`](./bastion.md) (§13).

> **Status: PENDING SHIP.** Skeleton landed pre-launch with the M51_001 architecture cross-reference pass. Dates, evidence URLs, and the post-ship narratives below are filled in by the team after the launch tweet goes out and the first external installs land. Do not paraphrase or wishful-think the evidence section — only write what actually happened.

---

## What shipped vs planned

PENDING SHIP. Walk M40–M49 in order. For each, one line: *"matches plan"* or *"deviated: <one-line reason>"*. Cross-reference each note against the spec in `docs/v2/done/`. The §4.1 cross-reference pass (M51_001) found these references already correct at the M-ID level (M41, M45, M46, M47, M48, M49 all in `done/`); this section captures whether the *capability description* in `capabilities.md` and the *interface signatures* in `data_flow.md` / `user_flow.md` survived implementation unchanged.

Did the wedge ship as designed (GitHub Actions trigger + chat steer + Slack post)? Did the substrate (M40–M45) hold up under the first real external incidents? Did context layering (M41 — memory checkpoint + rolling tool-result window + stage chunking) avoid the embarrassment Codex predicted in `plan_engg_review_v2.md`?

## What surprised us

PENDING SHIP. 1–2 paragraphs. Decisions that didn't survive contact with implementation. Operational learnings from the first week of external use. Anything that would have changed how M40–M45 was scoped if we'd known.

## What we deferred

PENDING SHIP. 1 paragraph. Concrete deferral status as of launch:
- **BYOK provider coverage (M48):** which providers actually shipped, which are stub-only (Together / Groq / Moonshot status as of ship date).
- **Approval inbox (M47):** dashboard inbox status; Slack DM approvals status.
- **Self-host:** confirmed deferred to v3 (per `high_level.md` §1) — note any v3 prework that *did* land opportunistically.
- **Install-skill host coverage (M49):** which hosts ship green (Claude Code, Amp, Codex CLI, OpenCode) at launch; any host with a known incompatibility.
- **Install pingback / Day-N telemetry:** removed from M51 scope May 03, 2026; note whether the deferral held or got reversed.

## Evidence

- Launch date: PENDING SHIP — `<YYYY-MM-DD>`
- First external install: PENDING SHIP — `<YYYY-MM-DD>`, `<operator>` at `<company>`
- Public artifacts: PENDING SHIP — `<URLs to launch post, HN thread, screen recording>`
- First real external incident the zombie diagnosed: PENDING SHIP — `<YYYY-MM-DD>`, `<one-line description>`

---

## How to use this section

This appendix is **what's NEW post-ship** — surprises, deferred items, evidence. It is **NOT a roadmap**: future work goes in pending specs under `docs/v2/pending/`, not here. Cap the prose at ~600 words once the PENDING SHIP placeholders are filled in. If the team wants longer-form retrospective, write a separate doc and link from here — keep this section scannable.

When the PENDING SHIP markers come down, update this file's parent header (`Status:` line in `README.md`) to `Status: Reflects v2 launch as of <date>` so future readers know the doc is grounded in real evidence rather than pre-launch placeholder.
