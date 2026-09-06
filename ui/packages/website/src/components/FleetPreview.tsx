import { AgentIllustration } from "./AgentIllustration";
import { Badge, Card } from "@agentsfleet/design-system";
import { INTEGRATION_ICONS } from "../lib/marketing-copy";

const EVIDENCE_SOURCES = [
  { ...INTEGRATION_ICONS.grafana, detail: "Metrics & dashboards" },
  { ...INTEGRATION_ICONS.elasticsearch, detail: "Application logs" },
] as const;

export function FleetPreview() {
  return (
    <figure className="incident-illustration m-0" aria-label="Illustrated example of incident diagnosis and approval-gated repair">
      <figcaption className="incident-caption">
        <span>Your tools. A shared investigation.</span>
        <Badge>Illustrative example</Badge>
      </figcaption>
      <div className="incident-diagnosis">
        <div className="incident-sources">
          <p className="incident-label">01 / Gather evidence</p>
          {EVIDENCE_SOURCES.map((source) => (
            <div className="incident-source" key={source.label}>
              <ConnectorMark icon={source.icon} />
              <div><p className="m-0 font-medium">{source.label}</p><p className="m-0 text-body-sm text-text-muted">{source.detail}</p></div>
            </div>
          ))}
          <p className="m-0 text-label text-text-muted">Scheduled checks · read access</p>
        </div>
        <Investigation />
        <Card className="incident-slack flex flex-col gap-4">
          <div className="flex items-center gap-3"><ConnectorMark icon={INTEGRATION_ICONS.slack.icon} /><span className="font-medium">A diagnosis in Slack</span></div>
          <p className="m-0 text-body-sm text-text-muted">The error spike follows the latest deploy. Here is the evidence worth checking.</p>
          <div className="incident-evidence"><span>Metric window</span><span>Relevant logs</span><span>Recent change</span></div>
          <span className="text-label text-text-subtle">Example output, not a live incident</span>
        </Card>
      </div>
      <RepairBoundary />
    </figure>
  );
}

function ConnectorMark({ icon }: { icon: string }) {
  return <span className="connector-mark" style={{ maskImage: `url("${icon}")` }} aria-hidden="true" />;
}

function Investigation() {
  return (
    <div className="incident-investigation">
      <p className="incident-label">02 / Investigate</p>
      <AgentIllustration className="incident-agent" />
      <p className="m-0 font-medium">Incident responder</p>
      <p className="m-0 text-body-sm text-text-muted">Correlate symptoms.<br />Explain the findings.</p>
    </div>
  );
}

function RepairBoundary() {
  return (
    <div className="incident-repair">
      <div className="incident-handoff">
        <span className="font-mono text-label text-pulse">A separate, controlled step</span>
        <p className="m-0 text-body-sm text-text-muted">A human request or failed GitHub workflow can start repair. A diagnosis alone never starts it.</p>
      </div>
      <div className="incident-repair-steps">
        <div><span className="incident-label">03 / You approve</span><p>Repository write access</p></div>
        <span className="incident-arrow" aria-hidden="true">→</span>
        <div><span className="incident-label">04 / Repairer checks</span><p>Reread evidence. Bound the fix.</p></div>
        <span className="incident-arrow" aria-hidden="true">→</span>
        <div><span className="incident-label">05 / GitHub draft PR</span><p className="flex items-center gap-2"><ConnectorMark icon={INTEGRATION_ICONS.github.icon} />You review and merge.</p></div>
      </div>
      <p className="m-0 text-label text-text-muted">Some investigations end with diagnosis only. Fleets do not merge or deploy.</p>
    </div>
  );
}
