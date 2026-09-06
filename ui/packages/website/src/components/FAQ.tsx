import type { ReactNode } from "react";
import {
  Accordion,
  AccordionItem,
  AccordionTrigger,
  AccordionContent,
  DisplayLG,
  SectionLabel,
} from "@agentsfleet/design-system";
import { DOCS_URL } from "../config";
import { FAQ_WEDGE_ITEM, PRICING_COPY } from "../lib/marketing-copy";

const items: { q: string; a: ReactNode }[] = [
  FAQ_WEDGE_ITEM,
  {
    q: "What is agentsfleet?",
    a: "agentsfleet gives your engineering team AI teammates for code review, incident investigation, and preparing fixes. A fleet is a teammate you configure for a job. You choose what it can access and review the results.",
  },
  {
    q: "What does self-managed mean?",
    a: "A self-managed provider key is your own model-provider credential, stored in the vault. You pay that provider directly for model usage. Fleet runtime is separate.",
  },
  {
    q: "What am I actually paying for?",
    a: `${PRICING_COPY.runtime} ${PRICING_COPY.models} ${PRICING_COPY.note}`,
  },
  {
    q: "Does bringing my own model key make agentsfleet free?",
    a: `No. ${PRICING_COPY.models} ${PRICING_COPY.note}`,
  },
  {
    q: "Can I self-host?",
    a: "Not currently. agentsfleet is hosted today. Self-hosting is planned for a future release.",
  },
  {
    q: "Which coding agents work for the install skill?",
    a: "Claude Code, Amp, Codex Command-Line Interface (CLI), and OpenCode — same skill, same prompts in every host. Run npm install -g @agentsfleet/cli, then /agentsfleet-install-platform-ops inside any of them.",
  },
  {
    q: "What if my Fleet hits the model's context window?",
    a: (
      <>
        A fleet can save useful findings and summarize earlier work before continuing.
        Continuation stays subject to its budget and runtime limits. You can inspect the run history and saved memory.{" "}
        <a
          href={`${DOCS_URL}/concepts/context-lifecycle`}
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
                className="font-sans text-body-sm py-4 text-text"
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
