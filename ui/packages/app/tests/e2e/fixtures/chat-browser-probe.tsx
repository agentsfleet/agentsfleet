import { ChatAlignmentFixture, FleetSampleFixture, LoadingReplyFixture } from "./chat-alignment-probe";
import { ChatAddressFixture, FleetSteerRowFixture, FleetStopFixture } from "./chat-address-probe";
import {ThemeProvider} from "next-themes";
import { createRoot } from "react-dom/client";
import { ActivityFixture } from "./chat-activity-probe";
import { FleetActivityRow, SenderLabelProvider } from "@/components/domain/activity/messages/FleetMessageRow";
import { ChatFailure } from "@/components/domain/shared/chat/ChatFailure";
import { DurableTaskFixture } from "./task-browser-probe";
import { ChatScrollFixture } from "./chat-scroll-probe";
import { ScheduleFixture } from "./schedule-browser-probe";
import { RefreshVisualFixture } from "./refresh-visual-probe";
import { AccountSettingsFixture } from "./account-settings-probe";
import { ResourceSettingsFixture } from "./resource-settings-probe";
import { ResourcePagesFixture } from "./resource-pages-probe";
import { UsageBrowserFixture } from "./usage-browser-probe";
import { NewFleetFixture } from "./new-fleet-probe";

const root = document.getElementById("root");
if (!root) throw new Error("Chat fixture root is missing");
const query = new URLSearchParams(location.search);
const MESSAGE_CREATED_AT = new Date("2026-11-01T13:00:00Z");
createRoot(root).render(<ThemeProvider attribute="data-theme" defaultTheme="dark" enableSystem={false}>{query.has("address") ? <ChatAddressFixture /> : query.has("fleetSteer") ? <FleetSteerRowFixture report={query.get("fleetSteer") === "report"} /> : query.has("fleetStop") ? <FleetStopFixture /> : query.has("usage") ? <UsageBrowserFixture /> : query.has("resourcePages") ? <ResourcePagesFixture /> : query.has("resources") ? <ResourceSettingsFixture /> : query.has("accountSettings") ? <AccountSettingsFixture /> : query.has("visual") ? <RefreshVisualFixture /> : (query.has("newFleet") || query.has("research")) ? <NewFleetFixture research={query.has("research")} /> : query.has("alignment") ? <ChatAlignmentFixture /> : query.has("scroll") ? <ChatScrollFixture /> : query.has("schedules") ? <ScheduleFixture /> : query.has("durable_tasks") ? <DurableTaskFixture /> : query.has("messages") ? <MessagesFixture /> : query.has("activity") ? <ActivityFixture /> : query.has("loading") ? <LoadingReplyFixture state={query.get("loading") ?? "running"} /> : <FleetSampleFixture />}</ThemeProvider>);

/** An incoming message and a failed request, as the fleet chat shows them. */
function MessagesFixture() {
  return <SenderLabelProvider senderLabel="ci-responder"><main className="min-h-dvh space-y-md bg-background p-lg text-foreground">
    <FleetActivityRow sender="Slack" headline="Message received" createdAt={MESSAGE_CREATED_AT} messageRole="system" detailsLabel="View message">
      <p>Diagnose the failed title regression using the current source.</p>
    </FleetActivityRow>
    <ChatFailure title="Couldn’t complete this request" description="The response was interrupted."
      details={<p>The connection to the model provider was lost mid-turn.</p>} />
  </main></SenderLabelProvider>;
}
