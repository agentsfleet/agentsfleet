import { useLayoutEffect, type RefObject } from "react";
import { useAui } from "@assistant-ui/react";

/** Within one pixel of the end counts as being at the latest turn. */
const AT_BOTTOM_PX = 1;

/**
 * Lands the thread on its latest turn as it opens, whether the turns came with
 * the mount or loaded just after; a first turn whose reply is running is live
 * and left alone. Settled rows lay out only near the viewport and move the
 * bottom when they do, so each resize of the log lands again until the thread
 * changes, the reader scrolls up or types. Nothing here runs after that.
 */
export function useOpenAtLatest(viewport: RefObject<HTMLDivElement | null>, log: RefObject<HTMLDivElement | null>): void {
  const aui = useAui();
  useLayoutEffect(() => {
    const frame = viewport.current;
    const rows = log.current;
    if (!frame || !rows) return;
    const mounted = aui.thread().getState().messages;
    let opened = mounted.length > 0 ? mounted : undefined;
    const land = new ResizeObserver(() => {
      const {messages} = aui.thread().getState();
      if (messages.length === 0) return;
      if (opened === undefined && messages.at(-1)?.status?.type === "running") return stop();
      opened ??= messages;
      if (messages === opened) frame.scrollTo({top: frame.scrollHeight, behavior: "instant"});
      else stop();
    });
    const leave = () => {
      if (frame.scrollHeight - frame.clientHeight - frame.scrollTop > AT_BOTTOM_PX) stop();
    };
    const stop = () => {
      land.disconnect();
      frame.removeEventListener("scroll", leave);
      frame.removeEventListener("input", stop);
    };
    land.observe(rows);
    frame.addEventListener("scroll", leave);
    frame.addEventListener("input", stop);
    return stop;
  }, [aui, viewport, log]);
}
