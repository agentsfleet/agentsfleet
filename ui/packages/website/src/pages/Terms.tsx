export default function Terms() {
  return (
    <section className="stack legal-page route-fade">
      <p className="eyebrow">legal</p>
      <h1>Terms of Service</h1>
      <p className="lead">Last updated: May 4, 2026</p>

      <h2>1. Acceptance</h2>
      <p>
        By accessing or using usezombie (&quot;the Service&quot;), you agree to these Terms of Service.
        If you do not agree, do not use the Service.
      </p>

      <h2>2. Service Description</h2>
      <p>
        usezombie is an agent delivery control plane that processes specification queues into
        validated pull requests. The Service operates on your Git repositories using branch-based
        state and BYOK (Bring Your Own Keys) model access.
      </p>

      <h2>3. Your Responsibilities</h2>
      <ul>
        <li>You are responsible for your LLM API keys and any costs incurred with your providers.</li>
        <li>You must not use the Service to generate malicious code, violate third-party rights, or circumvent security controls.</li>
        <li>You are responsible for the content of specifications submitted to the pipeline.</li>
        <li>You must maintain the security of your authentication credentials.</li>
      </ul>

      <h2>4. Billing</h2>
      <ul>
        <li>usezombie charges for agent compute time (per second of wall-clock time).</li>
        <li>A one-time workspace activation fee of $5 applies to each new workspace.</li>
        <li>LLM token costs are paid directly to your provider — usezombie never marks up tokens.</li>
        <li>Billing cycles are monthly. Invoices are issued at the start of each billing period.</li>
      </ul>

      <h2>5. Intellectual Property</h2>
      <p>
        You retain all rights to your source code, specifications, and generated artifacts.
        usezombie claims no ownership over outputs produced by the pipeline.
      </p>

      <h2>6. Limitation of Liability</h2>
      <p>
        usezombie is provided &quot;as is&quot; without warranty. We are not liable for damages arising
        from agent-generated code, pipeline failures, or third-party service outages. Enterprise
        tier customers may negotiate contractual SLAs.
      </p>

      <h2>7. Termination</h2>
      <p>
        Either party may terminate at any time. Upon termination, workspace data is retained
        for 30 days, after which it is permanently deleted.
      </p>

      <h2>8. Contact</h2>
      <p>
        For questions about these terms, contact <a href="mailto:legal@usezombie.com">legal@usezombie.com</a>.
      </p>
    </section>
  );
}
