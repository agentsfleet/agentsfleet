import { Button, DisplayLG } from "@agentsfleet/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked } from "../analytics/posthog";

/*
 * CTABlock — restrained closing CTA on the human landing page. Mono headline,
 * sans lede, two-button row (quickstart + pricing). Speaks to the operator who
 * landed here, not the machine API — that pitch lives on /agents.
 */
export default function CTABlock() {
  return (
    <section className="site-section" data-testid="cta-block">
      <div className="wrap flex flex-col gap-6">
        <DisplayLG>Stop chasing failed deploys.</DisplayLG>
        <div className="flex flex-col gap-6 max-w-measure">
          <p className="font-sans text-body-lg leading-body text-text-muted m-0">
            Install one agent, wire one webhook — it wakes on the failure, reads
            the logs, and posts the diagnosis with evidence. You read the answer,
            not another dashboard.
          </p>
          <div className="flex flex-wrap gap-3 items-center">
            <Button asChild>
              <a
                href={DOCS_QUICKSTART_URL}
                target="_blank"
                rel="noopener noreferrer"
                onClick={() =>
                  trackNavigationClicked({ source: "agents_cta_docs", surface: "cta_block", target: "docs" })
                }
              >
                → read quickstart
              </a>
            </Button>
            <Button asChild variant="ghost">
              <a
                href="/#pricing"
                onClick={() =>
                  trackNavigationClicked({ source: "agents_cta_pricing", surface: "cta_block", target: "pricing" })
                }
              >
                view pricing
              </a>
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
