import { lazy, Suspense } from "react";
import { InstallBlock } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

/* M26.8 demos are loaded from a lazy chunk so the agents-route initial
 * payload stays lean and the motion library never lands on /pricing,
 * /privacy, or /terms. */
const BackgroundBeamsWithCollision = lazy(() =>
  import("../components/domain/background-beams-with-collision").then((m) => ({
    default: m.BackgroundBeamsWithCollision,
  })),
);
const AnimatedTerminal = lazy(() =>
  import("../components/domain/animated-terminal").then((m) => ({
    default: m.AnimatedTerminal,
  })),
);

const DEMO_COMMANDS = [
  "zombiectl login",
  "zombiectl zombie install --template lead-collector",
  "zombiectl zombie up lead-collector --watch",
];

const DEMO_OUTPUTS: Record<number, string[]> = {
  1: ["Installed lead-collector@0.1.0"],
  2: ["[ready] lead-collector awaiting triggers"],
};

const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "UseZombie",
  applicationCategory: "DeveloperApplication",
  url: "https://usezombie.sh/agents",
  sameAs: [
    "https://usezombie.sh/openapi.json",
    "https://usezombie.sh/agent-manifest.json",
    "https://usezombie.sh/skill.md",
  ],
};

const contracts = [
  { endpoint: "/openapi.json", purpose: "Canonical API contract (OpenAPI 3.1)" },
  { endpoint: "/agent-manifest.json", purpose: "Machine-readable capability and endpoint summary" },
  { endpoint: "/skill.md", purpose: "Bootstrap instructions for autonomous agents" },
  { endpoint: "/llms.txt", purpose: "LLM-friendly index to docs and endpoints" },
  { endpoint: "/heartbeat", purpose: "Health check (static JSON)" },
];

const apiOps = [
  { action: "Create zombie", method: "POST", path: "/v1/workspaces/:workspace_id/zombies", purpose: "Provision a new Zombie in a workspace" },
  { action: "Update zombie", method: "PATCH", path: "/v1/workspaces/:workspace_id/zombies/:zombie_id", purpose: "Update a Zombie's mutable configuration (body: { config_json })." },
  { action: "Kill zombie", method: "PATCH", path: "/v1/workspaces/:workspace_id/zombies/:zombie_id", purpose: "Cancel any in-flight session and mark the Zombie killed (body: { status: \"killed\" })." },
  { action: "Steer zombie", method: "POST", path: "/v1/workspaces/:workspace_id/zombies/:zombie_id/steer", purpose: "Inject a steering instruction into the Zombie's loop" },
  { action: "Stream events", method: "GET", path: "/v1/workspaces/:workspace_id/zombies/:zombie_id/events/stream", purpose: "Server-Sent Events stream of new events" },
  { action: "Ingest webhook", method: "POST", path: "/v1/webhooks/:zombie_id", purpose: "Deliver an inbound event to a Zombie" },
  { action: "Pause workspace", method: "PATCH", path: "/v1/workspaces/:workspace_id", purpose: "Pause / unpause admission of new work (body: { pause, reason, version })" },
  { action: "Execute tool", method: "POST", path: "/v1/execute", purpose: "Synchronous tool execution proxy for external agents" },
];

const webhookPayload = `{
  "event_id": "evt_01JEXAMPLE",
  "type": "email.received",
  "data": {
    "from": "lead@example.com",
    "subject": "Demo request",
    "body": "..."
  }
}`;

