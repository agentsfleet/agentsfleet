import type { ReactNode } from "react";
import { DisplayLG, cn } from "@agentsfleet/design-system";
import {
  SECTION_SPACING,
  SITE_CONTAINER,
  TEXT_LINK,
} from "../components/site-classes";
import { DOCS_URL } from "../lib/site-links";
import { faq, type FaqItem } from "./faq";

function FaqAnswer({ item }: { item: FaqItem }) {
  let answer: ReactNode = item.answer;
  if (item.linkText) {
    const [before, after] = item.answer.split(item.linkText);
    answer = (
      <>
        {before}
        <a
          href={item.href}
          className={cn(TEXT_LINK, "font-semibold text-pulse")}
        >
          {item.linkText}
        </a>
        {after}
      </>
    );
  }
  return (
    <article className="grid gap-sm border-t border-border pt-lg">
      <h3 className="font-bold">{item.question}</h3>
      <p className="text-text-muted">{answer}</p>
    </article>
  );
}

export function CommonQuestions() {
  return (
    <section
      id="faq"
      data-theme="light"
      aria-labelledby="faq-heading"
      className={cn(SECTION_SPACING, "bg-background text-text")}
    >
      <div
        className={cn(
          SITE_CONTAINER,
          "grid items-start gap-2xl lg:grid-cols-[minmax(12rem,0.65fr)_minmax(0,1.35fr)] lg:gap-4xl",
        )}
      >
        <div className="grid gap-lg">
          <DisplayLG id="faq-heading" className="font-bold">
            Common questions
          </DisplayLG>
          <a
            href={DOCS_URL}
            className={cn(TEXT_LINK, "w-fit font-semibold text-pulse")}
          >
            Read the documentation
          </a>
        </div>
        <div className="grid gap-x-2xl gap-y-xl md:grid-cols-2">
          {faq.map((item) => (
            <FaqAnswer key={item.question} item={item} />
          ))}
        </div>
      </div>
    </section>
  );
}
