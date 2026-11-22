import type { ReactNode } from "react";
import { DisplayLG } from "@agentsfleet/design-system";
import { faq, type FaqItem } from "./faq";
import { DOCS_URL } from "../lib/site-links";
function FaqAnswer({ item }: { item: FaqItem }) {
  let answer: ReactNode = item.answer;
  if (item.linkText) {
    const [before, after] = item.answer.split(item.linkText);
    answer = (
      <>
        {before}
        <a href={item.href}>{item.linkText}</a>
        {after}
      </>
    );
  }
  return (
    <article className="faq-item">
      <h3>{item.question}</h3>
      <p>{answer}</p>
    </article>
  );
}
export function CommonQuestions() {
  return (
    <section className="answers" id="faq" aria-labelledby="faq-heading">
      <div className="wrap answers-layout">
        <div>
          <DisplayLG id="faq-heading">Common questions</DisplayLG>
          <a className="docs-link" href={DOCS_URL + "/"}>
            Read the documentation
          </a>
        </div>
        <div className="faq">
          {faq.map((item) => (
            <FaqAnswer key={item.question} item={item} />
          ))}
        </div>
      </div>
    </section>
  );
}
