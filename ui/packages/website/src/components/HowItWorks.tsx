import { Fragment, type ReactNode } from "react";
import { Card, DisplayLG, SectionLabel } from "@agentsfleet/design-system";
import {
  HOW_IT_WORKS_FOOTNOTE,
  HOW_IT_WORKS_HEADING,
  LOOP_STEPS,
} from "../lib/marketing-copy";

/*
 * HowItWorks — an opinionated three-beat flow instead of an abstract
 * eight-step list. The concrete Auto Reviewer path reads left-to-right
 * (PR -> review -> Slack), each beat carrying a minimal mint line-glyph, with
 * mint connectors between: an arrow on desktop, a down-caret when the cards
 * stack on mobile. Pictorial but restrained — line icons, no heavy art.
 */

// One minimal line-glyph per beat, mint via currentColor. Index-aligned with
// LOOP_STEPS: pull request -> review bubble -> Slack heads-up.
const STEP_GLYPHS: readonly ReactNode[] = [
  // pull request
  <svg key="pr" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="6" cy="6" r="2.25" />
    <circle cx="6" cy="18" r="2.25" />
    <circle cx="18" cy="18" r="2.25" />
    <path d="M6 8.25v7.5" />
    <path d="M18 15.75V12a3 3 0 0 0-3-3h-2.5" />
    <path d="m14 7 -1.5 2 1.5 2" />
  </svg>,
  // review bubble with check
  <svg key="review" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M4 5h16v11H8l-4 4z" />
    <path d="m9 10 2 2 4-4" />
  </svg>,
  // Slack heads-up (bell)
  <svg key="slack" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M6 9a6 6 0 0 1 12 0c0 4.5 1.5 5.5 1.5 5.5H4.5S6 13.5 6 9z" />
    <path d="M10 18.5a2 2 0 0 0 4 0" />
  </svg>,
];

export default function HowItWorks() {
  return (
    <section className="site-section" aria-label="How it works" data-testid="how-it-works">
      <div className="wrap flex flex-col gap-8">
        <div className="flex flex-col gap-3">
          <SectionLabel className="mb-0">How it works</SectionLabel>
          <DisplayLG className="max-w-narrow">
            {HOW_IT_WORKS_HEADING}
          </DisplayLG>
        </div>
        <div className="flex flex-col items-stretch gap-4 lg:flex-row">
          {LOOP_STEPS.map((step, i) => (
            <Fragment key={step.number}>
              <Card
                className="flex flex-1 flex-col gap-3"
                data-testid={`how-step-${step.number}`}
              >
                <div className="flex items-center justify-between">
                  <span className="inline-flex size-7 text-pulse" aria-hidden="true">
                    {STEP_GLYPHS[i]}
                  </span>
                  <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-pulse">
                    {step.number}
                  </span>
                </div>
                <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
                  {step.title}
                </h3>
                <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                  {step.description}
                </p>
              </Card>
              {i < LOOP_STEPS.length - 1 ? (
                <div
                  aria-hidden="true"
                  className="flex items-center justify-center font-mono text-heading text-pulse lg:px-1"
                >
                  <span className="lg:hidden">↓</span>
                  <span className="hidden lg:inline">→</span>
                </div>
              ) : null}
            </Fragment>
          ))}
        </div>
        <p className="font-sans text-body-sm leading-body text-text-subtle m-0 max-w-narrow">
          {HOW_IT_WORKS_FOOTNOTE}
        </p>
      </div>
    </section>
  );
}
