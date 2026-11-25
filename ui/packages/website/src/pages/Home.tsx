import { useId } from "react";
import { Button, DisplayLG, DisplayXL, List, ListItem, Section, cn } from "@agentsfleet/design-system";
import { ArrowRightIcon } from "@phosphor-icons/react/dist/ssr/ArrowRight";
import { ChatCircleTextIcon } from "@phosphor-icons/react/dist/ssr/ChatCircleText";
import { ClockIcon } from "@phosphor-icons/react/dist/ssr/Clock";
import { KeyIcon } from "@phosphor-icons/react/dist/ssr/Key";
import { linkTracking } from "../analytics/link-tracking";
import { Pricing } from "../components/Pricing";
import { SiteLayout } from "../components/SiteLayout";
import { SECTION_SPACING, SITE_CONTAINER } from "../components/site-classes";
import { CommonQuestions } from "../landing/CommonQuestions";
import { ProductDemo } from "../landing/ProductDemo";
import { FIRST_MESSAGE_FLOW, LOOP_ANCHOR_ID } from "../lib/marketing-copy";

type HomeProps = { waitlistUrl: string };

const WORKFLOW_ICONS = [ChatCircleTextIcon, KeyIcon, ClockIcon] as const;

export default function Home({ waitlistUrl }: HomeProps) {
  return (
    <SiteLayout waitlistUrl={waitlistUrl} landing>
      <Hero waitlistUrl={waitlistUrl} />
      <Workflow />
      <CommonQuestions />
      <Pricing waitlistUrl={waitlistUrl} />
    </SiteLayout>
  );
}

function Hero({ waitlistUrl }: HomeProps) {
  const heading = useId();
  return (
    <Section className="block" aria-labelledby={heading}>
      <div className={cn(SITE_CONTAINER, "grid items-center gap-3xl pt-4xl pb-5xl lg:grid-cols-2 lg:gap-5xl lg:py-6xl")}>
        <div className="grid gap-xl sm:gap-2xl">
          <DisplayXL id={heading} className="font-bold text-balance">
            Start a fleet with one message.
          </DisplayXL>
          <p className="max-w-narrow text-heading leading-heading text-text-muted">
            A fleet is an AI agent. Ask it to watch for events, run on a schedule, or keep working on a task after you leave.
          </p>
          <div className="flex flex-wrap items-center gap-md">
            <Button asChild wrap className="min-h-11">
              <a
                href={waitlistUrl}
                target="_blank"
                rel="noopener noreferrer"
                {...linkTracking("hero_signup", "hero", "signup")}
              >
                Join waitlist <ArrowRightIcon aria-hidden="true" />
              </a>
            </Button>
            <Button asChild wrap variant="ghost" className="min-h-11">
              <a href="/examples" {...linkTracking("hero_examples", "hero", "examples")}>
                See examples
              </a>
            </Button>
          </div>
        </div>
        <ProductDemo />
      </div>
    </Section>
  );
}

function Workflow() {
  const heading = useId();
  return (
    <Section variant="divided" id={LOOP_ANCHOR_ID} aria-labelledby={heading} className="scroll-mt-xl border-y border-border bg-card">
      <div className={cn(SITE_CONTAINER, SECTION_SPACING, "grid gap-2xl sm:gap-3xl")}>
        <DisplayLG id={heading} className="font-title">How fleets work</DisplayLG>
        <List variant="ordered" className="grid list-none gap-2xl space-y-0 pl-0 lg:grid-cols-3 lg:gap-4xl">
          {FIRST_MESSAGE_FLOW.map(({ title, description }, index) => {
            const Icon = WORKFLOW_ICONS[index];
            return (
              <ListItem key={title} className="grid content-start gap-md border-t border-border pt-xl">
                <Icon aria-hidden="true" className="size-5 text-pulse" />
                <h3 className="text-heading font-title">{title}</h3>
                <p className="max-w-prose text-conversation leading-conversation text-text-muted">{description}</p>
              </ListItem>
            );
          })}
        </List>
      </div>
    </Section>
  );
}
