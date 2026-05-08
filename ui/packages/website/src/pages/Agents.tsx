import { Card, InstallBlock, Terminal } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "usezombie",
  applicationCategory: "DeveloperApplication",
  url: "https://usezombie.sh/agents",
  sameAs: ["https://usezombie.sh/openapi.json"],
};

const apiOps = [
  { action: "Create agent",   method: "POST",   path: "/v1/workspaces/:workspace_id/zombies",                                  purpose: "Provision a new agent in a workspace" },
  { action: "Update agent",   method: "PATCH",  path: "/v1/workspaces/:workspace_id/zombies/:zombie_id",                       purpose: "Update mutable configuration (body: { config_json })." },
  { action: "Stop agent",     method: "PATCH",  path: "/v1/workspaces/:workspace_id/zombies/:zombie_id",                       purpose: "Halt the running session, keep the record (body: { status: \"stopped\" })." },
  { action: "Resume agent",   method: "PATCH",  path: "/v1/workspaces/:workspace_id/zombies/:zombie_id",                       purpose: "Return a stopped agent to active execution (body: { status: \"active\" })." },
  { action: "Kill agent",     method: "PATCH",  path: "/v1/workspaces/:workspace_id/zombies/:zombie_id",                       purpose: "Mark the agent terminal — irreversible (body: { status: \"killed\" })." },
  { action: "Delete agent",   method: "DELETE", path: "/v1/workspaces/:workspace_id/zombies/:zombie_id",                       purpose: "Hard-purge the agent and its history. Must kill first." },
  { action: "Steer / chat",   method: "POST",   path: "/v1/workspaces/:workspace_id/zombies/:zombie_id/messages",              purpose: "Send a steer message to an agent" },
  { action: "Stream events",  method: "GET",    path: "/v1/workspaces/:workspace_id/zombies/:zombie_id/events/stream",         purpose: "Server-Sent Events stream of new events" },
  { action: "Ingest webhook", method: "POST",   path: "/v1/webhooks/:zombie_id",                                                purpose: "Deliver an inbound event to an agent" },
] as const;

const bootstrapScript = `# 1. Shell — install the CLI and the skill bundle
npm install -g @usezombie/zombiectl
zombiectl login
npx skills add usezombie/usezombie

# 2. Inside your coding agent (Claude Code / Amp / Codex CLI / OpenCode), run:
#    /usezombie-install-platform-ops
#    The slash-command provisions the platform-ops zombie and prints its zombie_id.

# 3. Back in the shell — steer the zombie
zombiectl steer <zombie_id> "morning health check"`;

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
  { title: "Audit trail", body: "Append-only agent event stream records every inbound trigger, steer, status change, and tool call with timestamps and actor identity." },
  { title: "Secret management", body: "Vault secrets encrypted via BYTEA columns. Git hooks disabled during agent runs. Subprocess timeouts enforced." },
  { title: "Policy enforcement", body: "Commands classified as safe, sensitive, or critical. Critical operations require explicit policy approval." },
];

export default function Agents() {
  return (
    <div data-testid="agents-page">
      <section className="site-section">
        <div className="wrap flex flex-col gap-6">
          <p className="font-mono text-[12px] uppercase tracking-[0.1em] text-text-muted m-0">
            agent surface
          </p>
          <h1 className="font-mono text-[clamp(40px,6vw,64px)] leading-[1.05] tracking-[-0.025em] font-medium text-text m-0">
            This page is for autonomous agents.
          </h1>
          <p className="font-sans text-[18px] leading-[1.5] text-text-muted m-0 max-w-[640px]">
            Use <code className="font-mono">/openapi.json</code> as canonical surface. Docs are
            secondary.
          </p>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
            Bootstrap
          </h2>
          <Terminal label="Bootstrap commands" copyable className="max-w-[860px]">
            {bootstrapScript}
          </Terminal>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <InstallBlock
            title="Install zombiectl"
            command="npm install -g @usezombie/zombiectl"
            actions={[
              { label: "→ start an agent", to: DOCS_QUICKSTART_URL, variant: "default" },
              { label: "read the docs", to: DOCS_URL, variant: "ghost" },
              { label: "open mission control", to: APP_BASE_URL, variant: "default" },
            ]}
          />
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
            Machine surface
          </h2>
          <Card className="font-mono text-[13px]">
            <a
              href="/openapi.json"
              className="text-pulse hover:underline"
              data-testid="agents-openapi-link"
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
          <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
            API operations
          </h2>
          <Card className="p-0 overflow-x-auto">
            <table className="w-full min-w-[640px] font-mono text-[13px] tabular-nums">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-[0.06em] text-[11px]">action</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-[0.06em] text-[11px]">method</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-[0.06em] text-[11px]">path</th>
                  <th className="text-left py-3 px-4 font-medium text-text-muted uppercase tracking-[0.06em] text-[11px]">purpose</th>
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
          <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
            Webhook ingest example
          </h2>
          <p className="font-sans text-[15px] leading-[1.6] text-text-muted m-0 max-w-[720px]">
            Configure an agent&apos;s trigger and POST inbound events to{" "}
            <code className="font-mono">/v1/webhooks/:zombie_id</code>. Every inbound webhook must
            carry a per-zombie HMAC signature header — unsigned requests are rejected.
          </p>
          <Terminal label="Webhook payload example" className="max-w-[860px]">
            {webhookPayload}
          </Terminal>
        </div>
      </section>

      <section className="site-section">
        <div className="wrap flex flex-col gap-4">
          <h2 className="font-mono text-[clamp(24px,3vw,32px)] leading-[1.2] tracking-[-0.015em] font-medium text-text m-0">
            Safety limits
          </h2>
          <div className="grid gap-4 grid-cols-1 md:grid-cols-2">
            {safetyLimits.map((limit) => (
              <Card key={limit.title} className="flex flex-col gap-2">
                <h3 className="font-mono text-[16px] leading-[1.3] tracking-[-0.01em] text-text font-medium m-0">
                  {limit.title}
                </h3>
                <p className="font-sans text-[14px] leading-[1.55] text-text-muted m-0">
                  {limit.body}
                </p>
              </Card>
            ))}
          </div>
        </div>
      </section>

      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
    </div>
  );
}
