import type { ReactNode } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { FleetConversationLayout } from "../FleetWorkSidebar";
import { ChatThread } from "./ChatThread";

const NO_MESSAGES: ThreadMessageLike[] = [];

afterEach(cleanup);

it("keeps Summary inline and preserves the composer while recorded work changes", () => {
  const workspace = (status: string) => <Conversation work={<p>{status}</p>} />;
  const view = render(workspace("Running checks"));
  const draft = screen.getByRole("textbox", {name: "Draft"});
  draft.focus();
  fireEvent.change(draft, {target: {value: "Keep my exact direction"}});
  expect(screen.getByRole("complementary", {name: "Summary"}).textContent).toContain("Running checks");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
  view.rerender(workspace("Checks passed"));
  expect(screen.getAllByRole("complementary")).toHaveLength(1);
  expect(screen.getByRole("textbox")).toBe(draft);
  expect(draft).toHaveProperty("value", "Keep my exact direction");
  expect(document.activeElement).toBe(draft);
});

it("omits an absent work reader without hiding the conversation", () => {
  render(<Conversation />);
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(screen.getByRole("textbox")).toBeTruthy();
});

function Conversation({work}: {work?: ReactNode}) {
  const runtime = useExternalStoreRuntime({messages: NO_MESSAGES, isRunning: false, convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  return <AssistantRuntimeProvider runtime={runtime}><FleetConversationLayout work={work}>
    <ChatThread label="Conversation" autoScroll={false} footer={<textarea aria-label="Draft" defaultValue="Keep my draft" />}><p>Conversation history</p></ChatThread>
  </FleetConversationLayout></AssistantRuntimeProvider>;
}
