import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
} from "@usezombie/design-system";

const items = [
  {
    q: "What is UseZombie?",
    a: "A durable runtime for one operational outcome. The platform-ops agent wakes on a GitHub Actions deploy failure, gathers evidence from Fly, Upstash, Redis, and GitHub run logs, and posts an evidenced diagnosis to Slack. Reachable via `zombiectl steer` for manual investigation.",
  },
  {
    q: "What does BYOK mean?",
    a: "Bring Your Own Key. Store your own LLM provider credential — Anthropic, OpenAI, Fireworks (Kimi K2), Together, Groq, Moonshot — and the executor resolves it at the tool bridge. UseZombie marks up zero on inference; you pay your provider directly.",
  },
  {
    q: "What am I actually paying for?",
    a: "Hosted execution. Runs are metered against a credit pool with a $10 starter grant that never expires; the two debit points are event receipt and per-stage execution. Inference cost is yours via BYOK.",
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
    a: "It won't. The runtime layers three independent mechanisms — periodic memory checkpoints, a rolling tool-result window, and stage chunking — so a long incident keeps reasoning past the model's working-memory cap. See concepts/context-lifecycle.",
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
