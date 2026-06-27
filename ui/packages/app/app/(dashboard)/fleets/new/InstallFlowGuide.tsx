import { DashboardPanel, SectionLabel } from "@agentsfleet/design-system";

const INSTALL_FLOW_STEPS = [
  {
    number: "01",
    title: "Choose template",
    description: "Start with a ready GitHub fleet.",
  },
  {
    number: "02",
    title: "Connect the tool",
    description: "Add the token before first run.",
  },
  {
    number: "03",
    title: "Watch it wake",
    description: "Live states appear inline.",
  },
] as const;

export function InstallFlowGuide() {
  return (
    <DashboardPanel padding="compact" className="space-y-md">
      <SectionLabel>Install flow</SectionLabel>
      <ol className="space-y-md">
        {INSTALL_FLOW_STEPS.map((step) => (
          <li key={step.number} className="flex gap-md">
            <span className="font-mono text-eyebrow text-pulse">{step.number}</span>
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
