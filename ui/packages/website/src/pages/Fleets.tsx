import { Button, Card, DisplayLG, DisplayXL, SectionLabel, Terminal } from "@agentsfleet/design-system";
import { DOCS_QUICKSTART_URL, DOCS_URL, INSTALL_COMMAND } from "../config";

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "agentsfleet",
  applicationCategory: "DeveloperApplication",
  url: "https://agentsfleet.dev/fleets",
  sameAs: ["https://agentsfleet.dev/openapi.json"],
};

const apiOps = [
  { action: "Create Fleet",   method: "POST",   path: "/v1/workspaces/:workspace_id/fleets",                                  purpose: "Provision a new Fleet in a workspace" },
  { action: "Update Fleet",   method: "PATCH",  path: "/v1/workspaces/:workspace_id/fleets/:fleet_id",                       purpose: "Update mutable configuration (body: { config_json })." },
  { action: "Stop Fleet",     method: "PATCH",  path: "/v1/workspaces/:workspace_id/fleets/:fleet_id",                       purpose: "Halt the running session, keep the record (body: { status: \"stopped\" })." },
  { action: "Resume Fleet",   method: "PATCH",  path: "/v1/workspaces/:workspace_id/fleets/:fleet_id",                       purpose: "Return a stopped Fleet to active execution (body: { status: \"active\" })." },
  { action: "Kill Fleet",     method: "PATCH",  path: "/v1/workspaces/:workspace_id/fleets/:fleet_id",                       purpose: "Mark the Fleet terminal — irreversible (body: { status: \"killed\" })." },
  { action: "Delete Fleet",   method: "DELETE", path: "/v1/workspaces/:workspace_id/fleets/:fleet_id",                       purpose: "Hard-purge the Fleet and its history. Must kill first." },
  { action: "Steer / chat",   method: "POST",   path: "/v1/workspaces/:workspace_id/fleets/:fleet_id/messages",              purpose: "Send a steer message to a Fleet" },
  { action: "Stream events",  method: "GET",    path: "/v1/workspaces/:workspace_id/fleets/:fleet_id/events/stream",         purpose: "Server-Sent Events stream of new events" },
  { action: "Ingest webhook", method: "POST",   path: "/v1/webhooks/:fleet_id",                                                purpose: "Deliver an inbound event to a Fleet" },
] as const;

const bootstrapScript = `# 1. Shell — one command installs the Command-Line Interface (CLI) + the skill bundle
${INSTALL_COMMAND}
#    (or with npm: npm install -g @agentsfleet/cli && npx skills add agentsfleet/skills)
agentsfleet login

# 2. Inside your coding agent (Claude Code / Amp / Codex CLI / OpenCode), run:
#    /agentsfleet-install-platform-ops
#    The slash-command provisions the platform-ops Fleet and prints its fleet_id.

# 3. Back in the shell — steer the Fleet
agentsfleet steer <fleet_id> "morning health check"`;

const webhookPayload = `{
  "event_id": "evt_01JEXAMPLE",
  "type": "deploy.failed",
  "data": {
    "service": "checkout-api",
    "environment": "production",
    "reason": "health_check_timeout"
  }
}`;

const safetyLimits = [
  { title: "Idempotency", body: "Inbound webhook events deduplicate on event_id within a 24-hour window. Workspace updates use monotonic versions to prevent lost updates." },
  { title: "Audit trail", body: "Append-only Fleet event stream records every inbound trigger, steer, status change, and tool call with timestamps and actor identity." },
  { title: "Secret management", body: "Vault secrets encrypted via BYTEA columns. Git hooks disabled during Fleet runs. Subprocess timeouts enforced." },
  { title: "Policy enforcement", body: "Commands classified as safe, sensitive, or critical. Critical operations require explicit policy approval." },
];

// Coming-soon surface so a Fleet parser knows what is NOT yet generally available
// and should not be relied on. Grounded in the prebuilt fleet (Security Reviewer
// is comingSoon) and the v2 hosted-only posture (self-host deferred).
const comingSoon = [
  { title: "Security Reviewer (prebuilt)", body: "Scans each pull request and its dependencies for vulnerabilities and exposed secrets, opens a remediation pull request, and holds at human approval. Not generally available yet — join the waitlist." },
  { title: "Self-host", body: "Hosted-only today on api.agentsfleet.net (Bearer + Clerk OAuth). Self-managed deployment lands in a later release." },
];

