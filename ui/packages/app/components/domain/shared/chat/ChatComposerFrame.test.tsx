import type { ThreadMessageLike } from "@assistant-ui/react";
import { useRef } from "react";
import { AssistantRuntimeProvider, useExternalStoreRuntime } from "@assistant-ui/react";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Button } from "@agentsfleet/design-system";
import { ChatComposerFrame, ChatComposerHint, ChatComposerInput } from "./ChatComposerFrame";

afterEach(cleanup);

function Composer({disabled = false}: {disabled?: boolean}) {
  const runtime = useExternalStoreRuntime({messages: [] as ThreadMessageLike[], convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  const input = useRef<HTMLTextAreaElement>(null);
  return <AssistantRuntimeProvider runtime={runtime}>
    <ChatComposerFrame label="Chat composer" inputRef={input} disabled={disabled} toolbar={<Button>Model</Button>} actions={<Button>Send</Button>}
      input={<ChatComposerInput inputRef={input} placeholder="Message your fleet…" disabled={disabled} />} />
    <ChatComposerHint />
  </AssistantRuntimeProvider>;
}
const field = () => screen.getByRole("textbox", {name: "Message your fleet…"});

it("takes focus when it mounts on an idle page, and names Enter and Shift + Enter", () => {
  render(<Composer />);
  expect(document.activeElement).toBe(field());
  expect(screen.getByText("Enter to send · Shift + Enter for a new line")).toBeTruthy();
});

it("leaves an open menu's item focused when it mounts", () => {
  render(<div role="menu"><Button role="menuitem">Create workspace</Button></div>);
  const item = screen.getByRole("menuitem", {name: "Create workspace"});
  item.focus();
  render(<Composer />);
  expect(document.activeElement).toBe(item);
});

it("waits until it is enabled, then takes focus from the page but not from another control", () => {
  const view = render(<Composer disabled />);
  expect(document.activeElement).not.toBe(field());
  view.rerender(<Composer />);
  expect(document.activeElement).toBe(field());
  view.rerender(<Composer disabled />);
  const other = render(<Button>Inspect answer</Button>);
  screen.getByRole("button", {name: "Inspect answer"}).focus();
  view.rerender(<Composer />);
  expect(document.activeElement).toBe(screen.getByRole("button", {name: "Inspect answer"}));
  other.unmount();
});
