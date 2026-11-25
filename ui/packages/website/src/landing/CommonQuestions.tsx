import { useId } from "react";
import { DisplayLG, Section, cn } from "@agentsfleet/design-system";
import { SECTION_SPACING, SITE_CONTAINER, TEXT_LINK } from "../components/site-classes";
import { DOCS_URL } from "../lib/site-links";
import { faq } from "./faq";

export function CommonQuestions() {
  const heading = useId();
  return (
    <Section id="faq" className="block" aria-labelledby={heading}>
      <div className={cn(SITE_CONTAINER, SECTION_SPACING, "grid items-start gap-2xl lg:grid-cols-[1fr_2fr] lg:gap-5xl")}>
        <div className="grid gap-md">
          <DisplayLG id={heading} className="font-title">Common questions</DisplayLG>
          <a href={DOCS_URL} className={cn(TEXT_LINK, "w-fit font-semibold text-pulse")}>
            Read the documentation
          </a>
        </div>
        <div className="grid gap-xl sm:grid-cols-2 sm:gap-2xl">
          {faq.map(({ question, answer }) => (
            <article key={question} className="grid content-start gap-sm border-t border-border pt-lg">
              <h3 className="font-semibold">{question}</h3>
              <p className="text-conversation leading-conversation text-text-muted">{answer}</p>
            </article>
          ))}
        </div>
      </div>
    </Section>
  );
}
