import type { ReactNode } from "react";
import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
} from "@usezombie/design-system";

const items: { q: string; a: ReactNode }[] = [
  {
    q: "What is UseZombie?",
    a: "A durable runtime for one operational outcome. The platform-ops agent wakes on a GitHub Actions deploy failure, gathers evidence from your infrastructure and run logs, and posts an evidenced diagnosis to Slack. Reachable via `zombiectl steer` for manual investigation.",
  },
  {
    q: "What does BYOK mean?",
    a: "Bring Your Own Key. Store your own LLM provider credential — Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot — and the executor resolves it at the tool bridge. UseZombie marks up zero on inference; you pay your provider directly.",
  },
  {
    q: "What am I actually paying for?",
    a: "Hosted execution. Runs are metered against a credit pool with a $5 starter grant that never expires; the two debit points are event receipt and per-stage execution. Inference cost is yours via BYOK.",
  },
  {
    q: "Can I self-host?",
    a: "Not in v2. v2 ships hosted-only on api.usezombie.com via Clerk OAuth. Self-host arrives in v3 — the runtime is open source today, and the auth substrate plus KMS adapter are the only deployment-specific layers.",
  },
  {
    q: "Which agent hosts work for the install skill?",
    a: "Claude Code, Amp, Codex CLI, and OpenCode — same skill, same prompts in every host. Run `npm install -g @usezombie/zombiectl`, then `/usezombie-install-platform-ops` inside any of them.",
  },
  {
    q: "What if my agent hits the model's context window?",
    a: (
      <>
        It doesn&apos;t lose the thread. UseZombie keeps long incidents coherent through three layers
        working together. The runtime watches three signals — a <strong>tool-result window</strong>,{" "}
        <strong>memory checkpoints</strong>, and a <strong>stage-chunk threshold</strong> — and the
        agent responds by compacting tool results into durable memory via <code>memory_store</code>,
        ending the stage at safe boundaries, and re-entering on a continuation chain (capped at 10)
        for the next stage. Underneath, the agent loop runs its own rolling-summary compaction once
        message count or token budget crosses a built-in threshold. Net: a 40-tool-call deploy
        investigation stays reasoned through to a Slack diagnosis, not a context-overflow loop.{" "}
        <a
          href="https://docs.usezombie.com/concepts/context-lifecycle"
          target="_blank"
          rel="noreferrer"
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
    <div className="section-gap">
      <p className="eyebrow">FAQ</p>
      <h2>Common questions</h2>
      <div style={{ maxWidth: "640px" }}>
        <Accordion type="single" collapsible>
          {items.map((item, i) => (
            <AccordionItem
              key={i}
              value={`q-${i}`}
              data-testid={`faq-item-${i}`}
            >
              <AccordionTrigger data-testid={`faq-trigger-${i}`}>
                {item.q}
              </AccordionTrigger>
              <AccordionContent data-testid={`faq-answer-${i}`}>
                {item.a}
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </div>
    </div>
  );
}
