import type { ReactNode } from "react";
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
    a: "Free to try until July 31, 2026 — see the Pricing section above for the current rate table and trial details. Hosted execution is metered against a credit pool; once the trial closes, two rates apply (a platform-default per-stage rate and a 10× cheaper self-managed rate) and event receipt stays free. Stealth-mode testing rate — will rise post-GA.",
  },
  {
    q: "Does platform-managed cost more per stage than self-managed?",
    a: "Yes — that's the deliberate gradient. Once the free-trial window closes, platform default carries a per-stage rate (usezombie is paying your model provider for tokens) and self-managed is ~10× cheaper per stage (you pay your provider directly). The spread is the friction-reducer: on-ramp on platform without bringing a key, graduate to self-managed once volume justifies it. Current rates on the Pricing section above; usezombie marks up zero on inference in either posture.",
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
      <div className="wrap flex flex-col gap-8">
        <div className="flex flex-col gap-3">
          <SectionLabel className="mb-0">FAQ</SectionLabel>
          <DisplayLG>Common questions</DisplayLG>
        </div>
        <Accordion type="single" collapsible className="max-w-measure">
          {items.map((item, i) => (
            <AccordionItem
              key={i}
              value={`q-${i}`}
              data-testid={`faq-item-${i}`}
              className="border-b border-border"
            >
              <AccordionTrigger
                data-testid={`faq-trigger-${i}`}
                className="font-mono text-body-sm py-4 text-text"
              >
                {item.q}
              </AccordionTrigger>
              <AccordionContent
                data-testid={`faq-answer-${i}`}
                className="font-sans text-body-sm leading-prose text-text-muted pb-4"
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
