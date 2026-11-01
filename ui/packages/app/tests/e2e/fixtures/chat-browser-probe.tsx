import { createRoot } from "react-dom/client";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE, MODEL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { ActivityFixture } from "./chat-activity-probe";

const client = new DraftClient({account: "fixture-owner", workspace: WORKSPACE, subject: "creator"});
const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, model: MODEL.ref, models: [MODEL], messages: [
  {...MESSAGE, text: "Investigate the failing test and explain the evidence."},
  {...MESSAGE, sequence: "2", role: "assistant" as const,
    text: "The request has an owner. Switching conversations cancels its stream.\n\n- Stop closes the active reader.\n- Retry preserves the request identity.\n- The evidence stays with this conversation."},
]};
const root = document.getElementById("root");
if (!root) throw new Error("Chat fixture root is missing");
createRoot(root).render(new URLSearchParams(location.search).has("activity") ? <ActivityFixture /> : <main className="flex h-dvh flex-col bg-background text-foreground">
  <p className="px-xl py-sm text-eyebrow text-muted-foreground">Typography fixture · deterministic conversation</p>
  <DraftConversation client={client} view={view} workspace={WORKSPACE} />
</main>);
window.addEventListener("pagehide", () => client.dispose(), {once: true});
