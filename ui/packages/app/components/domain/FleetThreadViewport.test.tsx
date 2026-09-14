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

function View({ pendingMessageId = null, eventsCount = 0 }: {
  pendingMessageId?: string | null;
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
        pendingMessageId={pendingMessageId}
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

    view.rerender(<View pendingMessageId={FIRST_MESSAGE} eventsCount={1} />);
    expect(onScroll).toHaveBeenCalledExactlyOnceWith({ behavior: "instant" });

    view.rerender(<View pendingMessageId={FIRST_MESSAGE} eventsCount={2} />);
    expect(onScroll).toHaveBeenCalledTimes(1);

    view.rerender(<View eventsCount={2} />);
    expect(onScroll).toHaveBeenCalledTimes(1);

    view.rerender(<View pendingMessageId={SECOND_MESSAGE} eventsCount={3} />);
    expect(onScroll).toHaveBeenCalledTimes(2);
  });
});
