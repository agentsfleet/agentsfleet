---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# ChatGPT CTO AIWA review prompt

## What it is

A read-only review prompt for the amended proposal and one-PR specs.

## Why it exists

Challenge the selected design and its proof requirements without reopening owner decisions.

## How it behaves

Paste the complete prompt below and provide the [review packet](review-packet.md). ChatGPT without filesystem access needs the bundled Markdown files; it must identify source claims it cannot verify.

```text
You are CTO AIWA. Review this amended workspace-chat proposal and implementation specs read-only.

Worktree: /Users/kishore/.codex/worktrees/fd5a/agentsfleet
Branch: feat/website-completion
Source revision: ad3ed8f92c739b59df71863dbeec81d621dd7d0a
Start with docs/architecture/chat_authoring_review_packet.md and follow its reading order.
Read .orly/LOCAL.md and .orly/AGENTS.md before interpreting repository gates.
Do not edit code/documents, run paid models/providers, reset databases, commit, push, or deploy.

OWNER DECISIONS: L1–L21 in chat_authoring_decisions.md are settled; do not reopen.
The approved interface stays. One current branch/worktree/product PR contains the chat cutover
and folded website/PostHog/branding batch. L8 schedule provider redesign remains a follow-up.
The independently owned stable-ID/workspace-ownership fix is a prerequisite, not assumed merged.

CANONICAL API: chat_authoring_api_resources.md. Exactly17 new person routes,7 changed,2 removed.
One existing runner claim returns a tagged lease;4 new authoring machine routes, including daemon-mediated model calls. Private draft
/messages recovers state/answers/usage, no public turn ID/status route or operations service.
Fleet PATCH is status/name only with If-Match. Nanos replace dollar budgets. Library has one name.

REVIEW QUESTIONS:
1. Are the five source agreements correct? Cite actual file:line, distinguish inspected behavior
   from target changes. New APIs have no invented source reference.
2. Does Definition preserve every gate/anomaly/network/repository/write/context/metadata/support
   field through the single parser and export? Are nanos caps/positive values/precision exact?
3. Are creator scope, joined-owner models/wallet, stream/cursor/replay authorization and revocation
   enforced on every path? Can private text leak through caches/telemetry/report spool/publication?
4. Do key/body identity, If-Match tags, immutable pins, review digest and policy-marker admission
   stop races without making observation timestamps permanently stale? Is one replay key used?
5. Can setup recover every crash in-request, stay installing until actual grants, resist Kill
   versus activation, and admit exactly one run_now across append/receipt failure and retry?
6. Can Stop through a sequence race a later Send or terminal report safely? Can deletion fence
   late text while still settling known charges once? Are residual-copy bounds enforced/disclosed?
7. Does one daemon claim bound mixed capacity across replicas? Is lock order acyclic and fairness
   meaningful? Does authoring get only scoped model access, with no tenant tools/mint rights?
8. Is worst-case input8*32768, output8192 and runtime300 seconds priced with checked integer
   rounding under one wallet lock? Does the maximum-priced active/proposed model cover varied
   prices? Are unknown upstream obligations retained without inventing refunds/zero spend?
   The design does not reserve funds against later runtime debits; assess that honest limit.
9. Are token/window/system/tool/reasoning/output/time/provider retry caps actually enforceable?
   Can a 32 KiB edit fit through small operations without re-emitting the whole definition?
10. Is Library publication separate from Save, with receipt reuse, history pins, owned-only writes,
    no removed-route aliases, and installed fleet references surviving entry removal?
11. Are runtime affinity/fences/wake fallback, renewed budget, optional child narrowing, wall-clock
    queue wait, Slack stopped notice, steer receipt-after-append and outbound delivery preserved?
12. Are the two specs complete and consistent with the 17/7/2 routes and proposed errors? Map every
    failure to meaningful unit/integration/browser proof. Check real Make targets, CI environment,
    generated API/client removal, frozen migration history and coordinated release recovery.

OUTPUT:
- Verdict APPROVE / APPROVE WITH REQUIRED CHANGES / RETHINK, separating spec structure from
  implementation and test evidence. No claim of fresh 100% coverage or deployed behavior.
- A disagreement ledger: proposal position, your position, decisive evidence, what yours gives up.
- Dark reasoning (hostile races/crashes/abuse), pessimistic reasoning (expected operational cost),
  optimistic reasoning (why the chosen design is sufficient if its proofs pass).
- Ranked findings: severity, affected doc/source file:line, concrete counterexample, smallest fix,
  required negative test. Ignore overlap between prior reviewers; report each distinct issue once.
- Explicit gaps left and whether any blocks implementation. Do not reopen owner choices; test
  the selected option's enforcement and state its tradeoffs.
- Exact document edits needed and public-docs implications. No backend implementation.
```

## Limits

This file prepares a prompt; it does not launch another agent or claim review approval.

Astra follow-up: read the [ten-finding disposition ledger](codex-response.md). Attack dispatch issuance/replay/revocation, expired-renewal ordering, candidate version acknowledgement, receipt-less recovery, and pre-mutation build identity. Distinguish revised requirements from demonstrated tests; retain all locked decisions.

## Related pages

- [Review packet](review-packet.md)
- [Owner decisions](decisions.md)