const mono = "font-mono text-text";

// The minimal authenticated call sequence a Fleet follows to go from zero to a
// running, observable Fleet. Grounded in the OpenAPI surface: BearerAuth +
// POST /v1/api-keys mint, Fleet CRUD, HMAC-signed webhook ingest, SSE stream.
const getStartedSteps = [
  {
    number: "01",
    label: "authenticate",
    body: (
      <>
        Mint a tenant key with <code className={mono}>POST /v1/api-keys</code> (returns an{" "}
        <code className={mono}>agt_t…</code> key once), then send{" "}
        <code className={mono}>Authorization: Bearer agt_t…</code> on every request.
      </>
    ),
  },
  {
    number: "02",
    label: "create a Fleet",
    body: (
      <>
        Provision one with{" "}
        <code className={mono}>POST /v1/workspaces/:workspace_id/fleets</code>.
      </>
    ),
  },
  {
    number: "03",
    label: "trigger it",
    body: (
      <>
        Send an event to <code className={mono}>POST /v1/webhooks/:fleet_id</code> (HMAC-signed),
        or steer it with <code className={mono}>POST …/fleets/:fleet_id/messages</code>.
      </>
    ),
  },
  {
    number: "04",
    label: "watch it work",
    body: (
      <>
        Stream the run over Server-Sent Events at{" "}
        <code className={mono}>GET …/fleets/:fleet_id/events/stream</code>.
      </>
    ),
  },
];

/*
 * ConstraintTable — the machine-readable two-column table shape shared by the
 * Safety limits and Coming soon sections (constraint -> rule / capability ->
 * status). `<th scope="row">` keeps each row label a row header for assistive
 * tech + Fleet parsers. An optional `badge` annotates every row title (the
 * Coming soon section uses it).
 */
