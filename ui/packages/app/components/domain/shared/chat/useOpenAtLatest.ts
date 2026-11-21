import { useLayoutEffect, type RefObject } from "react";
import { useAui } from "@assistant-ui/react";

/** Within one pixel of the end counts as being at the latest turn. */
const AT_BOTTOM_PX = 1;

/**
 * Lands the thread on its latest turn as it opens, whether the turns came with
 * the mount or loaded just after; a first turn whose reply is running is live
 * and left alone. Settled rows lay out only near the viewport and move the
 * bottom when they do, so each resize of the log lands again until the thread
 * changes, the reader scrolls up or types. An explicit jump starts this again
 * so laid-out rows and text still being revealed cannot leave the end out of view.
 */
export function useOpenAtLatest(viewport: RefObject<HTMLDivElement | null>, log: RefObject<HTMLDivElement | null>, jumps = 0): void {
  const aui = useAui();
  useLayoutEffect(() => {
    const frame = viewport.current;
    const rows = log.current;
    if (!frame || !rows) return;
    const mounted = aui.thread().getState().messages;
    const latestAtJump = mounted.at(-1)?.id;
    let opened = mounted.length > 0 ? mounted : undefined;
    let landedTop = frame.scrollTop;
    const land = new ResizeObserver(() => {
      const {messages} = aui.thread().getState();
      const latest = messages.at(-1);
      if (!latest) return;
      if (opened === undefined && latest.status?.type === "running") return stop();
      opened ??= messages;
      if (messages !== opened && (!jumps || latest.id !== latestAtJump)) return stop();
      frame.scrollTo({top: frame.scrollHeight, behavior: "instant"});
      landedTop = frame.scrollTop;
    });
    const leave = () => {
      if (frame.scrollHeight - frame.clientHeight - frame.scrollTop > AT_BOTTOM_PX && (!jumps || frame.scrollTop < landedTop)) stop();
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
  }, [aui, viewport, log, jumps]);
}
