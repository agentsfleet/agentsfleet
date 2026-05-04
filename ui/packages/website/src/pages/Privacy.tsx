export default function Privacy() {
  return (
    <section className="stack legal-page route-fade">
      <p className="eyebrow">legal</p>
      <h1>Privacy Policy</h1>
      <p className="lead">Last updated: May 4, 2026</p>

      <h2>1. Information We Collect</h2>
      <p>
        UseZombie collects only the minimum information required to operate the service:
      </p>
      <ul>
        <li><strong>Account information</strong> — email address and authentication credentials managed by our identity provider (Clerk).</li>
        <li><strong>Workspace metadata</strong> — repository URLs, workspace configuration, run history, and transition logs.</li>
        <li><strong>Usage telemetry</strong> — agent compute time, API call counts, and error rates for billing and reliability.</li>
      </ul>

      <h2>2. Information We Do Not Collect</h2>
      <ul>
        <li><strong>LLM API keys</strong> — your keys are stored encrypted and never transmitted to UseZombie servers in plaintext. We operate on a BYOK model.</li>
        <li><strong>Source code contents</strong> — UseZombie agents operate within your Git repositories via branch-based state. Code is never copied to UseZombie infrastructure.</li>
        <li><strong>Model outputs</strong> — generated patches, plans, and validation results remain in your repository as artifacts.</li>
      </ul>

      <h2>3. How We Use Your Information</h2>
      <ul>
        <li>Authenticate and authorize access to your workspaces.</li>
        <li>Calculate billing based on agent compute time (wall-clock seconds).</li>
        <li>Monitor service health and investigate operational issues.</li>
        <li>Send transactional notifications (run completions, failures, billing).</li>
      </ul>

      <h2>4. Data Retention</h2>
      <p>
        Run transition logs and audit trails are retained for the lifetime of your account.
        Upon account deletion, all workspace metadata and logs are permanently removed within 30 days.
      </p>

      <h2>5. Third-Party Services</h2>
      <ul>
        <li><strong>Clerk</strong> — identity and authentication.</li>
        <li><strong>PostHog</strong> — product analytics (anonymized).</li>
        <li><strong>Vercel</strong> — website hosting.</li>
      </ul>

      <h2>6. Contact</h2>
      <p>
        For privacy inquiries, contact <a href="mailto:privacy@usezombie.com">privacy@usezombie.com</a>.
      </p>
    </section>
  );
}
