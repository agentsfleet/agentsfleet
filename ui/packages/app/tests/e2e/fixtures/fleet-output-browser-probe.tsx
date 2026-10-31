import { createRoot } from "react-dom/client";
import { FleetToolOutputDialog } from "../../../components/domain/FleetToolOutputDialog";

const root = document.getElementById("root");
if (!root) throw new Error("Output probe root is missing");
createRoot(root).render(<FleetToolOutputDialog
  scope={{ workspaceId: "ws_probe", fleetId: "fleet_probe" }} eventId="event_probe" callId="call:1"
  name="http_request" verb="Ran command" target="GET https://example.test/log"
  outcome={undefined} onClose={() => {}}
/>);
