import { List, ListItem } from "@usezombie/design-system";

/*
 * Terms — single-column long-form prose. Per DESIGN_SYSTEM.md §Layout
 * docs: ~68ch measure, Commit Mono headings, Instrument Sans body.
 */
export default function Terms() {
  return (
    <article
      data-testid="terms-page"
      className="wrap site-section flex flex-col gap-6 max-w-[68ch] font-sans text-[15px] leading-[1.7] text-text"
    >
      <p className="font-mono text-[12px] uppercase tracking-[0.1em] text-text-muted m-0">
        legal
      </p>
      <h1 className="font-mono text-[clamp(36px,5vw,52px)] leading-[1.05] tracking-[-0.025em] font-medium text-text m-0">
        Terms of Service
      </h1>
      <p className="font-mono text-[12px] text-text-muted m-0">Last updated: May 4, 2026</p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">1. Acceptance</h2>
      <p className="text-text-muted m-0">
        By accessing or using usezombie (&quot;the Service&quot;), you agree to these Terms of Service.
        If you do not agree, do not use the Service.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">2. Service description</h2>
      <p className="text-text-muted m-0">
        usezombie is an agent delivery control plane that processes specification queues into
        validated pull requests. The Service operates on your Git repositories using branch-based
        state and BYOK (Bring Your Own Keys) model access.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">3. Your responsibilities</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>You are responsible for your LLM API keys and any costs incurred with your providers.</ListItem>
        <ListItem>You must not use the Service to generate malicious code, violate third-party rights, or circumvent security controls.</ListItem>
        <ListItem>You are responsible for the content of specifications submitted to the pipeline.</ListItem>
        <ListItem>You must maintain the security of your authentication credentials.</ListItem>
      </List>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">4. Billing</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>usezombie charges for agent compute time (per second of wall-clock time).</ListItem>
        <ListItem>A one-time workspace activation fee of $5 applies to each new workspace.</ListItem>
        <ListItem>LLM token costs are paid directly to your provider — usezombie never marks up tokens.</ListItem>
        <ListItem>Billing cycles are monthly. Invoices are issued at the start of each billing period.</ListItem>
      </List>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">5. Intellectual property</h2>
      <p className="text-text-muted m-0">
        You retain all rights to your source code, specifications, and generated artifacts.
        usezombie claims no ownership over outputs produced by the pipeline.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">6. Limitation of liability</h2>
      <p className="text-text-muted m-0">
        usezombie is provided &quot;as is&quot; without warranty. We are not liable for damages arising
        from agent-generated code, pipeline failures, or third-party service outages. Enterprise
        tier customers may negotiate contractual SLAs.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">7. Termination</h2>
      <p className="text-text-muted m-0">
        Either party may terminate at any time. Upon termination, workspace data is retained
        for 30 days, after which it is permanently deleted.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">8. Contact</h2>
      <p className="text-text-muted m-0">
        For questions about these terms, contact{" "}
        <a href="mailto:legal@usezombie.com" className="text-pulse hover:underline">
          legal@usezombie.com
        </a>
        .
      </p>
    </article>
  );
}
