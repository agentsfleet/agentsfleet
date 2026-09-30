import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
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

function View({ eventsCount = 0, messages = NO_MESSAGES, isRunning = false, connectionStatus = CONNECTION_STATUS.LIVE }: {
  eventsCount?: number;
  messages?: ThreadMessageLike[];
  isRunning?: boolean;
  connectionStatus?: ConnectionStatus;
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
  // fleet-thread-anchor.spec.ts. This pins where the notice lives: laid over
  // the history from inside the footer, never in the flow above the thread.
  it("warns over the history from the composer's footer", () => {
    const view = render(<View connectionStatus={CONNECTION_STATUS.OFFLINE} />);
    const notice = view.getByTestId("fleet-connection-notice");
    const footer = view.getByTestId("fleet-chat-footer");
    expect(footer.contains(notice)).toBe(true);
    const overlay = notice.closest(".absolute");
    expect(overlay?.parentElement).toBe(footer);
    expect([...(overlay?.classList ?? [])]).toEqual(expect.arrayContaining(["bottom-full", "inset-x-0"]));
  });
});

describe("FleetThreadViewport layout", () => {
  // Pin test: jsdom does no layout, so the scroll itself cannot be reproduced
  // here. Live, a long streamed reply left the hidden-overflow root at
  // scrollTop 173 and the composer 181 px above the panel floor; a clipped
  // root cannot scroll at all, and only the inner viewport may.
  it("keeps the thread root unscrollable so the composer stays on the floor", () => {
    const view = render(<View />);
    const root = view.getByTestId("fleet-thread-root");
    expect(root.className).toContain("overflow-clip");
    expect(root.className).not.toContain("overflow-hidden");
  });

  // Pin test for assistant-ui's composer placement. A footer outside the
  // viewport floated over a blank band and, in a panel shorter than the
  // composer, left Send below the clipped root. Inside the viewport, stuck to
  // its bottom and capped at its height, the footer keeps Send in view; the
  // composer must never scroll itself, or anchoring hides the failure notice.
  it("keeps the composer in the viewport footer so Send stays in view", () => {
    const view = render(<View />);
    const footerElement = view.getByTestId("fleet-chat-footer");
    expect(view.getByTestId("fleet-thread-root").querySelector('[role="presentation"]')?.contains(footerElement)).toBe(true);
    const footer = [...footerElement.classList];
    expect(footer).toEqual(expect.arrayContaining(["sticky", "bottom-0", "max-h-full", "flex-col"]));
    const composer = [...view.getByRole("form", { name: "Chat composer" }).classList];
    expect(composer).toContain("min-h-0");
    expect(composer).not.toContain("overflow-y-auto");
    const textbox = view.getByRole("textbox");
    const row = [...(textbox.parentElement?.classList ?? [])];
    expect(row).toContain("items-stretch");
    expect(row).not.toContain("items-end");
    // A stretched textarea starts its text at the top, so on touch, where Send
    // is 44 px, the row floor and textarea padding must grow to keep one line
    // level with the arrow (measured 7.2 px high without them).
    expect(row).toContain("pointer-coarse:min-h-11");
    expect([...textbox.classList]).toContain("pointer-coarse:py-md");
  });
});
