import type { ReactNode } from "react";
import { BrandMark, DisplayXL, cn } from "@agentsfleet/design-system";
import { HERO_HEADLINE, PRICING_COPY } from "../lib/marketing-copy";
import type { ExampleJob } from "../lib/example-jobs";
import { JobComposer } from "./JobComposer";
import { CliQuickstart } from "./CliQuickstart";
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
    <section
      className="bg-radial-[at_50%_0%] from-pulse/10 via-background to-background py-3xl sm:py-4xl"
      aria-label="Your engineering job"
      data-testid="hero"
    >
      <div className={SITE_CONTAINER}>
        <div className="mx-auto max-w-measure">
          <div
            className={cn(
              "grid justify-items-center gap-lg text-center",
              children ? "pb-2xl" : "pb-3xl",
            )}
          >
            {!children && <BrandMark className="size-10 text-pulse" />}
            <DisplayXL
              data-testid="hero-headline"
              className={cn(
                "font-bold text-balance",
                children ? "text-heading" : "text-fluid-display-lg",
              )}
            >
              {HERO_HEADLINE}
            </DisplayXL>
            {!children && (
              <p className="text-body-lg leading-body-lg text-text-muted">
                Describe a job. Build an agent to handle it.
              </p>
            )}
            {!children && (
              <p className="text-body-sm text-pulse ph-no-capture">
                {PRICING_COPY.status}
              </p>
            )}
          </div>
          {children || <JobComposer {...props} />}
          <CliQuickstart />
          <p className="mt-xl text-center text-label text-text-muted">
            Your fleet is an agent with a job, the tools it needs, and the
            access you choose.
          </p>
        </div>
      </div>
    </section>
  );
}
