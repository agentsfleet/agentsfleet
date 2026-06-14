import { DisplayXL, List, ListItem, SectionLabel } from "@agentsfleet/design-system";
import { SUPPORT_EMAIL } from "../lib/contact";

/*
 * Privacy — single-column long-form prose. Per DESIGN_SYSTEM.md §Layout
 * docs: ~68ch measure, Commit Mono headings, Instrument Sans body.
 */
export default function Privacy() {
  return (
    <article
      data-testid="privacy-page"
      className="wrap site-section flex flex-col gap-6 max-w-prose font-sans text-body leading-prose text-text"
    >
      <SectionLabel className="mb-0">legal</SectionLabel>
      <DisplayXL className="text-fluid-display-lg">Privacy Policy</DisplayXL>
      <p className="font-mono text-eyebrow text-text-muted m-0">Last updated: May 5, 2026</p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">1. Information we collect</h2>
      <p className="text-text-muted m-0">usezombie collects only the minimum information required to operate the service:</p>
      <List className="pl-6 text-text-muted m-0">
        <ListItem><strong className="text-text font-medium">Account information</strong> — email address and authentication credentials managed by our identity provider (Clerk).</ListItem>
        <ListItem><strong className="text-text font-medium">Workspace metadata</strong> — repository URLs, workspace configuration, run history, and transition logs.</ListItem>
        <ListItem><strong className="text-text font-medium">Usage telemetry</strong> — event receipts, stage executions, API call counts, and error rates for credit-pool metering and reliability.</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">2. Information we do not collect</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem><strong className="text-text font-medium">LLM API keys</strong> — your keys are stored encrypted and never transmitted to usezombie servers in plaintext. We operate on a self-managed model.</ListItem>
        <ListItem><strong className="text-text font-medium">Source code contents</strong> — usezombie agents operate within your Git repositories via branch-based state. Code is never copied to usezombie infrastructure.</ListItem>
        <ListItem><strong className="text-text font-medium">Model outputs</strong> — generated patches, plans, and validation results remain in your repository as artifacts.</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">3. How we use your information</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem>Authenticate and authorize access to your workspaces.</ListItem>
        <ListItem>Meter hosted execution against your credit pool: a debit fires before each stage execution.</ListItem>
        <ListItem>Monitor service health and investigate operational issues.</ListItem>
        <ListItem>Send transactional notifications (run completions, failures, billing).</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">4. Data retention</h2>
      <p className="text-text-muted m-0">
        Run transition logs and audit trails are retained for the lifetime of your account. Upon
        account deletion, all workspace metadata and logs are permanently removed within 30 days.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">5. Third-party services</h2>
      <List className="pl-6 text-text-muted m-0">
        <ListItem><strong className="text-text font-medium">Clerk</strong> — identity and authentication.</ListItem>
        <ListItem>
          <strong className="text-text font-medium">PostHog</strong> — product analytics. Captures pageviews,
          anonymized interaction events (clicks, form submissions with values masked), and
          performance metrics. Stores a first-party identifier in{" "}
          <code className="font-mono text-text">localStorage</code> (no tracking cookie) so we can
          recognize a returning visitor on the marketing site. Known bots are auto-tagged and
          excluded from human-traffic insights. PostHog data is hosted on US infrastructure
          (<code className="font-mono text-text">us.i.posthog.com</code>).
        </ListItem>
        <ListItem><strong className="text-text font-medium">Vercel</strong> — website hosting.</ListItem>
      </List>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">6. Cookies and local storage</h2>
      <p className="text-text-muted m-0">We use a small number of first-party identifiers to operate the site:</p>
      <List className="pl-6 text-text-muted m-0">
        <ListItem><strong className="text-text font-medium">Authentication</strong> — session cookies set by Clerk to keep you signed in.</ListItem>
        <ListItem><strong className="text-text font-medium">Analytics</strong> — a PostHog-managed identifier in <code className="font-mono text-text">localStorage</code> on the marketing site. It is not a cookie and does not contain personal information.</ListItem>
      </List>
      <p className="text-text-muted m-0">
        We do not set a tracking cookie for analytics, run third-party advertising trackers, or use
        cross-site cookies / re-targeting pixels. To opt out of analytics, block{" "}
        <code className="font-mono text-text">us.i.posthog.com</code> in your browser or content
        blocker — the site continues to function normally without it.
      </p>

      <h2 className="font-mono text-heading mt-6 mb-0 font-medium">7. Contact</h2>
      <p className="text-text-muted m-0">
        For privacy inquiries, contact{" "}
        <a href={`mailto:${SUPPORT_EMAIL}`} className="text-pulse hover:underline">
          {SUPPORT_EMAIL}
        </a>
        .
      </p>
    </article>
  );
}
