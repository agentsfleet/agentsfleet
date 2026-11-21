import { useLayoutEffect, type RefObject } from "react";
import { useAui } from "@assistant-ui/react";

/**
 * Lands the thread on its latest turn as it opens, whether the turns came with
 * the mount or loaded just after; a first turn whose reply is running is live
 * and left alone. Settled rows lay out only near the viewport and move the
 * bottom when they do, so each resize of the log lands again until the thread
 * changes or the reader interacts with it. An explicit jump starts this again
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
    const interaction = new AbortController();
    const land = new ResizeObserver(() => {
      const {messages} = aui.thread().getState();
      const latest = messages.at(-1);
      if (!latest) return;
      if (opened === undefined && latest.status?.type === "running") return stop();
      opened ??= messages;
      if (messages !== opened && (!jumps || latest.id !== latestAtJump)) return stop();
      frame.scrollTo({top: frame.scrollHeight, behavior: "instant"});
    });
    const stop = () => {
      land.disconnect();
      interaction.abort();
    };
    land.observe(rows);
    // The browser also emits scroll events when deferred history adjusts the anchor.
    const listenerOptions = {passive: true, signal: interaction.signal};
    for (const event of ["pointerdown", "wheel", "keydown", "input"]) {
      frame.addEventListener(event, stop, listenerOptions);
    }
    rows.addEventListener("focusin", stop, listenerOptions);
    return stop;
  }, [aui, viewport, log, jumps]);
}
