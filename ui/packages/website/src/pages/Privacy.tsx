export default function Privacy() {
  return (
    <section className="stack legal-page route-fade">
      <p className="eyebrow">legal</p>
      <h1>Privacy Policy</h1>
      <p className="lead">Last updated: May 5, 2026</p>

      <h2>1. Information We Collect</h2>
      <p>
        usezombie collects only the minimum information required to operate the service:
      </p>
      <ul>
        <li><strong>Account information</strong> — email address and authentication credentials managed by our identity provider (Clerk).</li>
        <li><strong>Workspace metadata</strong> — repository URLs, workspace configuration, run history, and transition logs.</li>
        <li><strong>Usage telemetry</strong> — agent compute time, API call counts, and error rates for billing and reliability.</li>
      </ul>

      <h2>2. Information We Do Not Collect</h2>
      <ul>
        <li><strong>LLM API keys</strong> — your keys are stored encrypted and never transmitted to usezombie servers in plaintext. We operate on a BYOK model.</li>
        <li><strong>Source code contents</strong> — usezombie agents operate within your Git repositories via branch-based state. Code is never copied to usezombie infrastructure.</li>
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
        <li>
          <strong>PostHog</strong> — product analytics. Captures pageviews, anonymized
          interaction events (clicks, form submissions with values masked), and
          performance metrics. Stores a first-party identifier in <code>localStorage</code>
          and a cookie scoped to <code>usezombie.com</code> so we can stitch the
          marketing site and signed-in app into one journey for funnel analysis.
          Known bots are auto-tagged and excluded from human-traffic insights.
          PostHog data is hosted on US infrastructure (<code>us.i.posthog.com</code>).
        </li>
        <li><strong>Vercel</strong> — website hosting.</li>
      </ul>

      <h2>6. Cookies and Local Storage</h2>
      <p>
        We use a small number of first-party identifiers to operate the site:
      </p>
      <ul>
        <li>
          <strong>Authentication</strong> — session cookies set by Clerk to keep
          you signed in.
        </li>
        <li>
          <strong>Analytics</strong> — a PostHog-managed identifier in
          <code> localStorage</code> and a same-site cookie that lets us recognize
          a returning visitor across pages and across the marketing site / app
          subdomain. It does not contain personal information.
        </li>
      </ul>
      <p>
        We do not run third-party advertising trackers, cross-site cookies, or
        re-targeting pixels. To opt out of analytics, block
        <code> us.i.posthog.com</code> in your browser or content blocker — the
        site continues to function normally without it.
      </p>

      <h2>7. Contact</h2>
      <p>
        For privacy inquiries, contact <a href="mailto:privacy@usezombie.com">privacy@usezombie.com</a>.
      </p>
    </section>
  );
}
