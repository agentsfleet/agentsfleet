import { DisplayXL, List, ListItem, SectionLabel } from "@agentsfleet/design-system";
import { SUPPORT_EMAIL } from "../lib/contact";
import { PRICING_COPY } from "../lib/marketing-copy";

/* Terms — single-column long-form prose with technical metadata in mono. */
export default function Terms() {
  return (
    <article
      data-testid="terms-page"
      className="wrap site-section flex flex-col gap-6 max-w-prose font-sans text-body leading-prose text-text"
    >
      <SectionLabel className="mb-0">legal</SectionLabel>
      <DisplayXL className="text-fluid-display-lg">Terms of Service</DisplayXL>
      <p className="font-mono text-eyebrow text-text-muted m-0">Last updated: June 2, 2026</p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">1. Acceptance</h2>
      <p className="text-text-muted m-0">
        By accessing or using agentsfleet (&quot;the Service&quot;), you agree to these Terms of Service.
        If you do not agree, do not use the Service.
      </p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">2. Service description</h2>
      <p className="text-text-muted m-0">
        agentsfleet is a Fleet delivery control plane that processes specification queues into
        validated pull requests. The Service operates on your Git repositories using branch-based
        state and self-managed (self-managed provider keys) model access.
      </p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">3. Your responsibilities</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>You are responsible for your LLM API keys and any costs incurred with your providers.</ListItem>
        <ListItem>You must not use the Service to generate malicious code, violate third-party rights, or circumvent security controls.</ListItem>
        <ListItem>You are responsible for the content of specifications submitted to the pipeline.</ListItem>
        <ListItem>You must maintain the security of your authentication credentials.</ListItem>
      </List>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">4. Billing</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>{PRICING_COPY.note}</ListItem>
        <ListItem>{PRICING_COPY.runtime}</ListItem>
        <ListItem>{PRICING_COPY.models}</ListItem>
      </List>

      <LiabilityAndContact />
    </article>
  );
}

function LiabilityAndContact() {
  return (
    <>
      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">5. Intellectual property</h2>
      <p className="text-text-muted m-0">
        You retain all rights to your source code, specifications, and generated artifacts.
        agentsfleet claims no ownership over outputs produced by the pipeline.
      </p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">6. Limitation of liability</h2>
      <p className="text-text-muted m-0">
        agentsfleet is provided &quot;as is&quot; without warranty. We are not liable for damages arising
        from Fleet-generated code, pipeline failures, or third-party service outages.
      </p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">7. Termination</h2>
      <p className="text-text-muted m-0">
        Either party may terminate at any time. Upon termination, workspace data is retained
        for 30 days, after which it is permanently deleted.
      </p>

      <h2 className="font-sans text-heading mt-6 mb-0 font-medium">8. Contact</h2>
      <p className="text-text-muted m-0">
        For questions about these terms, contact{" "}
        <a href={`mailto:${SUPPORT_EMAIL}`} className="text-pulse hover:underline">
          {SUPPORT_EMAIL}
        </a>
        .
      </p>
    </>
  );
}
