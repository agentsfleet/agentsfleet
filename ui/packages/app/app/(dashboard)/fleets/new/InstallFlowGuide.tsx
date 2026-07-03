import { CheckCircle2Icon } from "lucide-react";
import { DashboardPanel, SectionLabel } from "@agentsfleet/design-system";

// The model + starter credit are already provisioned (platform default), so
// nothing here asks the user to paste a key. Each step carries a tick to read
// as "handled for you", and step 02 reflects the one-click connect reality
// rather than the old "add the token before first run" (which was never true —
// the platform default resolves the model server-side).
const INSTALL_FLOW_STEPS = [
  {
    id: "template",
    title: "Choose a template",
    description: "Start with a ready GitHub fleet.",
  },
  {
    id: "connect",
    title: "Connect the tool",
    description: "One click for GitHub or Slack — no token to paste.",
  },
  {
    id: "run",
    title: "It starts running",
    description: "Watch live progress appear inline.",
  },
] as const;

export function InstallFlowGuide() {
  return (
    <DashboardPanel padding="compact" className="space-y-md">
      <SectionLabel>How it works</SectionLabel>
      <ol className="space-y-md">
        {INSTALL_FLOW_STEPS.map((step) => (
          <li key={step.id} className="flex gap-md">
            <CheckCircle2Icon size={16} className="mt-0.5 shrink-0 text-pulse" aria-hidden="true" />
            <div>
              <h3 className="font-medium text-foreground">{step.title}</h3>
              <p className="text-body-sm leading-body-sm text-muted-foreground">
                {step.description}
              </p>
            </div>
          </li>
        ))}
      </ol>
    </DashboardPanel>
  );
}
