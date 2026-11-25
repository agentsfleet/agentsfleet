import { Terminal } from "@agentsfleet/design-system";
import { trackInteractionClicked } from "../analytics/posthog";

const BOOTSTRAP = `agentsfleet login
agentsfleet fleet create "Check my latest deploy and report what you find." --name deploy-watch
agentsfleet fleet steer @deploy-watch`;

export function CliQuickstart() {
  return (
    <Terminal
      label="Fleet commands"
      className="[&_pre]:whitespace-pre-wrap"
      copyable
      onCopyClick={() =>
        trackInteractionClicked({
          source: "cli_quickstart_copy",
          surface: "cli_quickstart",
          target: "fleet_create",
        })
      }
    >
      {BOOTSTRAP}
    </Terminal>
  );
}