function ConstraintTable({
  cols,
  rows,
  badge,
}: {
  cols: readonly [string, string];
  rows: readonly { title: string; body: string }[];
  badge?: string;
}) {
  return (
    <Card className="p-0 overflow-x-auto">
      <table className="w-full min-w-narrow font-mono text-mono">
        <thead>
          <tr className="border-b border-border">
            {cols.map((col) => (
              <th
                key={col}
                className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-label text-label"
              >
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.title}
              className="border-b border-border last:border-b-0 align-top"
            >
              <th
                scope="row"
                className="text-left py-3 px-4 font-medium text-text whitespace-nowrap"
              >
                {row.title}
                {badge ? <span className="text-evidence"> {badge}</span> : null}
              </th>
              <td className="py-3 px-4 text-text-muted">{row.body}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

export default function Fleets() {
  return (
    <div data-testid="fleets-page">
      <section className="site-section">
        <div className="wrap flex flex-col gap-6">
          <SectionLabel className="mb-0">Fleet surface</SectionLabel>
          <DisplayXL>This page is for autonomous Fleets.</DisplayXL>
          <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">
            Use <code className="font-mono">/openapi.json</code> as canonical surface. Docs are
            secondary.
          </p>
        </div>
      </section>

      {/* Get started for a Fleet — the minimal authenticated call sequence.
        * This page's audience is a machine, so the first thing it needs (how to
        * authenticate) leads, then create / trigger / stream. Full schemas live
        * in /openapi.json. */}
      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md text-pulse">
            Get started in four calls
          </DisplayLG>
          <p className="font-sans text-body leading-body text-text-muted m-0 max-w-measure">
            Authenticate, create a Fleet, trigger it, then stream what it does.
            Full request and response schemas live in{" "}
            <a href="/openapi.json" className="text-pulse hover:underline">
              /openapi.json
            </a>
            .
          </p>
          <Card>
            <ol className="m-0 flex list-none flex-col gap-4 p-0">
              {getStartedSteps.map((step) => (
                <li key={step.number} className="flex flex-col gap-1">
                  <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-pulse">
                    {step.number} · {step.label}
                  </span>
                  <p className="font-sans text-body-sm leading-body text-text-muted m-0">
                    {step.body}
                  </p>
                </li>
              ))}
            </ol>
          </Card>
        </div>
      </section>

      {/* One install story for a Fleet: the CLI plus the skill bundle plus the
        * in-agent slash command. The old standalone InstallBlock (bare `npm
        * install -g` + an "open dashboard" link) was redundant with this and
        * has been folded in. Heading carries the mint brand color. */}
      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md text-pulse">
            Install agentsfleet
          </DisplayLG>
          <p className="font-sans text-body leading-body text-text-muted m-0 max-w-measure">
            Install the command-line interface and the skill bundle, then
            provision a Fleet from inside your coding agent. No dashboard
            required.
          </p>
          <Terminal label="Bootstrap commands" copyable className="max-w-wide">
            {bootstrapScript}
          </Terminal>
          <div className="flex flex-wrap items-center gap-3">
            <Button asChild className="min-h-11">
              <a href={DOCS_QUICKSTART_URL} target="_blank" rel="noopener noreferrer">
                → start a Fleet
              </a>
            </Button>
            <Button asChild variant="ghost" className="min-h-11">
              <a href={DOCS_URL} target="_blank" rel="noopener noreferrer">
                read the docs
              </a>
            </Button>
          </div>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md">
            Machine surface
          </DisplayLG>
          <Card className="font-mono text-mono">
            <a
              href="/openapi.json"
              className="text-pulse hover:underline"
              data-testid="fleets-openapi-link"
            >
              /openapi.json
            </a>
            <span className="text-text-muted ml-3">
              Canonical API surface (OpenAPI 3.1)
            </span>
          </Card>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md">
            API operations
          </DisplayLG>
          <Card className="p-0 overflow-x-auto">
            <table className="w-full min-w-narrow font-mono text-mono tabular-nums">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-label text-label">action</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-label text-label">method</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-label text-label">path</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-label text-label">purpose</th>
                </tr>
              </thead>
              <tbody>
                {apiOps.map((op) => (
                  <tr
                    key={`${op.action}-${op.method}-${op.path}`}
                    className="border-b border-border last:border-b-0"
                  >
                    <td className="py-3 px-4 text-text">{op.action}</td>
                    <td className="py-3 px-4 text-info">{op.method}</td>
                    <td className="py-3 px-4 text-text-muted">{op.path}</td>
                    <td className="py-3 px-4 text-text-muted">{op.purpose}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md">
            Webhook ingest example
          </DisplayLG>
          <p className="font-sans text-body leading-body text-text-muted m-0 max-w-measure">
            Configure a Fleet&apos;s trigger and POST inbound events to{" "}
            <code className="font-mono">/v1/webhooks/:fleet_id</code>. Each webhook is HMAC-signed
            per the trigger source&apos;s scheme — e.g.{" "}
            <code className="font-mono">x-hub-signature-256</code> for GitHub,{" "}
            <code className="font-mono">x-slack-signature</code> +{" "}
            <code className="font-mono">x-slack-request-timestamp</code> for Slack. Unsigned or
            stale requests are rejected; the exact header set per source lives in{" "}
            <a href="/openapi.json" className="text-pulse hover:underline">
              /openapi.json
            </a>
            . Duplicate deliveries within the source&apos;s dedup window (24h or more) are accepted
            idempotently.
          </p>
          <Terminal label="Webhook payload example" className="max-w-wide">
            {webhookPayload}
          </Terminal>
        </div>
      </section>

      {/* Safety limits as a machine-readable constraint table (constraint ->
        * rule), matching the API-operations table above — this page's audience
        * is an autonomous Fleet parsing the surface, not a human reading
        * marketing cards. */}
      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md">
            Safety limits
          </DisplayLG>
          <ConstraintTable cols={["constraint", "rule"]} rows={safetyLimits} />
        </div>
      </section>

      {/* Coming soon — tells a Fleet parser which capabilities are NOT yet generally available so it
        * doesn't build against them. Same constraint-table shape. */}
      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <DisplayLG className="text-fluid-display-md">
            Coming soon
          </DisplayLG>
          <ConstraintTable
            cols={["capability", "status"]}
            rows={comingSoon}
            badge="coming soon"
          />
        </div>
      </section>

      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
    </div>
  );
}
