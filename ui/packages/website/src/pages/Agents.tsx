import { InstallBlock } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_QUICKSTART_URL, DOCS_URL } from "../config";

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
  { action: "Start run", method: "POST", path: "/v1/runs", purpose: "Queue a spec for delivery" },
  { action: "Get run", method: "GET", path: "/v1/runs/:id", purpose: "Check run status and artifacts" },
  { action: "Retry run", method: "POST", path: "/v1/runs/:id/retry", purpose: "Retry a failed run" },
  { action: "Pause workspace", method: "POST", path: "/v1/workspaces/:id/pause", purpose: "Pause all runs in workspace" },
  { action: "List specs", method: "GET", path: "/v1/specs", purpose: "List queued specs" },
  { action: "Sync specs", method: "POST", path: "/v1/specs/sync", purpose: "Sync specs from repo" },
];

const webhookPayload = `{
  "event": "run.completed",
  "run_id": "run_01JEXAMPLE",
  "workspace_id": "ws_01JEXAMPLE",
  "status": "DONE",
  "pr_url": "https://github.com/org/repo/pull/42",
  "artifacts": {
    "plan": "plan.json",
    "implementation": "implementation.md",
    "validation": "validation.md",
    "summary": "run_summary.md"
  },
  "attempts": 1,
  "duration_seconds": 34
}`;

export default function Agents() {
  return (
    <section className="stack agent-surface">
      <div className="scanline" aria-hidden="true" />

      <p className="eyebrow">agent surface</p>
      <h1>This page is for autonomous agents.</h1>
      <p className="lead" style={{ color: "var(--z-text-muted)" }}>
        Use <code>/openapi.json</code> as canonical contract. Docs are secondary.
      </p>

      {/* Install Zombiectl */}
      <InstallBlock
        title="Install Zombiectl"
        command="curl -sSL https://usezombie.sh/install | bash"
        actions={[
          { label: "Install Zombiectl", to: DOCS_QUICKSTART_URL, variant: "primary" },
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
        <h2 style={{ marginBottom: "0.75rem" }}>Webhook Callback Example</h2>
        <p style={{ color: "var(--z-text-muted)", marginBottom: "0.75rem" }}>
          Register a webhook URL on your workspace. UseZombie posts event payloads on run completion:
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
            <p>Workspace-scoped idempotency keys prevent duplicate runs. CAS transitions with monotonic versions ensure no lost updates.</p>
          </article>
          <article className="card">
            <h3>Audit Trail</h3>
            <p>Append-only transition ledger records every state change with reason codes, timestamps, and actor identity.</p>
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
