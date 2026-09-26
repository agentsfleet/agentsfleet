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
const onRetry = vi.fn();

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
        failureKind={null}
        onRetry={onRetry}
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
});
