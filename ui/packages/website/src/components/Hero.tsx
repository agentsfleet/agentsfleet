import type { ReactNode } from "react";
import type { ExampleJob } from "../lib/example-jobs";
import { DisplayXL, Section, cn } from "@agentsfleet/design-system";
import { JobComposer } from "./JobComposer";
import { SITE_CONTAINER } from "./site-classes";

type HeroProps = {
  request: string;
  onRequest: (request: string) => void;
  job: ExampleJob;
  onJob: (job: ExampleJob) => void;
  children?: ReactNode;
};

export default function Hero({ children, ...props }: HeroProps) {
  return (
    <Section className="block py-3xl sm:py-4xl" aria-label="Fleet examples" data-testid="hero">
      <div className={SITE_CONTAINER}>
        <div className="mx-auto max-w-chat">
          <div className="mb-2xl grid gap-md">
            <DisplayXL
              data-testid="hero-headline"
              className={cn("font-title text-balance", children ? "text-heading" : "text-fluid-display-lg")}
            >
              Try a fleet conversation
            </DisplayXL>
            {!children && (
              <p className="text-conversation leading-conversation text-text-muted">
                A fleet is an AI agent. Pick an example to see what a first message can start.
              </p>
            )}
          </div>
          {children || <JobComposer {...props} />}
        </div>
      </div>
    </Section>
  );
}
