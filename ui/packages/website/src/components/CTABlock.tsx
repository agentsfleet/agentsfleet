import { Link } from "react-router-dom";
import { Button } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked } from "../analytics/posthog";

export default function CTABlock() {
  return (
    <div className="cta-block">
      <h2>Building agents on usezombie?</h2>
      <p>Stable machine surface via OpenAPI 3.1. Webhook ingest, steer, event streams, approval grants — the same surfaces the human dashboard uses.</p>
      <div className="cta-row">
        <Button asChild>
          <a
            href={DOCS_QUICKSTART_URL}
            target="_blank"
            rel="noopener noreferrer"
            onClick={() => trackNavigationClicked({ source: "agents_cta_docs", surface: "cta_block", target: "docs" })}
          >
            Read quickstart
          </a>
        </Button>
        <Button asChild variant="double-border">
          <Link
            to="/pricing"
            onClick={() => trackNavigationClicked({ source: "agents_cta_pricing", surface: "cta_block", target: "pricing" })}
          >
            View pricing
          </Link>
        </Button>
      </div>
    </div>
  );
}
