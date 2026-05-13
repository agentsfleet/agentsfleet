import { DisplayXL, List, ListItem, SectionLabel } from "@usezombie/design-system";
import { SUPPORT_EMAIL } from "../lib/contact";
import { RATES_DISPLAY } from "../lib/rates";

/*
 * Terms — single-column long-form prose. Per DESIGN_SYSTEM.md §Layout
 * docs: ~68ch measure, Commit Mono headings, Instrument Sans body.
 */
export default function Terms() {
  return (
    <article
      data-testid="terms-page"
      className="wrap site-section flex flex-col gap-6 max-w-prose font-sans text-body leading-prose text-text"
    >
      <SectionLabel className="mb-0">legal</SectionLabel>
      <DisplayXL className="text-fluid-display-lg">Terms of Service</DisplayXL>
      <p className="font-mono text-eyebrow text-text-muted m-0">Last updated: May 4, 2026</p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">1. Acceptance</h2>
      <p className="text-text-muted m-0">
        By accessing or using usezombie (&quot;the Service&quot;), you agree to these Terms of Service.
        If you do not agree, do not use the Service.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">2. Service description</h2>
      <p className="text-text-muted m-0">
        usezombie is an agent delivery control plane that processes specification queues into
        validated pull requests. The Service operates on your Git repositories using branch-based
        state and self-managed (self-managed provider keys) model access.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">3. Your responsibilities</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>You are responsible for your LLM API keys and any costs incurred with your providers.</ListItem>
        <ListItem>You must not use the Service to generate malicious code, violate third-party rights, or circumvent security controls.</ListItem>
        <ListItem>You are responsible for the content of specifications submitted to the pipeline.</ListItem>
        <ListItem>You must maintain the security of your authentication credentials.</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">4. Billing</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>usezombie charges {RATES_DISPLAY.EVENT_RATE} per event receipt and {RATES_DISPLAY.STAGE_PLATFORM} per stage execution on platform default ({RATES_DISPLAY.STAGE_SELF_MANAGED} per stage on self-managed). Each new account receives a {RATES_DISPLAY.STARTER_CREDIT} starter credit that never expires. Stealth-mode testing rate — will rise post-GA.</ListItem>
        <ListItem>Hosted execution is metered against a credit pool. Debits fire before each stage execution; event receipt is currently free.</ListItem>
        <ListItem>LLM token costs are paid directly to your provider — usezombie never marks up tokens.</ListItem>
        <ListItem>Once your credit pool is exhausted, additional usage requires a top-up via the Dashboard.</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">5. Intellectual property</h2>
      <p className="text-text-muted m-0">
        You retain all rights to your source code, specifications, and generated artifacts.
        usezombie claims no ownership over outputs produced by the pipeline.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">6. Limitation of liability</h2>
      <p className="text-text-muted m-0">
        usezombie is provided &quot;as is&quot; without warranty. We are not liable for damages arising
        from agent-generated code, pipeline failures, or third-party service outages. Enterprise
        tier customers may negotiate contractual SLAs.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">7. Termination</h2>
      <p className="text-text-muted m-0">
        Either party may terminate at any time. Upon termination, workspace data is retained
        for 30 days, after which it is permanently deleted.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">8. Contact</h2>
      <p className="text-text-muted m-0">
        For questions about these terms, contact{" "}
        <a href={`mailto:${SUPPORT_EMAIL}`} className="text-pulse hover:underline">
          {SUPPORT_EMAIL}
        </a>
        .
      </p>
    </article>
  );
}
