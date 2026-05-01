# Office Hours v2 — Product Framing Notes

> Parent: [`README.md`](./README.md)
>
> Status: Internal strategy note, cleaned for durable reference.
> Use this file for product framing, persona choice, wedge choice, and validation bar.
> Do not use it as the canonical source for implementation status, spec backlog, or launch sequencing.

---

## What problem this product is actually solving

Operational outcomes fall into limbo.

When a deploy fails, an inference endpoint starts returning errors, or production looks unhealthy, the operator has to correlate signals across several tools, hold the timeline in their head, decide what to try next, and often communicate status manually. The painful part is not one missing graph or one missing alert. The painful part is fragmented evidence, no durable memory across attempts, and no long-lived worker that keeps carrying the outcome forward.

For v2, the sharpest wedge is deploy-failure and production-triage work:

- gather evidence from CI, infrastructure, logs, and chat surfaces
- preserve the timeline with actor provenance
- keep going after the original interactive session is gone
- ask for approval when the next action is risky

The wedge is intentionally narrower than "AI for SRE." It is a durable operational runtime for one concrete outcome.

## Demand honesty

The demand starting point is Customer Zero pain, not broad external validation.

That is acceptable only if the product is explicit about the bar it must clear next:

- an external operator must keep the install on
- the zombie must help on a real production-flavored event
- the value must come from the durable runtime, not just from a one-shot chat summary

The purpose of this file is to keep that honesty intact. It is not a substitute for customer-development evidence.

## Target user

The primary user is a high-agency operator who owns infrastructure outcomes directly:

- SRE or platform engineer at a small team
- founder-operator or strong indie engineer
- teams that cannot or do not want to pipe their entire ops stack into another SaaS
- teams that value open source, Bring Your Own Key, and prompt-defined behaviour over a heavy workflow builder

The broader category includes AI-infra teams, GPU-cloud operators, regulated mid-market teams, and agentic-operations teams, but the v2 wedge should stay grounded in the small-team operator who feels this pain acutely today.

## Product wedge

The right wedge is `/usezombie-install-platform-ops`.

It is not a bash installer and it is not a hosted demo first. It is a host-neutral skill that:

- detects the user's repo shape
- asks a very small number of gating questions
- resolves credentials with minimal repeated input
- generates `.usezombie/platform-ops/{SKILL,TRIGGER,README}.md`
- drives `zombiectl install --from ...`
- immediately opens `zombiectl steer` for a real smoke test

This wedge matters because it makes the architecture reachable. The markdown-defined runtime is the product, but the install skill is the front door.

## Durable product conclusions

These are the conclusions from the office-hours work that should be treated as durable unless a later architecture decision replaces them:

- v2 differentiation is **open source + Bring Your Own Key + markdown-defined behaviour**.
- v2 is **hosted-only** on `api.usezombie.com`; self-host is a later workstream.
- the flagship workflow is **GitHub Actions deploy-failure response plus manual steer**, not a generic automation platform
- the same zombie should handle webhook, cron, and manual steer through one reasoning loop
- the initial trust boundary is **internal operations**, not customer-facing status communication
- the install experience should be **host-neutral** across Claude Code, Amp, Codex CLI, and OpenCode

## What not to lead with

Do not lead the product with:

- "general AI assistant for ops"
- "self-hostable today"
- "automated customer status page"
- "another workflow builder"

Those may become adjacent stories later, but they weaken the v2 wedge if they lead the narrative.

## Bastion direction

The bastion direction remains strategically important: the same zombie that handles internal triage could later own customer-facing status communication. But that is a second-order extension. The product has to earn trust on internal-only incidents first.

That is why `bastion.md` exists as direction, while the launch framing stays centered on internal deploy and outage triage.

## Validation bar

The useful validation bar is not raw install count.

The useful validation bar is:

- an external team keeps the install on for at least a week
- the zombie participates in a real event
- the operator reports that the product materially improved the outcome

Install count matters only as supporting evidence. Retention and real-event value matter more.

## Open questions that still matter

- Where should the published skill live for easiest discovery and maintenance?
- How should non-GitHub-CI users be handled before there is first-class support?
- How much of the generated `.usezombie/platform-ops/` config should be committed versus regenerated?
- What is the cleanest public story for install telemetry and privacy?

These are product questions, not architecture constants.

## How to use this file

Use this file when writing:

- product framing in public docs
- hero copy and quickstart positioning
- founder notes about wedge choice
- internal discussion about who the first user is and why

Do not use this file when you need:

- implementation truth
- test truth
- current spec or milestone status
- route-by-route system behaviour

For those, use [`README.md`](./README.md), [`high_level.md`](./high_level.md), [`user_flow.md`](./user_flow.md), [`data_flow.md`](./data_flow.md), [`capabilities.md`](./capabilities.md), and [`billing_and_byok.md`](./billing_and_byok.md).
