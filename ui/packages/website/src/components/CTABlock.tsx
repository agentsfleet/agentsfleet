import { Button, DisplayLG } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked } from "../analytics/posthog";

/*
 * CTABlock — restrained closing CTA. Mono headline, sans lede, two-button
 * row (primary install + ghost pricing). No animated shimmer pill.
 */
export default function CTABlock() {
  return (
    <section className="site-section" data-testid="cta-block">
      <div className="wrap flex flex-col gap-6 max-w-[720px]">
        <DisplayLG>Building agents on usezombie?</DisplayLG>
        <p className="font-sans text-[16px] leading-[1.6] text-text-muted m-0">
          Stable machine surface via OpenAPI 3.1. Webhook ingest, steer, event
          streams, approval grants — the same surfaces the human dashboard uses.
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
    </section>
  );
}
