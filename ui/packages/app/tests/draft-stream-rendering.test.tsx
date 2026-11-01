import type { Message } from "@/lib/api/draft-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import Markdown from "react-markdown";
import { useAuiState } from "@assistant-ui/react";
import type { ComponentProps } from "react";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE, WORKSPACE } from "./helpers/draft-fixtures";

const renders = vi.hoisted(() => ({assistant: vi.fn<(text: string) => void>()}));
vi.mock("@assistant-ui/react-markdown", async importOriginal => {
  const original = await importOriginal<typeof import("@assistant-ui/react-markdown")>();
  return {...original, MarkdownTextPrimitive: function TrackedMarkdown(props: ComponentProps<typeof original.MarkdownTextPrimitive>) {
    const text = useAuiState(state => state.message.content.filter(part => part.type === "text").map(part => part.text).join("\n"));
    renders.assistant(text);
    return <original.MarkdownTextPrimitive {...props} />;
  }};
});
vi.mock("react-markdown", async importOriginal => {
  const original = await importOriginal<typeof import("react-markdown")>();
  return {...original, default: vi.fn(original.default)};
});
afterEach(() => {cleanup(); vi.clearAllMocks();});

it("does not parse settled history again while ten answer updates render", async () => {
  const client = new DraftClient({account: "owner", workspace: WORKSPACE, subject: "creator"});
  const history: Message[] = Array.from({length: 20}, (_, index) => ({...MESSAGE, sequence: String(index + 1),
    role: index % 2 ? "assistant" : "user", text: `Settled message ${index + 1}`}));
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, model: DRAFT.authoring_model_ref,
    messages: [...history, {...MESSAGE, sequence: "21", state: "running" as const}]};
  const mounted = render(<DraftConversation client={client} view={view} workspace={WORKSPACE} />);
  await screen.findByText("Settled message 20");
  const calls = () => vi.mocked(Markdown).mock.calls.filter(([props]) => typeof props.children === "string" && props.children.startsWith("Settled message ")).length
    + renders.assistant.mock.calls.filter(([text]) => text.startsWith("Settled message ")).length;
  const initial = calls();
  expect(initial).toBe(20);
  try {
    for (let update = 1; update <= 10; update++) {
      const text = `Current answer update ${update}`;
      mounted.rerender(<DraftConversation client={client} view={{...view, liveReply: {sequence: "21", stream: "current", text, gapped: false}}} workspace={WORKSPACE} />);
      await screen.findByText(text);
    }
    expect(calls() - initial).toBe(0);
    expect(screen.getAllByText(/^Settled message /)).toHaveLength(20);
  } finally {client.dispose();}
});
