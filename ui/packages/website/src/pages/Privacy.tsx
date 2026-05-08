/*
 * Privacy — single-column long-form prose. Per DESIGN_SYSTEM.md §Layout
 * docs: ~68ch measure, Commit Mono headings, Instrument Sans body.
 */
export default function Privacy() {
  return (
    <article
      data-testid="privacy-page"
      className="wrap site-section flex flex-col gap-6 max-w-[68ch] font-sans text-[15px] leading-[1.7] text-text"
    >
      <p className="font-mono text-[12px] uppercase tracking-[0.1em] text-text-muted m-0">
        legal
      </p>
      <h1 className="font-mono text-[clamp(36px,5vw,52px)] leading-[1.05] tracking-[-0.025em] font-medium text-text m-0">
        Privacy Policy
      </h1>
      <p className="font-mono text-[12px] text-text-muted m-0">Last updated: May 5, 2026</p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">1. Information we collect</h2>
      <p className="text-text-muted m-0">usezombie collects only the minimum information required to operate the service:</p>
      <ul className="list-disc pl-6 text-text-muted space-y-2 m-0">
        <li><strong className="text-text font-medium">Account information</strong> — email address and authentication credentials managed by our identity provider (Clerk).</li>
        <li><strong className="text-text font-medium">Workspace metadata</strong> — repository URLs, workspace configuration, run history, and transition logs.</li>
        <li><strong className="text-text font-medium">Usage telemetry</strong> — agent compute time, API call counts, and error rates for billing and reliability.</li>
      </ul>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">2. Information we do not collect</h2>
      <ul className="list-disc pl-6 text-text-muted space-y-2 m-0">
        <li><strong className="text-text font-medium">LLM API keys</strong> — your keys are stored encrypted and never transmitted to usezombie servers in plaintext. We operate on a BYOK model.</li>
        <li><strong className="text-text font-medium">Source code contents</strong> — usezombie agents operate within your Git repositories via branch-based state. Code is never copied to usezombie infrastructure.</li>
        <li><strong className="text-text font-medium">Model outputs</strong> — generated patches, plans, and validation results remain in your repository as artifacts.</li>
      </ul>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">3. How we use your information</h2>
      <ul className="list-disc pl-6 text-text-muted space-y-2 m-0">
        <li>Authenticate and authorize access to your workspaces.</li>
        <li>Calculate billing based on agent compute time (wall-clock seconds).</li>
        <li>Monitor service health and investigate operational issues.</li>
        <li>Send transactional notifications (run completions, failures, billing).</li>
      </ul>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">4. Data retention</h2>
      <p className="text-text-muted m-0">
        Run transition logs and audit trails are retained for the lifetime of your account. Upon
        account deletion, all workspace metadata and logs are permanently removed within 30 days.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">5. Third-party services</h2>
      <ul className="list-disc pl-6 text-text-muted space-y-2 m-0">
        <li><strong className="text-text font-medium">Clerk</strong> — identity and authentication.</li>
        <li>
          <strong className="text-text font-medium">PostHog</strong> — product analytics. Captures pageviews,
          anonymized interaction events (clicks, form submissions with values masked), and
          performance metrics. Stores a first-party identifier in{" "}
          <code className="font-mono text-text">localStorage</code> (no tracking cookie) so we can
          recognize a returning visitor on the marketing site. Known bots are auto-tagged and
          excluded from human-traffic insights. PostHog data is hosted on US infrastructure
          (<code className="font-mono text-text">us.i.posthog.com</code>).
        </li>
        <li><strong className="text-text font-medium">Vercel</strong> — website hosting.</li>
      </ul>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">6. Cookies and local storage</h2>
      <p className="text-text-muted m-0">We use a small number of first-party identifiers to operate the site:</p>
      <ul className="list-disc pl-6 text-text-muted space-y-2 m-0">
        <li><strong className="text-text font-medium">Authentication</strong> — session cookies set by Clerk to keep you signed in.</li>
        <li><strong className="text-text font-medium">Analytics</strong> — a PostHog-managed identifier in <code className="font-mono text-text">localStorage</code> on the marketing site. It is not a cookie and does not contain personal information.</li>
      </ul>
      <p className="text-text-muted m-0">
        We do not set a tracking cookie for analytics, run third-party advertising trackers, or use
        cross-site cookies / re-targeting pixels. To opt out of analytics, block{" "}
        <code className="font-mono text-text">us.i.posthog.com</code> in your browser or content
        blocker — the site continues to function normally without it.
      </p>

      <h2 className="font-mono text-[20px] mt-6 mb-0 font-medium tracking-[-0.01em]">7. Contact</h2>
      <p className="text-text-muted m-0">
        For privacy inquiries, contact{" "}
        <a href="mailto:privacy@usezombie.com" className="text-pulse hover:underline">
          privacy@usezombie.com
        </a>
        .
      </p>
    </article>
  );
}
