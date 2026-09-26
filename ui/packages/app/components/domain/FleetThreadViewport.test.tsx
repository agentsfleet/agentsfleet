import { useLayoutEffect } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  useThreadViewportStore,
} from "@assistant-ui/react";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { CONNECTION_STATUS } from "./useFleetEventStream";

const FIRST_MESSAGE = "optim-first";
const SECOND_MESSAGE = "optim-second";
const onScroll = vi.fn();

afterEach(() => {
  cleanup();
  onScroll.mockClear();
});

function ScrollObserver() {
  const viewport = useThreadViewportStore();
  useLayoutEffect(() => viewport.getState().onScrollToBottom(onScroll), [viewport]);
  return null;
}

function View({ submittedMessageId = null, eventsCount = 0 }: {
  submittedMessageId?: string | null;
  eventsCount?: number;
}) {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({
    messages: [],
    convertMessage: (message) => message,
    onNew: async () => {},
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <ScrollObserver />
      <FleetThreadViewport
        submittedMessageId={submittedMessageId}
        eventsCount={eventsCount}
        connectionStatus={CONNECTION_STATUS.LIVE}
        failure={null}
      />
    </AssistantRuntimeProvider>
  );
}

describe("FleetThreadViewport scroll intent", () => {
  it("follows each new submission once without pulling the reader on background updates", () => {
    const view = render(<View />);
    expect(onScroll).not.toHaveBeenCalled();

    view.rerender(<View submittedMessageId={FIRST_MESSAGE} eventsCount={1} />);
    expect(onScroll).toHaveBeenCalledExactlyOnceWith({ behavior: "instant" });

    view.rerender(<View submittedMessageId={FIRST_MESSAGE} eventsCount={2} />);
    expect(onScroll).toHaveBeenCalledTimes(1);

    view.rerender(<View eventsCount={2} />);
    expect(onScroll).toHaveBeenCalledTimes(1);

    view.rerender(<View submittedMessageId={SECOND_MESSAGE} eventsCount={3} />);
    expect(onScroll).toHaveBeenCalledTimes(2);
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
