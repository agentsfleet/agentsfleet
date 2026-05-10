import type { ReactNode } from "react";
import { RATES_DISPLAY } from "../lib/rates";
import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
  DisplayLG,
  SectionLabel,
} from "@usezombie/design-system";

const items: { q: string; a: ReactNode }[] = [
  {
    q: "What is usezombie?",
    a: "A durable runtime for one operational outcome. The platform-ops agent wakes on a GitHub Actions deploy failure, gathers evidence from your infrastructure and run logs, and posts an evidenced diagnosis to Slack. Reachable via zombiectl steer for manual investigation.",
  },
  {
    q: "What does self-managed mean?",
    a: "self-managed provider key. Store your own LLM provider credential — Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot — and the executor resolves it at the tool bridge. usezombie marks up zero on inference; you pay your provider directly.",
  },
  {
    q: "What am I actually paying for?",
    a: `Hosted execution. Runs are metered against a credit pool with a ${RATES_DISPLAY.starterCredit} starter grant that never expires. Two debit points per event: ${RATES_DISPLAY.eventPlatform} on receipt (after the balance gate passes), then ${RATES_DISPLAY.stage} before each stage the agent runs — every wake-up and every reasoning step is line-itemed. A typical 3-stage incident is ${RATES_DISPLAY.eventPlatform} + 3 × ${RATES_DISPLAY.stage} = $0.31. Inference cost is yours via self-managed key; usezombie marks up zero.`,
  },
  {
    q: "Does platform-managed inference cost more per stage?",
    a: `Same headline rate today. Platform-managed stage cost is structured as ${RATES_DISPLAY.stage} overhead plus a token component, but the pre-execution charge uses a 100/100-token floor estimate that rounds to 0¢ against every current model rate — so both postures bill ${RATES_DISPLAY.stage} per stage in practice. Token-level reconciliation (heavy stages truing up against actual usage) lands when Stripe wires in. Until then: self-managed pays ${RATES_DISPLAY.stage} flat per stage and your provider bills the inference; platform-managed pays ${RATES_DISPLAY.stage} flat per stage and we bill the inference at provider rates with zero markup.`,
  },
  {
    q: "What does 'extras provisioned per workspace' mean?",
    a: "Operational features — multi-workspace, approval gating, workspace-scoped credentials and webhooks, higher concurrency, longer per-stage windows, priority support — turn on per workspace as you scale, not as paywalled tier upgrades. Need bigger caps? Ask; we lift them.",
  },
  {
    q: "Can I self-host?",
    a: "Not in v2. v2 ships hosted-only on api.usezombie.com via Clerk OAuth. Self-host arrives in v3 — the runtime is open source today, and the auth substrate plus KMS adapter are the only deployment-specific layers.",
  },
  {
    q: "Which agent hosts work for the install skill?",
    a: "Claude Code, Amp, Codex CLI, and OpenCode — same skill, same prompts in every host. Run npm install -g @usezombie/zombiectl, then /usezombie-install-platform-ops inside any of them.",
  },
  {
    q: "What if my agent hits the model's context window?",
    a: (
      <>
        It doesn&apos;t lose the thread. usezombie keeps long incidents coherent through three layers
        working together. The runtime watches three signals — a <strong>tool-result window</strong>,{" "}
        <strong>memory checkpoints</strong>, and a <strong>stage-chunk threshold</strong> — and the
        agent responds by compacting tool results into durable memory via{" "}
        <code className="font-mono">memory_store</code>, ending the stage at safe boundaries, and
        re-entering on a continuation chain (capped at 10) for the next stage. Underneath, the agent
        loop runs its own rolling-summary compaction once message count or token budget crosses a
        built-in threshold. Net: a 40-tool-call deploy investigation stays reasoned through to a
        Slack diagnosis, not a context-overflow loop.{" "}
        <a
          href="https://docs.usezombie.com/concepts/context-lifecycle"
          target="_blank"
          rel="noreferrer"
          className="text-pulse hover:border-b hover:border-pulse"
        >
          Read more in the context lifecycle docs
        </a>
        .
      </>
    ),
  },
];

export default function FAQ() {
  return (
    <section className="site-section" data-testid="faq">
      <div className="wrap flex flex-col gap-8 max-w-[720px]">
        <div className="flex flex-col gap-3">
          <SectionLabel className="mb-0">FAQ</SectionLabel>
          <DisplayLG>Common questions</DisplayLG>
        </div>
        <Accordion type="single" collapsible>
          {items.map((item, i) => (
            <AccordionItem
              key={i}
              value={`q-${i}`}
              data-testid={`faq-item-${i}`}
              className="border-b border-border"
            >
              <AccordionTrigger
                data-testid={`faq-trigger-${i}`}
                className="font-mono text-[14px] py-4 text-text"
              >
                {item.q}
              </AccordionTrigger>
              <AccordionContent
                data-testid={`faq-answer-${i}`}
                className="font-sans text-[14px] leading-[1.65] text-text-muted pb-4"
              >
                {item.a}
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </div>
    </section>
  );
}
