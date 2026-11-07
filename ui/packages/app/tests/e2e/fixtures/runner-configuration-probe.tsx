import { createRoot } from "react-dom/client";
import { useState } from "react";
import { Badge, PageTitle, Section, Separator } from "@agentsfleet/design-system";
import { EditPolicyDialog } from "@/app/(dashboard)/admin/runners/components/EditPolicyDialog";
import type { AssignedPolicy } from "@/lib/api/runners-types";
import { JEV_MODEL } from "@/tests/helpers/catalogue-fixtures";

const INITIAL: AssignedPolicy = {
  decision_model_id: JEV_MODEL.id,
  durable_tasks_enabled: true, sandbox_tier: "landlock_full", network_policy: "allow_all",
  registry_allowlist: ["registry.npmjs.org", "pypi.org"], worker_count: 16,
  extra_binds: [{path: "/srv/models", mode: "read_only", note: "Shared model weights"}],
};

function RunnerConfigurationProbe() {
  const [saved, setSaved] = useState(false);
  return <main className="min-h-dvh bg-background p-xl font-sans text-foreground sm:p-3xl">
    <div className="mb-2xl flex flex-wrap items-center justify-between gap-lg">
      <div className="space-y-sm"><PageTitle>Build runner</PageTitle>
        <p className="text-body-sm text-muted-foreground">Runner details</p></div>
      <EditPolicyDialog runnerId="preview-runner" current={INITIAL} currentLabels={["linux", "eu-west"]}
        onSaved={() => setSaved(true)} />
    </div>
    <Separator />
    <Section aria-label="Runner state" className="max-w-form py-2xl">
      <div className="flex items-center gap-md"><Badge variant="green">Online</Badge><span>Ready for fleet work</span></div>
      <p className="text-body-sm text-muted-foreground">16 workers · Landlock isolation · Durable tasks enabled</p>
      {saved ? <output>Runner configuration saved.</output> : null}
    </Section>
  </main>;
}

const root = document.getElementById("root");
if (!root) throw new Error("Runner fixture root is missing");
createRoot(root).render(<RunnerConfigurationProbe />);
