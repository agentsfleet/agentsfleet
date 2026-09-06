import { Button, Card, DisplayLG, DisplayXL, SectionLabel, Terminal } from "@agentsfleet/design-system";
import { DOCS_QUICKSTART_URL, DOCS_URL, INSTALL_COMMAND } from "../config";

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "agentsfleet",
  applicationCategory: "DeveloperApplication",
  url: "https://agentsfleet.dev/agents",
  sameAs: ["https://agentsfleet.dev/openapi.json"],
};

const bootstrapScript = `# 1. Shell — one command installs the Command-Line Interface (CLI) + the skill bundle
${INSTALL_COMMAND}
#    (or with npm: npm install -g @agentsfleet/cli && npx skills add agentsfleet/skills)
agentsfleet login

# 2. Inside your coding agent (Claude Code / Amp / Codex CLI / OpenCode), ask:
#    "Create a fleet for incident response in my workspace."
#    Use the fleet_id returned when the fleet is created.

# 3. Back in the shell — steer the Fleet
agentsfleet steer <fleet_id> "morning health check"`;

const safetyLimits = [
  { title: "Idempotency", body: "Inbound webhook events deduplicate on event_id within a 24-hour window. Workspace updates use monotonic versions to prevent lost updates." },
  { title: "Audit trail", body: "Append-only Fleet event stream records every inbound trigger, steer, status change, and tool call with timestamps and actor identity." },
  { title: "Secret management", body: "Vault secrets encrypted via BYTEA columns. Git hooks disabled during Fleet runs. Subprocess timeouts enforced." },
  { title: "Policy enforcement", body: "Commands classified as safe, sensitive, or critical. Critical operations require explicit policy approval." },
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

// Row headers keep safety constraints readable to assistive technology.
function ConstraintTable({
  cols,
  rows,
}: {
  cols: readonly [string, string];
  rows: readonly { title: string; body: string }[];
}) {
  return (
    <Card tabIndex={0} className="p-0 overflow-x-auto focus-visible:outline-2 focus-visible:outline-ring">
      <table className="w-full min-w-narrow font-sans text-body-sm">
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
          <SectionLabel className="mb-0">Agent resources</SectionLabel>
          <DisplayXL>This page is for agents.</DisplayXL>
          <p className="font-sans text-body-lg leading-body-lg text-text-muted m-0 max-w-narrow">
            Use <code className="font-mono">/openapi.json</code> as canonical surface. Docs are
            secondary.
          </p>
        </div>
      </section>

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
            Safety limits
          </DisplayLG>
          <ConstraintTable cols={["constraint", "rule"]} rows={safetyLimits} />
        </div>
      </section>

      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
    </div>
  );
}
