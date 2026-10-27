import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { ReactNode } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

const NO_MESSAGES: ThreadMessageLike[] = [];
// An operator's turn with its reply still running, after an earlier one: the
// library never anchors the thread's first message.
const RUNNING_TURN: ThreadMessageLike[] = [
  { id: "turn-0", role: "user", content: "check the build" },
  { id: "turn-0:reply", role: "assistant", content: "Green." },
  { id: "turn-1", role: "user", content: "deploy the canary" },
  { id: "turn-1:reply", role: "assistant", content: "" },
];
const TOP_ANCHOR_USER = "[data-aui-top-anchor-user]";

afterEach(() => cleanup());

function View({ eventsCount = 0, messages = NO_MESSAGES, isRunning = false, connectionStatus = CONNECTION_STATUS.LIVE, setup, setupEventId }: {
  eventsCount?: number;
  messages?: ThreadMessageLike[];
  isRunning?: boolean;
  connectionStatus?: ConnectionStatus;
  setup?: ReactNode;
  setupEventId?: string;
}) {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({
    messages,
    isRunning,
    convertMessage: (message) => message,
    onNew: async () => {},
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetThreadViewport
        setup={setup} setupEventId={setupEventId}
        eventsCount={eventsCount}
        connectionStatus={connectionStatus}
        onRetry={() => {}}
        pending={[]}
        onResend={() => {}}
        onDismiss={() => {}}
        onRestored={() => {}}
        onDraft={() => {}}
      />
    </AssistantRuntimeProvider>
  );
}

describe("FleetThreadViewport scroll", () => {
  // jsdom does no layout, so the held view itself is proven in
  // fleet-thread-anchor.spec.ts. This pins the mode that holds it: while a
  // reply runs, the operator's newest row is the library's top anchor.
  it("anchors the operator's newest row while its reply runs", () => {
    const view = render(<View messages={RUNNING_TURN} />);
    expect(view.container.querySelector(TOP_ANCHOR_USER)).toBeNull();

    view.rerender(<View messages={RUNNING_TURN} isRunning />);
    const anchor = view.container.querySelector(TOP_ANCHOR_USER);
    expect(anchor?.textContent).toContain("deploy the canary");
  });
});

describe("FleetThreadViewport offline notice", () => {
  // jsdom does no layout, so the held transcript is proven in
  // fleet-thread-anchor.spec.ts. This pins where the notice lives: in the
  // sticky footer's flow, above the composer. Above the thread it moved every
  // row; laid over the history it hid the newest reply from a reader at the
  // bottom.
  it("warns from the composer's footer, in its flow", () => {
    const view = render(<View connectionStatus={CONNECTION_STATUS.OFFLINE} />);
    const notice = view.getByTestId("fleet-connection-notice");
    const footer = view.getByTestId("fleet-chat-footer");
    expect(notice.parentElement).toBe(footer);
    expect(notice.closest(".absolute")).toBeNull();
    const composer = view.getByRole("form", { name: "Chat composer" });
    expect(notice.compareDocumentPosition(composer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe("FleetThreadViewport layout", () => {
  // A footer outside the scrolling viewport floated over a blank band and left
  // Send below the clipped root. Where it sits on screen is measured in the
  // browser (fleet-thread.spec.ts); here the structure holds it inside.
  it("keeps the composer footer inside the thread viewport", () => {
    const view = render(<View />);
    const footerElement = view.getByTestId("fleet-chat-footer");
    expect(view.getByTestId("fleet-thread-root").querySelector('[role="presentation"]')?.contains(footerElement)).toBe(true);
  });
  it("places the saved setup inside the log once, with the composer still available", () => {
    const view = render(<View setup={<h2>Setup saved · v1</h2>} />);
    expect(view.getByRole("log").contains(view.getByRole("heading", {name: "Setup saved · v1"}))).toBe(true);
    expect(view.queryAllByText("Setup saved · v1")).toHaveLength(1);
    expect(view.queryByText("Saved v1")).toBeNull();
    expect(view.getByRole("form", {name: "Chat composer"})).toBeTruthy();
  });
  it("replaces the saved receipt in sequence without repeating it above later messages", () => {
    const messages: ThreadMessageLike[] = [
      {id: "before", role: "user", content: "Save this setup"},
      {id: "saved", role: "system", content: "Setup saved · v1"},
      {id: "after", role: "user", content: "Check performance"},
    ];
    const view = render(<View messages={messages} eventsCount={3} setupEventId="saved" setup={<h2>Setup saved · v1</h2>} />);
    const history = view.getByRole("log").textContent ?? "";
    expect(history.indexOf("Save this setup")).toBeLessThan(history.indexOf("Setup saved · v1"));
    expect(history.indexOf("Setup saved · v1")).toBeLessThan(history.indexOf("Check performance"));
    expect(view.getAllByText("Setup saved · v1")).toHaveLength(1);
  });
});
