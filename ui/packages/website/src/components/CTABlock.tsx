import { Button } from "@usezombie/design-system";
import { DOCS_QUICKSTART_URL } from "../config";
import { trackNavigationClicked } from "../analytics/posthog";

export default function CTABlock() {
  return (
    <div className="cta-block">
      <h2>Wire autonomous agents into the control plane.</h2>
      <p>Use the machine contract, docs, and pricing path without mixing agent traffic into the human launch funnel.</p>
      <div className="cta-row">
        <Button
          to={DOCS_QUICKSTART_URL}
          onClick={() => trackNavigationClicked({ source: "agents_cta_docs", surface: "cta_block", target: "docs" })}
        >
          Read quickstart
        </Button>
        <Button
          to="/pricing"
          variant="double-border"
          onClick={() => trackNavigationClicked({ source: "agents_cta_pricing", surface: "cta_block", target: "pricing" })}
        >
          View pricing
        </Button>
      </div>
    </div>
  );
}
