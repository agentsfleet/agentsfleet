import { createRoot } from "react-dom/client";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE, MODEL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { ActivityFixture } from "./chat-activity-probe";
import { SetupChecks } from "@/components/domain/chat/SetupChecks";
import { FleetActivityRow, SenderLabelProvider } from "@/components/domain/FleetMessageRow";
import { ChatFailure } from "@/components/domain/chat/ChatFailure";

const client = new DraftClient({account: "fixture-owner", workspace: WORKSPACE, subject: "creator"});
const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, model: MODEL.ref, models: [MODEL], messages: [
  {...MESSAGE, text: "Investigate the failing test and explain the evidence."},
  {...MESSAGE, sequence: "2", role: "assistant" as const,
    text: "The request has an owner. Switching conversations cancels its stream.\n\n- Stop closes the active reader.\n- Retry preserves the request identity.\n- The evidence stays with this conversation."},
]};
const root = document.getElementById("root");
if (!root) throw new Error("Chat fixture root is missing");
const query = new URLSearchParams(location.search);
const SETUP_CREATED_AT = new Date("2026-11-01T13:00:00Z");
createRoot(root).render(query.has("setup") ? <SetupFixture /> : query.has("activity") ? <ActivityFixture /> : <main className="flex h-dvh flex-col bg-background text-foreground">
  <p className="px-xl py-sm text-eyebrow text-muted-foreground">Typography fixture · deterministic conversation</p>
  <DraftConversation client={client} view={view} workspace={WORKSPACE} />
</main>);
window.addEventListener("pagehide", () => client.dispose(), {once: true});

function SetupFixture() {
  const checks = [{kind: "model", subject: "platform-default", state: "ready"},
    {kind: "credentials", subject: "github", state: "ready"}, {kind: "connections", subject: "github", state: "ready"},
    {kind: "grants", subject: "github", state: "ready"}, {kind: "credentials", subject: "slack", state: "ready"},
    {kind: "connections", subject: "slack", state: "ready"}];
  return <SenderLabelProvider senderLabel="ci-responder"><main className="min-h-dvh space-y-md bg-background p-lg text-foreground">
    <h1>Setup saved · v2</h1><SetupChecks checks={checks} label="Setup checks" />
    <FleetActivityRow sender="Slack" headline="Message received" createdAt={SETUP_CREATED_AT} messageRole="system" detailsLabel="View message">
      <p>Diagnose the failed title regression using the current source.</p>
    </FleetActivityRow>
    <ChatFailure title="Couldn’t complete this request" description="The response was interrupted."
      details={<p>The connection to the model provider was lost mid-turn.</p>} />
  </main></SenderLabelProvider>;
}
