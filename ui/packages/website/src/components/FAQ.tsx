import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
} from "@usezombie/design-system";

const items = [
  {
    q: "What does BYOK mean?",
    a: "Bring Your Own Keys. You provide your own LLM API keys (Anthropic, OpenAI, Google, etc.). UseZombie never resells model tokens or marks them up. You pay your provider directly for token usage.",
  },
  {
    q: "What am I actually paying for?",
    a: "UseZombie charges for hosted agent execution time — per agent-second of wall-clock time. Model usage is billed directly by your provider through BYOK, so there is no token markup.",
  },
  {
    q: "Can I try it before committing?",
    a: "Yes. Hobby includes one workspace and $10 in credit with no expiry. No credit card required. Move to Scale when you need shared workspaces, stronger governance, and deeper quality visibility.",
  },
  {
    q: "What happens to my code?",
    a: "UseZombie operates on your Git repos via branch-based state. Code stays in your repositories. Artifacts and run transitions are stored with full audit trails you can export.",
  },
  {
    q: "Which Git providers are supported?",
    a: "GitHub is fully supported in v1. GitLab and Bitbucket support are on the roadmap.",
  },
  {
    q: "Is there an SLA?",
    a: "Scale includes priority support and stronger execution boundaries. Contractual SLAs are available for larger deployments.",
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
