import type { Message } from "@/lib/api/draft-types";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import Markdown from "react-markdown";
import { useAuiState } from "@assistant-ui/react";
import type { ComponentProps } from "react";
import { ChatFixture, CHAT_PROPS } from "./workspace-chat/fixture";
import { DraftConversation } from "@/components/domain/setup/conversation/DraftConversation";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { DraftClient } from "@/lib/api/drafts";
import { DRAFT_DELTA_EVENT, DRAFT_EVENT } from "@/lib/api/draft-events";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE, TURN_ID, WORKSPACE } from "./helpers/draft-fixtures";
import { FakeEventSource } from "./helpers/fake-event-source";

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
afterEach(() => {cleanup(); vi.unstubAllGlobals(); vi.clearAllMocks();});

it("does not parse settled history again while ten answer updates render", async () => {
  const client = new DraftClient({account: "owner", workspace: WORKSPACE, subject: "creator"});
  const history: Message[] = Array.from({length: 20}, (_, index) => ({...MESSAGE, sequence: String(index + 1),
    role: index % 2 ? "assistant" : "user", text: `Settled message ${index + 1}`}));
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, model: DRAFT.setup_model_ref,
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

it("sends twenty New fleet turns without repainting settled answers or reading on text deltas", async () => {
  const fixture = new ChatFixture();
  fixture.mount();
  const fetch = fixture.fetch.getMockImplementation()!;
  let settled: Message[] = [];
  fixture.fetch.mockImplementation(async (input, init) => {
    const response = await fetch(input, init);
    if (init?.method !== "POST") return response;
    const user = fixture.messages.at(-2)!;
    const answer = {...fixture.messages.at(-1)!, text: `Settled setup reply ${user.sequence}`};
    settled = [...fixture.messages.slice(0, -2), user, answer];
    fixture.messages = [...settled.slice(0, -2), {...user, state: "running"}];
    return response;
  });
  const paints = (prior: number) => renders.assistant.mock.calls.filter(([text]) => {
    const match = /^Settled setup reply (\d+)$/.exec(text);
    return match && Number(match[1]) <= prior;
  }).length;
  render(<WorkspaceChat {...CHAT_PROPS} />);

  for (let turn = 1; turn <= 20; turn += 1) {
    const previousPaints = paints(turn - 1);
    const input = await screen.findByRole("textbox", {name: CHAT_PLACEHOLDER});
    fireEvent.change(input, {target: {value: `Setup request ${turn}`}});
    const send = screen.getByRole("button", {name: turn === 1 ? "New fleet from message" : "Send message"});
    await waitFor(() => expect(send).toHaveProperty("disabled", false));
    fireEvent.click(send);
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(turn));
    const stream = FakeEventSource.instances.at(-1)!;
    await act(async () => stream.open());

    const readsBeforeDelta = fixture.calls.length;
    const delta = `Setup preview ${turn}`;
    await act(async () => stream.emitNamed(DRAFT_DELTA_EVENT, JSON.stringify({
      draft_id: DRAFT.id, sequence: String(turn), stream_id: TURN_ID, stream_seq: 1,
      stream_start: true, text_kind: "answer", text: delta,
    })));
    await screen.findByText(delta);
    expect(fixture.calls).toHaveLength(readsBeforeDelta);
    expect(paints(turn - 1)).toBe(previousPaints);

    fixture.messages = settled;
    await act(async () => stream.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT.id, sequence: String(turn)})));
    await screen.findByText(`Settled setup reply ${turn}`);
    await waitFor(() => expect(stream.closed).toBe(true));
    expect(paints(turn - 1)).toBe(previousPaints);
  }

  expect(fixture.writes()).toHaveLength(20);
  expect(screen.getAllByText(/^Settled setup reply /)).toHaveLength(20);
});
