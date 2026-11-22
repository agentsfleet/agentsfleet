import { Button, DisplayXL, DisplayLG } from "@agentsfleet/design-system";
import { GITHUB_URL, DOCS_QUICKSTART_URL } from "../lib/site-links";
import { SiteHeader, SiteFooter } from "../landing/SiteNavigation";
import { CommonQuestions } from "../landing/CommonQuestions";
import { ProductDemo } from "../landing/ProductDemo";
import { linkTracking } from "../landing/link-tracking";

type HomeProps = { waitlistUrl: string };

function Workflow() {
  return (
    <section className="how" id="how-it-works" aria-labelledby="how-heading">
      <div className="wrap how-layout">
        <div>
          <DisplayLG id="how-heading">
            How does agentsfleet work in a private network?
          </DisplayLG>
          <p className="how-intro">
            Run it where your logs and code live. You choose the model and what
            each agent can read or change.
          </p>
          <a
            href="https://docs.agentsfleet.net/fleets/overview"
            className="how-guide"
          >
            Read how fleets work
          </a>
        </div>
        <div className="workflow-list">
          <ol>
            <li>
              <strong>Find the delay</strong>
              <p>
                Scout checks Grafana. The slow replies line up with dropped
                connections.
              </p>
            </li>
            <li>
              <strong>Review the fix</strong>
              <p>
                The repair fleet opens a draft pull request with a fix and a
                test. Leah reviews it and ships it.
              </p>
            </li>
            <li>
              <strong>Check the result</strong>
              <p>
                Scout compares reply times before and after the fix. Mira posts
                the numbers in Leah’s thread.
              </p>
            </li>
          </ol>
        </div>
      </div>
    </section>
  );
}
function Pricing({ waitlistUrl }: HomeProps) {
  return (
    <section
      className="closing"
      data-testid="pricing-block"
      id="pricing"
      aria-labelledby="pricing-heading"
    >
      <div className="wrap price-panel">
        <div>
          <DisplayLG id="pricing-heading">
            Start with one support ticket.
          </DisplayLG>
          <p>Run agentsfleet yourself or join the waitlist for early access.</p>
          <div className="actions">
            <Button asChild>
              <a
                data-testid="pricing-cta-early-access"
                className="button"
                href={waitlistUrl}
                {...linkTracking("pricing_signup", "pricing", "signup")}
              >
                Join waitlist
              </a>
            </Button>
            <Button asChild>
              <a
                className="button button-secondary"
                href={DOCS_QUICKSTART_URL}
                {...linkTracking("pricing_quickstart", "pricing", "docs")}
              >
                Read the quickstart
              </a>
            </Button>
          </div>
        </div>
        <div className="price-details">
          <h3>Early access pricing</h3>
          <p>
            Fleet runtime starts at <strong>$0</strong>. If you bring your own
            model key, your provider bills model usage directly.
          </p>
        </div>
      </div>
    </section>
  );
}
export default function Home({ waitlistUrl }: HomeProps) {
  return (
    <div className="marketing-home">
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <SiteHeader waitlistUrl={waitlistUrl} />
      <main id="main" tabIndex={-1}>
        <section className="hero" aria-labelledby="hero-heading">
          <div className="wrap spread">
            <div className="cover-copy">
              <DisplayXL id="hero-heading">
                <span className="condition">“Chat froze again.”</span>
                <span className="promise">Find out why. Check the fix.</span>
              </DisplayXL>
              <p className="hero-intro">
                Tag agentsfleet in Slack. It checks Grafana inside your network.
                After you ship the fix, it tells Support whether replies got
                faster.
              </p>
              <div className="actions">
                <Button asChild>
                  <a
                    className="button"
                    href={waitlistUrl}
                    {...linkTracking("hero_signup", "hero", "signup")}
                  >
                    Join waitlist
                  </a>
                </Button>
                <a
                  className="text-link"
                  href={GITHUB_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  {...linkTracking("hero_github", "hero", "github")}
                >
                  View on GitHub
                </a>
              </div>
            </div>
            <ProductDemo />
          </div>
        </section>
        <Workflow />
        <CommonQuestions />
        <Pricing waitlistUrl={waitlistUrl} />
      </main>
      <SiteFooter />
    </div>
  );
}