export default function Agents() {
  return (
    <section className="stack agent-surface route-fade">
      <div className="scanline" aria-hidden="true" />

      <p className="eyebrow">agent surface</p>
      <h1>This page is for autonomous agents.</h1>
      <p className="lead" style={{ color: "var(--z-text-muted)" }}>
        Use <code>/openapi.json</code> as canonical contract. Docs are secondary.
      </p>

      <Suspense fallback={<div className="min-h-[20rem]" aria-hidden="true" />}>
        <BackgroundBeamsWithCollision className="rounded-lg border border-border">
          <div className="flex flex-col items-start gap-md px-xl py-2xl">
            <p className="eyebrow">interactive</p>
            <h2 className="text-2xl">Zombie agents orchestrate work, humans approve.</h2>
            <p className="text-muted-foreground max-w-[52ch]">
              Install, run, and observe the agent lifecycle without leaving this page.
            </p>
          </div>
        </BackgroundBeamsWithCollision>
      </Suspense>

      <Suspense fallback={<div className="min-h-[16rem]" aria-hidden="true" />}>
        <AnimatedTerminal commands={DEMO_COMMANDS} outputs={DEMO_OUTPUTS} />
      </Suspense>

      {/* Install Zombiectl */}
      <InstallBlock
        title="Install Zombiectl"
        command="curl -sSL https://usezombie.sh/install | bash"
        actions={[
          { label: "Install Zombiectl", to: DOCS_QUICKSTART_URL, variant: "default" },
          { label: "Read the docs", to: DOCS_URL, variant: "ghost" },
          { label: "Setup your personal dashboard", to: APP_BASE_URL, variant: "double-border" },
        ]}
      />

      {/* Bootstrap */}
      <div>
        <h2 style={{ marginBottom: "0.75rem" }}>Bootstrap</h2>
        <pre className="terminal" aria-label="Bootstrap commands">
          <code>{`# Read the skill guide
curl -s https://usezombie.sh/skill.md

# Authenticate and add a workspace
npx zombiectl login && zombiectl workspace add https://github.com/your-org/your-repo`}</code>
        </pre>
      </div>

      {/* Machine contracts */}
      <div>
        <h2 style={{ marginBottom: "0.75rem" }}>Machine Contracts</h2>
        <table className="agent-table">
          <thead>
            <tr>
              <th>Endpoint</th>
              <th>Purpose</th>
            </tr>
          </thead>
          <tbody>
            {contracts.map((c) => (
              <tr key={c.endpoint}>
                <td><a href={c.endpoint}>{c.endpoint}</a></td>
                <td>{c.purpose}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* API Operations */}
      <div>
        <h2 style={{ marginBottom: "0.75rem" }}>API Operations</h2>
        <table className="agent-table">
          <thead>
            <tr>
              <th>Action</th>
              <th>Method</th>
              <th>Path</th>
              <th>Purpose</th>
            </tr>
          </thead>
          <tbody>
            {apiOps.map((op) => (
              <tr key={op.action}>
                <td>{op.action}</td>
                <td><span className="method">{op.method}</span></td>
                <td><code>{op.path}</code></td>
                <td>{op.purpose}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Webhook example */}
      <div>
        <h2 style={{ marginBottom: "0.75rem" }}>Webhook Ingest Example</h2>
        <p style={{ color: "var(--z-text-muted)", marginBottom: "0.75rem" }}>
          Configure a Zombie&apos;s trigger and POST inbound events to <code>/v1/webhooks/:zombie_id</code>.
          The event is appended to the Zombie&apos;s stream and dispatched to its loop:
        </p>
        <pre className="terminal" aria-label="Webhook payload example">
          <code>{webhookPayload}</code>
        </pre>
      </div>

      {/* Safety limits */}
      <div>
        <h2 style={{ marginBottom: "0.75rem" }}>Safety Limits</h2>
        <div className="grid two">
          <article className="card">
            <h3>Idempotency</h3>
            <p>Inbound webhook events deduplicate on <code>event_id</code> within a 24-hour window. Workspace updates use monotonic versions to prevent lost updates.</p>
          </article>
          <article className="card">
            <h3>Audit Trail</h3>
            <p>Append-only zombie event stream records every inbound trigger, steer, status change, and tool call with timestamps and actor identity.</p>
          </article>
          <article className="card">
            <h3>Secret Management</h3>
            <p>Vault secrets encrypted via BYTEA columns. Git hooks disabled during agent runs. Subprocess timeouts enforced.</p>
          </article>
          <article className="card">
            <h3>Policy Enforcement</h3>
            <p>Commands classified as safe, sensitive, or critical. Critical operations require explicit policy approval.</p>
          </article>
        </div>
      </div>

      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
    </section>
  );
}
