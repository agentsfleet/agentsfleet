import { createRef } from "react";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StoryFrame } from "../lib/chat-story";
import { useChatScroll } from "./use-chat-scroll";

const FRAME: StoryFrame = { entries: [], pending: null, finished: false, cycle: 0 };
const NEXT_FRAME = { ...FRAME, cycle: 1 };
const SCROLL_HEIGHT = 1000;
const VIEWPORT_HEIGHT = 400;
const FOLLOW_DISTANCE = 24;
const HISTORY_TOP = 200;
const SCROLL_BEHAVIOR = "smooth";

function scrollRegion() {
  const ref = createRef<HTMLDivElement>();
  const element = document.createElement("div");
  Object.defineProperties(element, {
    scrollHeight: { value: SCROLL_HEIGHT },
    clientHeight: { value: VIEWPORT_HEIGHT },
  });
  element.scrollTo = vi.fn();
  ref.current = element;
  return { ref, element };
}

describe("Conversation scroll following", () => {
  it("follows new output only while the reader stays near the latest message", () => {
    const { ref, element } = scrollRegion();
    const { result, rerender } = renderHook(({ frame, running }) => useChatScroll(ref, frame, running), {
      initialProps: { frame: FRAME, running: true },
    });
    expect(element.scrollTop).toBe(SCROLL_HEIGHT);
    element.scrollTop = SCROLL_HEIGHT - VIEWPORT_HEIGHT - FOLLOW_DISTANCE;
    act(() => result.current.onScroll());
    expect(result.current.following).toBe(true);
    element.scrollTop -= 1;
    act(() => result.current.onScroll());
    expect(result.current.following).toBe(false);
    const position = element.scrollTop;
    rerender({ frame: NEXT_FRAME, running: true });
    expect(element.scrollTop).toBe(position);
    rerender({ frame: NEXT_FRAME, running: false });
    act(() => result.current.onScroll());
    rerender({ frame: NEXT_FRAME, running: true });
    expect(element.scrollTop).toBe(position);
    act(() => result.current.jump());
    expect(element.scrollTo).toHaveBeenCalledWith({ top: SCROLL_HEIGHT, behavior: SCROLL_BEHAVIOR });
    expect(result.current.following).toBe(true);
    expect(element.scrollTop).toBe(SCROLL_HEIGHT);
  });

  it("does not move or change following while playback is paused", () => {
    const { ref, element } = scrollRegion();
    element.scrollTop = HISTORY_TOP;
    const { result } = renderHook(() => useChatScroll(ref, FRAME, false));
    act(() => result.current.onScroll());
    expect(element.scrollTop).toBe(HISTORY_TOP);
    expect(result.current.following).toBe(true);
  });

  it("tolerates an absent or detached scroll region", () => {
    const ref = createRef<HTMLDivElement>();
    const { result, rerender } = renderHook(({ frame }) => useChatScroll(ref, frame, true), {
      initialProps: { frame: FRAME },
    });
    act(() => { result.current.onScroll(); result.current.jump(); });
    expect(result.current.following).toBe(true);
    const { element } = scrollRegion();
    ref.current = element;
    rerender({ frame: NEXT_FRAME });
    element.scrollTop = HISTORY_TOP;
    act(() => result.current.onScroll());
    expect(result.current.following).toBe(false);
    ref.current = null;
    act(() => result.current.onScroll());
    expect(result.current.following).toBe(false);
    act(() => result.current.jump());
    expect(result.current.following).toBe(true);
    expect(element.scrollTo).not.toHaveBeenCalled();
  });
});
