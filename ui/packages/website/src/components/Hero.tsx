import type { ReactNode } from "react";
import { BrandMark, DisplayXL, Section } from "@agentsfleet/design-system";
import { HERO_HEADLINE, PRICING_COPY } from "../lib/marketing-copy";
import type { ExampleJob } from "../lib/example-jobs";
import { JobComposer } from "./JobComposer";
import { SignalField } from "./SignalField";
import { CliQuickstart } from "./CliQuickstart";

type HeroProps = {
  request: string;
  onRequest: (request: string) => void;
  job: ExampleJob;
  onJob: (job: ExampleJob) => void;
  children?: ReactNode;
};

export default function Hero({ children, ...props }: HeroProps) {
  return (
    <Section asChild>
      <section className="entrance-hero" aria-label="Your engineering job" data-testid="hero" data-example={Boolean(children)}>
        <SignalField />
        <div className="wrap entrance-content">
          <div className="job-entrance">
            <div className="hero-heading">
              <BrandMark className="entrance-mark" />
              <DisplayXL data-testid="hero-headline">{HERO_HEADLINE}</DisplayXL>
              <p className="hero-description">Describe a job. Build an agent to handle it.</p>
              <p className="hero-credit ph-no-capture">{PRICING_COPY.status}</p>
            </div>
            {children}
            {!children && <JobComposer {...props} />}
            <CliQuickstart />
          </div>
          <p className="entrance-note">Your fleet is an agent with a job, the tools it needs, and the access you choose.</p>
        </div>
      </section>
    </Section>
  );
}
