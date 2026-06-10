# Office hours — the support-engineer wedge (Jun 09–10, 2026)

> Archive record of a product-direction session (started in Codex, continued in
> Claude). Non-canon: this captures the thesis as explored, not a commitment.
> Sibling of [`office_hours.md`](./office_hours.md) (the original v2-wedge
> session).

## The wedge under exploration

A "virtual support engineer" that does the first 20–120 minutes of escalation
archaeology on infrastructure-product tickets. Dataset in hand: 100+ E2E
Networks Zoho Desk tickets (5 sampled in-session).

## Thesis state at close

- **Zendesk read:** a B2B company whose product/AI economics are tuned to
  B2C-shaped support — deflection at volume, per-resolution pricing (bought
  Ultimate, 2024). Verified via web during the session.
- **Competitive grid** (trigger × where-the-answer-lives):
  - ticket × docs — crowded (Zendesk AI, Zia, Fin, Decagon, Sierra, RunLLM).
  - alert × state — the AI Site Reliability Engineering players (Resolve.ai,
    Cleric, Traversal).
  - **ticket × internal-control-plane-state — the open cell.** Only bespoke
    builds (e.g. the WSO2 case study) live there today.
- **Falsifiable problem statement:** at B2B infrastructure vendors, ≥40% of
  support→engineering escalations need no novel engineering judgment — only
  privileged state access plus known-failure-mode recognition. Ticket evidence:
  19 h/23 h response latencies are queue time, not think time; some engineering
  replies are just "the next question for the customer."
- **Ideal customer profile:** B2B infrastructure vendors (cloud / hosting /
  DBaaS / storage), 50–5000 employees, a 5–50-person support org separate from
  engineering. User = tier-1/2 support; buyer = engineering leadership;
  co-sponsor = head of support. Fork answered: vertical infra diagnosis layer,
  not a horizontal SaaS copilot.
- **Why `usezombie`:** the M84 chassis — sandbox hardening, egress allowlist,
  vault credential custody, the approvals plane, agent memory — IS the
  trust/deployment shape this cell requires: an in-network agent reading
  attacker-controllable ticket text while holding production read-credentials.
  Connectors (git/Notion/Jira) are commodity, not the moat. E2E's dead internal
  LangChain bot is the proof that docs-RAG without credentialed state access
  changes no behavior.

## The three validation tests

1. **Bucket-label 30 tickets** into 5 classes; buckets 1–3 (state-access +
   known-failure-mode) ≥40% → proceed. Bucket 4 (product bugs) dominant →
   reliability is the real lever, not support tooling.
2. **Time-to-encode** a failure class must fall toward <1 day by class 3–5 —
   otherwise it's the consulting trap (WSO2-shaped bespoke work).
3. **Two-week shadow at E2E**; the metric is escalations avoided per week.

## Open assignment at session close

Bucket-label the 30 tickets. Offered next artifacts: a labeling rubric + an
evidence-packet template for the volume-detach class.
