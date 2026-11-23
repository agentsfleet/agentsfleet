import {
  Button,
  Card,
  DisplayXL,
  DisplayLG,
  cn,
} from "@agentsfleet/design-system";
import { linkTracking } from "../analytics/link-tracking";
import { SiteLayout } from "../components/SiteLayout";
import {
  SECTION_SPACING,
  SITE_CONTAINER,
  TEXT_LINK,
} from "../components/site-classes";
import { GITHUB_URL, DOCS_QUICKSTART_URL } from "../lib/site-links";
import { LOOP_STEPS } from "../lib/marketing-copy";
import { CommonQuestions } from "../landing/CommonQuestions";
import { ProductDemo } from "../landing/ProductDemo";

type HomeProps = { waitlistUrl: string };

function Workflow() {
  return (
    <section
      id="how-it-works"
      aria-labelledby="how-heading"
      className="scroll-mt-xl border-y border-border bg-card"
    >
      <div
        className={cn(
          SITE_CONTAINER,
          SECTION_SPACING,
          "grid items-start gap-2xl lg:grid-cols-[minmax(0,0.85fr)_minmax(0,1fr)] lg:gap-5xl",
        )}
      >
        <div className="grid gap-lg">
          <DisplayLG
            id="how-heading"
            className="max-w-tagline font-bold text-balance"
          >
            How does agentsfleet work in a private network?
          </DisplayLG>
          <p className="max-w-narrow text-body-lg leading-body-lg text-text-muted">
            Run it where your logs and code live. You choose the model and what
            each agent can read or change.
          </p>
          <a
            href="https://docs.agentsfleet.net/fleets/overview"
            className={cn(TEXT_LINK, "w-fit font-semibold text-pulse")}
          >
            Read how fleets work
          </a>
        </div>
        <ol className="divide-y divide-border border-y border-border">
          {LOOP_STEPS.map(({ title, description }) => (
            <li key={title} className="grid gap-sm py-xl">
              <strong className="font-bold">{title}</strong>
              <p className="text-text-muted">{description}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

function Pricing({ waitlistUrl }: HomeProps) {
  return (
    <section
      id="pricing"
      data-testid="pricing-block"
      aria-labelledby="pricing-heading"
      className={cn(SECTION_SPACING, "scroll-mt-xl")}
    >
      <div className={SITE_CONTAINER}>
        <Card
          asChild
          className="grid grid-cols-1 gap-2xl p-xl sm:p-3xl lg:grid-cols-[minmax(0,1.25fr)_minmax(0,0.75fr)] lg:items-center lg:gap-4xl"
        >
          <div>
            <div className="grid gap-lg">
              <DisplayLG
                id="pricing-heading"
                className="font-bold text-balance"
              >
                Start with one support ticket.
              </DisplayLG>
              <p className="text-body-lg leading-body-lg text-text-muted">
                Run agentsfleet yourself or join the waitlist for early access.
              </p>
              <div className="flex flex-wrap items-center gap-md">
                <Button asChild className="min-h-11">
                  <a
                    href={waitlistUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    data-testid="pricing-cta-early-access"
                    {...linkTracking("pricing_signup", "pricing", "signup")}
                  >
                    Join waitlist
                  </a>
                </Button>
                <Button wrap asChild variant="outline" className="min-h-11">
                  <a
                    href={DOCS_QUICKSTART_URL}
                    {...linkTracking("pricing_quickstart", "pricing", "docs")}
                  >
                    Read the quickstart
                  </a>
                </Button>
              </div>
            </div>
            <div className="grid gap-md border-t border-border pt-xl lg:border-t-0 lg:border-l lg:pt-0 lg:pl-2xl">
              <h3 className="font-bold">Early access pricing</h3>
              <p className="text-text-muted">
                Fleet runtime starts at{" "}
                <strong className="text-text">$0</strong>. If you bring your own
                model key, your provider bills model usage directly.
              </p>
            </div>
          </div>
        </Card>
      </div>
    </section>
  );
}

function Hero({ waitlistUrl }: HomeProps) {
  return (
    <section
      aria-labelledby="hero-heading"
      className="bg-radial-[at_82%_5%] from-pulse/12 via-background to-background"
    >
      <div
        className={cn(
          SITE_CONTAINER,
          SECTION_SPACING,
          "grid items-start gap-3xl lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] lg:gap-3xl lg:motion-safe:items-center",
        )}
      >
        <div className="grid items-start gap-xl">
          <DisplayXL
            id="hero-heading"
            className="grid gap-lg font-extrabold text-balance"
          >
            <span className="text-heading font-semibold tracking-normal text-text-muted">
              “Chat froze again.”
            </span>
            <span>Find out why. Check the fix.</span>
          </DisplayXL>
          <p className="max-w-narrow text-body-lg leading-body-lg text-text-muted">
            Tag agentsfleet in Slack. It checks Grafana inside your network.
            After you ship the fix, it tells Support whether replies got faster.
          </p>
          <div className="flex flex-wrap items-center gap-x-xl gap-y-md">
            <Button asChild className="min-h-11">
              <a
                href={waitlistUrl}
                target="_blank"
                rel="noopener noreferrer"
                {...linkTracking("hero_signup", "hero", "signup")}
              >
                Join waitlist
              </a>
            </Button>
            <a
              href={GITHUB_URL}
              target="_blank"
              rel="noopener noreferrer"
              className={cn(
                TEXT_LINK,
                "inline-flex min-h-11 items-center font-semibold",
              )}
              {...linkTracking("hero_github", "hero", "github")}
            >
              View on GitHub
            </a>
          </div>
        </div>
        <ProductDemo />
      </div>
    </section>
  );
}

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
